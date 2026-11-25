//! The managed environment: the place relays will run, kept apart from the
//! desktop.
//!
//! Three things are kept here. The image is a WSL distro on Windows; this
//! build can use one that exists and prints how a dedicated one would be
//! made, but does not make it. The recorded paths are the folders the image
//! may see. A work is a git worktree on its own branch, with snapshots that
//! a bad run can be rolled back to.
//!
//! When a distro is chosen, every git command runs inside it, so a worktree
//! is made by the same git that a relayed tool will use. Git records
//! absolute paths in a worktree, and those differ on the two sides.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

const FILE: &str = "environment.json";

/// The tools looked for inside the image.
const TOOLS: &[&str] = &["git", "claude", "codex", "agent", "gemini"];

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Environment {
    /// The WSL distro relays run in. None means this machine itself.
    pub distro: Option<String>,
    /// Folders the image may see, as paths on this machine.
    pub mounts: Vec<String>,
    pub works: Vec<Work>,
}

/// One git worktree on its own branch, made for a route.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Work {
    pub name: String,
    /// The repository it was cut from. This is the recorded path it uses.
    pub repo: String,
    pub path: String,
    pub branch: String,
    /// The commit it started from.
    pub base: String,
    pub snapshots: Vec<Snapshot>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Snapshot {
    pub number: u32,
    pub commit: String,
    pub at_ms: u64,
    pub note: String,
}

/// A path on this machine as the image sees it. A Windows drive path maps
/// to WSL's `/mnt/<drive>`; a path that already starts with `/` is kept.
/// Network paths have no fixed place inside, so they give None.
pub fn inside(path: &str) -> Option<String> {
    let bytes = path.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        let rest = path[2..].replace('\\', "/");
        let rest = rest.trim_end_matches('/');
        let drive = (bytes[0] as char).to_ascii_lowercase();
        return Some(format!("/mnt/{drive}{rest}"));
    }
    if path.starts_with('/') && !path.starts_with("//") {
        return Some(path.to_string());
    }
    None
}

/// A path in one spelling, for comparing. Drive paths ignore case, as
/// Windows does.
fn norm(path: &str) -> String {
    let text = path.replace('\\', "/");
    let text = text.trim_end_matches('/').to_string();
    let bytes = text.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        text.to_lowercase()
    } else {
        text
    }
}

/// Whether a path is one of the recorded folders or inside one.
pub fn covered(mounts: &[String], path: &str) -> bool {
    let path = norm(path);
    mounts.iter().map(|m| norm(m)).any(|mount| {
        path == mount || path.strip_prefix(&mount).is_some_and(|rest| rest.starts_with('/'))
    })
}

/// Whether a distro's `/etc/wsl.conf` stops it mounting every Windows drive.
pub fn automount_off(conf: &str) -> bool {
    let mut section = String::new();
    for line in conf.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line.to_lowercase();
        } else if section == "[automount]" {
            let mut parts = line.splitn(2, '=');
            let key = parts.next().unwrap_or("").trim().to_lowercase();
            let value = parts.next().unwrap_or("").trim().to_lowercase();
            if key == "enabled" {
                return value == "false";
            }
        }
    }
    false
}

/// The full path of a folder on this machine, without resolving links, so a
/// mapped drive keeps its letter.
fn full(path: &str) -> Result<String, String> {
    let path = std::path::absolute(path).map_err(|e| format!("{path}: {e}"))?;
    let text = path.to_string_lossy().to_string();
    let trimmed = text.trim_end_matches(['\\', '/']);
    // Keep the slash of a bare root such as C:\ or /.
    Ok(if trimmed.is_empty() || trimmed.ends_with(':') { text } else { trimmed.to_string() })
}

fn run(mut command: Command) -> Result<String, String> {
    let name = command.get_program().to_string_lossy().to_string();
    let out = command.output().map_err(|e| format!("Could not start {name}: {e}"))?;
    if out.status.success() {
        return Ok(String::from_utf8_lossy(&out.stdout).replace('\0', "").trim().to_string());
    }
    let problem: Vec<String> = String::from_utf8_lossy(&out.stderr)
        .replace('\0', "")
        .lines()
        // WSL complains about Windows PATH entries it cannot map. Not ours.
        .filter(|line| !line.starts_with("wsl: Failed to translate"))
        .map(str::to_string)
        .collect();
    let mut problem = problem.join("\n").trim().to_string();
    // Git says some things, such as having nothing to commit, on its
    // ordinary output, and a program that dies at start says nothing at all.
    // Without these two a failure can read as a harmless warning.
    let said = String::from_utf8_lossy(&out.stdout).replace('\0', "").trim().to_string();
    if !said.is_empty() {
        problem = format!("{problem}\n{said}").trim().to_string();
    }
    Err(format!("{problem}\n({name} ended with {})", out.status).trim().to_string())
}

/// A program to start. On Windows it is told not to open a console window,
/// which it otherwise does whenever Maestro itself has none.
fn program(name: &str) -> Command {
    #[allow(unused_mut)]
    let mut command = Command::new(name);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command
}

fn wsl(args: &[&str]) -> Command {
    let mut command = program("wsl.exe");
    // Without this, WSL's own messages come out as UTF-16.
    command.env("WSL_UTF8", "1").args(args);
    command
}

/// The WSL distros on this machine.
pub fn distros() -> Result<Vec<String>, String> {
    if !cfg!(windows) {
        return Err(
            "The image is a WSL distro, which only Windows has. On this system works run \
             on the machine itself; a container image is not built yet."
                .to_string(),
        );
    }
    let list = run(wsl(&["-l", "-q"]))?;
    Ok(list.lines().map(|l| l.trim().to_string()).filter(|l| !l.is_empty()).collect())
}

fn check_name(name: &str) -> Result<(), String> {
    let fine = !name.is_empty()
        && name.len() <= 40
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        && !name.starts_with('-');
    if fine {
        Ok(())
    } else {
        Err("A work's name is 1 to 40 lowercase letters, digits and hyphens.".to_string())
    }
}

impl Environment {
    pub fn load() -> Self {
        crate::paths::load(FILE)
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            crate::paths::save(FILE, &text);
        }
    }

    /// The distro commands run in, if one is chosen.
    fn image(&self) -> Option<&str> {
        if cfg!(windows) {
            self.distro.as_deref()
        } else {
            None
        }
    }

    /// A path on this machine as the side that runs git sees it.
    fn place(&self, path: &str) -> Result<String, String> {
        match self.image() {
            Some(_) => inside(path)
                .ok_or(format!("{path} is a network path, which has no place inside the image.")),
            None => Ok(path.to_string()),
        }
    }

    fn git(&self, dir: &str, args: &[&str]) -> Result<String, String> {
        let mut command = match self.image() {
            Some(distro) => wsl(&["-d", distro, "--exec", "git"]),
            None => program("git"),
        };
        command
            // Snapshots are commits, and a commit needs an author.
            .args(["-c", "user.name=Maestro", "-c", "user.email=maestro@localhost", "-C"])
            .arg(self.place(dir)?)
            .args(args);
        run(command)
    }

    pub fn mount(&mut self, folder: &str) -> Result<String, String> {
        let folder = full(folder)?;
        if !Path::new(&folder).is_dir() {
            return Err(format!("{folder} is not a folder."));
        }
        if inside(&folder).is_none() {
            return Err(format!("{folder} is a network path, which has no place inside the image."));
        }
        if covered(&self.mounts, &folder) {
            return Err(format!("{folder} is already recorded, or is inside a folder that is."));
        }
        self.mounts.push(folder.clone());
        Ok(folder)
    }

    pub fn unmount(&mut self, folder: &str) -> Result<String, String> {
        let folder = full(folder)?;
        if let Some(work) = self.works.iter().find(|w| covered(&[folder.clone()], &w.repo)) {
            return Err(format!("The work {} still uses {folder}. Remove it first.", work.name));
        }
        let before = self.mounts.len();
        self.mounts.retain(|m| norm(m) != norm(&folder));
        if self.mounts.len() == before {
            return Err(format!("{folder} is not a recorded folder."));
        }
        Ok(folder)
    }

    fn work(&self, name: &str) -> Result<&Work, String> {
        self.works
            .iter()
            .find(|w| w.name == name)
            .ok_or(format!("There is no work called {name}."))
    }

    /// Cut a worktree from a repository's current commit, on a new branch.
    /// The repository must be inside a recorded folder. Changes that are
    /// not committed in the repository are not carried over.
    pub fn work_new(&mut self, root: &Path, repo: &str, name: &str) -> Result<Work, String> {
        check_name(name)?;
        if self.works.iter().any(|w| w.name == name) {
            return Err(format!("There is already a work called {name}."));
        }
        let repo = full(repo)?;
        if !covered(&self.mounts, &repo) {
            return Err(format!(
                "{repo} is not inside a recorded folder. Record it first with --env-mount."
            ));
        }
        let path = root.join(name);
        if path.exists() {
            return Err(format!("{} already exists.", path.display()));
        }
        let path = path.to_string_lossy().to_string();
        let top = self.git(&repo, &["rev-parse", "--show-toplevel"])?;
        if norm(&top) != norm(&self.place(&repo)?) {
            return Err(format!("{repo} is inside a repository but is not its top folder."));
        }
        let base = self
            .git(&repo, &["rev-parse", "--verify", "HEAD"])
            .map_err(|_| format!("{repo} has no commit yet, so there is nothing to start from."))?;
        std::fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
        let branch = format!("maestro/{name}");
        self.git(&repo, &["worktree", "add", "-q", "-b", &branch, &self.place(&path)?, "HEAD"])?;
        let work = Work { name: name.to_string(), repo, path, branch, base, snapshots: Vec::new() };
        self.works.push(work.clone());
        Ok(work)
    }

    /// Record the worktree exactly as it is now, as a commit kept under
    /// `refs/maestro/`. Files git is told to ignore are not included.
    pub fn snapshot(&mut self, name: &str, note: &str) -> Result<Snapshot, String> {
        let work = self.work(name)?.clone();
        let number = work.snapshots.last().map_or(1, |s| s.number + 1);
        let note = if note.is_empty() { format!("snapshot {number}") } else { note.to_string() };
        self.git(&work.path, &["add", "-A"])?;
        let tree = self.git(&work.path, &["write-tree"]);
        // The worktree's own staging area is put back whatever happened.
        self.git(&work.path, &["reset", "-q"])?;
        let head = self.git(&work.path, &["rev-parse", "HEAD"])?;
        let commit = self.git(&work.path, &["commit-tree", &tree?, "-p", &head, "-m", &note])?;
        let name_of_ref = format!("refs/maestro/{name}/{number}");
        self.git(&work.path, &["update-ref", &name_of_ref, &commit])?;
        let snapshot = Snapshot { number, commit, at_ms: crate::history::now_ms(), note };
        if let Some(work) = self.works.iter_mut().find(|w| w.name == name) {
            work.snapshots.push(snapshot.clone());
        }
        Ok(snapshot)
    }

    /// Put the worktree back as a snapshot recorded it: the branch returns
    /// to the commit it was on, and files are restored, changed back or
    /// removed. Ignored files are left alone. Without a number, the latest
    /// snapshot is used.
    pub fn rollback(&self, name: &str, number: Option<u32>) -> Result<Snapshot, String> {
        let work = self.work(name)?;
        let snapshot = match number {
            Some(n) => work.snapshots.iter().find(|s| s.number == n),
            None => work.snapshots.last(),
        }
        .ok_or(format!("The work {name} has no such snapshot."))?
        .clone();
        let before = format!("{}^", snapshot.commit);
        self.git(&work.path, &["reset", "-q", "--hard", &before])?;
        self.git(&work.path, &["clean", "-q", "-fd"])?;
        self.git(&work.path, &["read-tree", "--reset", "-u", &snapshot.commit])?;
        self.git(&work.path, &["reset", "-q"])?;
        Ok(snapshot)
    }

    /// The files that differ from a snapshot, one per line as git names
    /// them: a letter for what happened, a tab, and the path. Files not yet
    /// committed count, and files git is told to ignore do not.
    pub fn changes(&self, name: &str, number: u32) -> Result<Vec<String>, String> {
        let work = self.work(name)?;
        let snapshot = work
            .snapshots
            .iter()
            .find(|s| s.number == number)
            .ok_or(format!("The work {name} has no such snapshot."))?;
        self.git(&work.path, &["add", "-A"])?;
        let listed = self.git(&work.path, &["diff", "--cached", "--name-status", &snapshot.commit]);
        // The worktree's own staging area is put back whatever happened.
        self.git(&work.path, &["reset", "-q"])?;
        Ok(listed?.lines().filter(|l| !l.trim().is_empty()).map(str::to_string).collect())
    }

    /// Commit everything in the work to its own branch, and give back the
    /// short name of the commit.
    pub fn keep(&self, name: &str, message: &str) -> Result<String, String> {
        let work = self.work(name)?;
        self.git(&work.path, &["add", "-A"])?;
        self.git(&work.path, &["commit", "-q", "-m", message])?;
        self.git(&work.path, &["rev-parse", "--short", "HEAD"])
    }

    /// Delete the worktree's folder. Its branch and snapshots stay in the
    /// repository, so nothing committed or snapshotted is lost.
    pub fn work_remove(&mut self, name: &str) -> Result<Work, String> {
        let work = self.work(name)?.clone();
        if Path::new(&work.path).exists() {
            self.git(&work.repo, &["worktree", "remove", "--force", &self.place(&work.path)?])?;
        } else {
            self.git(&work.repo, &["worktree", "prune"])?;
        }
        self.works.retain(|w| w.name != name);
        Ok(work)
    }

    /// What is known without starting anything.
    pub fn summary(&self, root: &Path) -> String {
        let mut out = String::new();
        match &self.distro {
            Some(distro) => out.push_str(&format!("Image: the WSL distro {distro}.\n")),
            None => out.push_str(
                "Image: none chosen. Works run on this machine itself, which isolates \
                 nothing.\n",
            ),
        }
        out.push_str(&format!("Works are kept in {}.\n", root.display()));
        if self.mounts.is_empty() {
            out.push_str("Recorded folders: none. Add one with --env-mount.\n");
        }
        for mount in &self.mounts {
            let place = inside(mount).unwrap_or_default();
            out.push_str(&format!("Recorded folder: {mount}, inside as {place}\n"));
        }
        for work in &self.works {
            out.push_str(&format!(
                "Work {}: branch {} of {}, {} snapshot{}, at {}\n",
                work.name,
                work.branch,
                work.repo,
                work.snapshots.len(),
                if work.snapshots.len() == 1 { "" } else { "s" },
                work.path
            ));
        }
        out
    }

    /// Look inside the chosen distro. This starts it if it is stopped.
    pub fn probe(&self) -> String {
        let Some(distro) = &self.distro else {
            return String::new();
        };
        let found = match distros() {
            Ok(found) => found,
            Err(problem) => return format!("{problem}\n"),
        };
        if !found.contains(distro) {
            return format!("{distro} is not a WSL distro on this machine. Found: {}.\n", found.join(", "));
        }
        let mut out = String::new();
        let script = format!("command -v {}", TOOLS.join(" "));
        // `command -v` fails when any name is missing, so read what it printed either way.
        let mut command = wsl(&["-d", distro, "--exec", "bash", "-lc", &script]);
        let seen = command
            .output()
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let has = |tool: &str| seen.lines().any(|line| line.trim().rsplit('/').next() == Some(tool));
        let (yes, no): (Vec<&str>, Vec<&str>) = TOOLS.iter().partition(|tool| has(tool));
        out.push_str(&format!(
            "Inside {distro}: found {}; not found {}.\n",
            if yes.is_empty() { "nothing".to_string() } else { yes.join(", ") },
            if no.is_empty() { "nothing".to_string() } else { no.join(", ") },
        ));
        let conf = run(wsl(&["-d", distro, "--exec", "cat", "/etc/wsl.conf"])).unwrap_or_default();
        out.push_str(if automount_off(&conf) {
            "It does not mount Windows drives by itself, so only what its own mount table \
             lists is visible inside.\n"
        } else {
            "It mounts every Windows drive, so a tool inside can reach all your files, not \
             only the recorded folders. See --env-plan for an image that cannot.\n"
        });
        out
    }

    /// How a dedicated image would be made from the recorded folders.
    /// Nothing here is run.
    pub fn setup_plan(&self, root: &Path) -> String {
        let mut folders: Vec<String> = self.mounts.clone();
        folders.push(root.to_string_lossy().to_string());
        let escape = |text: &str| text.replace(' ', "\\040");
        let mut table = String::new();
        let mut points = Vec::new();
        for folder in &folders {
            if let Some(place) = inside(folder) {
                table.push_str(&format!("{} {} drvfs defaults 0 0\n", escape(folder), escape(&place)));
                points.push(format!("'{place}'"));
            }
        }
        format!(
            "DRY RUN. Nothing is started, and none of this has been run by Maestro.\n\
             \n\
             1. Make the image. This downloads Ubuntu and takes some disk space:\n\
             \x20  wsl --install Ubuntu-24.04 --name maestro --no-launch\n\
             \n\
             2. Inside it, as root, write /etc/wsl.conf so it stops mounting every drive\n\
             \x20  and cannot start Windows programs:\n\
             [automount]\n\
             enabled = false\n\
             mountFsTab = true\n\
             [interop]\n\
             enabled = false\n\
             appendWindowsPath = false\n\
             \n\
             3. Inside it, make a mount point for each recorded folder and for the works:\n\
             \x20  mkdir -p {}\n\
             \n\
             4. Inside it, write /etc/fstab so only those folders are mounted:\n\
             {table}\
             \n\
             5. Restart it so both files take effect:\n\
             \x20  wsl --terminate maestro\n\
             \n\
             6. Install git and each vendor's command-line tool inside it, by the vendor's\n\
             \x20  own instructions, and sign in to each once.\n\
             \n\
             7. Tell Maestro to use it:\n\
             \x20  maestro --env-use maestro\n\
             \n\
             A snapshot of the whole image, as opposed to a work, would be\n\
             wsl --export maestro <file>. Maestro does not take those yet.\n",
            points.join(" ")
        )
    }
}

const USE: &str = "\
--env                               what the environment is, and a look inside the image
--env-use <distro> | --env-use none choose the WSL distro works run in
--env-mount <folder>                record a folder the image may see
--env-unmount <folder>              stop recording it
--env-plan                          print how a dedicated image would be made
--work                              list works
--work-new <repository> <name>      cut a worktree on a new branch
--work-snapshot <name> [note]       record the worktree as it is now
--work-rollback <name> [number]     put it back as a snapshot recorded it
--work-remove <name>                delete the worktree, keeping its branch";

/// The command-line flags for the environment. Returns the exit code when
/// one of them was given.
pub fn cli(args: &[String]) -> Option<i32> {
    let flag = args.iter().skip(1).find(|a| {
        *a == "--env" || a.starts_with("--env-") || *a == "--work" || a.starts_with("--work-")
    })?;
    let at = args.iter().position(|a| a == flag)?;
    let rest: Vec<&str> = args[at + 1..]
        .iter()
        .map(String::as_str)
        .take_while(|a| !a.starts_with("--"))
        .collect();
    let root = crate::paths::data_dir().join("work");
    // One spelling of the separators, whatever the data folder was given as.
    let root = std::path::absolute(&root).unwrap_or(root);
    let mut env = Environment::load();

    let result: Result<String, String> = match (flag.as_str(), rest.as_slice()) {
        ("--env", []) => Ok(format!("{}{}", env.summary(&root), env.probe())),
        ("--env-plan", []) => Ok(env.setup_plan(&root)),
        ("--env-use", ["none"]) => {
            env.distro = None;
            Ok("Works now run on this machine itself.".to_string())
        }
        ("--env-use", [distro]) => distros().and_then(|found| {
            if !env.works.is_empty() {
                return Err("Remove the works first: each was made by the git of the side it \
                            was made on, and the other side cannot read it."
                    .to_string());
            }
            if found.iter().any(|d| d == distro) {
                env.distro = Some(distro.to_string());
                Ok(format!("Works now run inside {distro}."))
            } else {
                Err(format!("{distro} is not a WSL distro here. Found: {}.", found.join(", ")))
            }
        }),
        ("--env-mount", [folder]) => env.mount(folder).map(|f| format!("recorded {f}")),
        ("--env-unmount", [folder]) => env.unmount(folder).map(|f| format!("no longer recorded: {f}")),
        ("--work", []) => Ok(if env.works.is_empty() {
            "There are no works. Cut one with --work-new.".to_string()
        } else {
            let mut out = String::new();
            for work in &env.works {
                out.push_str(&format!(
                    "{}: branch {} of {}, from {}, at {}\n",
                    work.name,
                    work.branch,
                    work.repo,
                    &work.base[..work.base.len().min(8)],
                    work.path
                ));
                for s in &work.snapshots {
                    out.push_str(&format!("  snapshot {}: {}\n", s.number, s.note));
                }
            }
            out
        }),
        ("--work-new", [repo, name]) => env.work_new(&root, repo, name).map(|work| {
            format!("cut {} on branch {} at {}", work.name, work.branch, work.path)
        }),
        ("--work-snapshot", [name, note @ ..]) => env
            .snapshot(name, &note.join(" "))
            .map(|s| format!("snapshot {} of {name}: {}", s.number, s.note)),
        ("--work-rollback", [name]) => env
            .rollback(name, None)
            .map(|s| format!("{name} is back at snapshot {}: {}", s.number, s.note)),
        ("--work-rollback", [name, number]) => match number.parse::<u32>() {
            Ok(n) => env
                .rollback(name, Some(n))
                .map(|s| format!("{name} is back at snapshot {}: {}", s.number, s.note)),
            Err(_) => Err(format!("{number} is not a snapshot number.")),
        },
        ("--work-remove", [name]) => env.work_remove(name).map(|work| {
            format!("removed {}. Its branch {} and snapshots are still in {}.", work.name, work.branch, work.repo)
        }),
        _ => Err(format!("That is not how {flag} is used.\n{USE}")),
    };
    Some(match result {
        Ok(text) => {
            env.save();
            println!("{}", text.trim_end());
            0
        }
        Err(problem) => {
            eprintln!("{problem}");
            2
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn paths_map_into_the_image() {
        assert_eq!(inside("Z:\\Work\\repo").as_deref(), Some("/mnt/z/Work/repo"));
        assert_eq!(inside("c:/Users/me/").as_deref(), Some("/mnt/c/Users/me"));
        assert_eq!(inside("C:\\").as_deref(), Some("/mnt/c"));
        assert_eq!(inside("/home/me/repo").as_deref(), Some("/home/me/repo"));
        assert_eq!(inside("\\\\server\\share\\repo"), None);
        assert_eq!(inside("//server/share"), None);
        assert_eq!(inside("repo"), None);
    }

    #[test]
    fn only_recorded_folders_are_covered() {
        let mounts = vec!["Z:\\Work\\repo".to_string(), "/srv/code".to_string()];
        assert!(covered(&mounts, "Z:\\Work\\repo"));
        assert!(covered(&mounts, "z:/work/repo/src"));
        assert!(covered(&mounts, "/srv/code/app"));
        // A name that merely starts the same is a different folder.
        assert!(!covered(&mounts, "Z:\\Work\\repo2"));
        assert!(!covered(&mounts, "Z:\\Work"));
        // Paths that are not drive paths keep their case.
        assert!(!covered(&mounts, "/srv/Code/app"));
    }

    #[test]
    fn reads_whether_a_distro_mounts_every_drive() {
        assert!(!automount_off(""));
        assert!(!automount_off("[boot]\nsystemd=true\n"));
        assert!(!automount_off("[automount]\nenabled = true\n"));
        assert!(automount_off("[boot]\nsystemd=true\n[automount]\nEnabled = False\n"));
        // The same key in another section says nothing about mounting.
        assert!(!automount_off("[interop]\nenabled = false\n"));
    }

    #[test]
    fn the_plan_mounts_only_recorded_folders() {
        let env = Environment {
            mounts: vec!["Z:\\Work\\my repo".to_string()],
            ..Default::default()
        };
        let plan = env.setup_plan(Path::new("C:\\Data\\Maestro\\work"));
        assert!(plan.contains("Z:\\Work\\my\\040repo /mnt/z/Work/my\\040repo drvfs defaults 0 0"));
        assert!(plan.contains("C:\\Data\\Maestro\\work /mnt/c/Data/Maestro/work drvfs"));
        assert!(plan.contains("mkdir -p '/mnt/z/Work/my repo' '/mnt/c/Data/Maestro/work'"));
        assert!(plan.starts_with("DRY RUN"));
    }

    /// A scratch folder with a repository of one commit in it.
    fn scratch(tag: &str) -> (PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("maestro-test-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let repo = repo.to_string_lossy().to_string();
        let env = Environment::default();
        env.git(&repo, &["init", "-q"]).unwrap();
        std::fs::write(Path::new(&repo).join("kept.txt"), "one\n").unwrap();
        env.git(&repo, &["add", "-A"]).unwrap();
        env.git(&repo, &["commit", "-q", "-m", "first"]).unwrap();
        (dir, repo)
    }

    #[test]
    fn a_work_can_be_snapshotted_and_rolled_back() {
        let (dir, repo) = scratch("roll");
        let root = dir.join("work");
        let mut env = Environment::default();
        env.mount(&repo).unwrap();
        let work = env.work_new(&root, &repo, "job-1").unwrap();
        let file = |name: &str| Path::new(&work.path).join(name);
        assert_eq!(std::fs::read_to_string(file("kept.txt")).unwrap().trim(), "one");

        // State to keep: one changed file and one new file, neither committed.
        std::fs::write(file("kept.txt"), "two\n").unwrap();
        std::fs::write(file("new.txt"), "new\n").unwrap();
        let first = env.snapshot("job-1", "before the run").unwrap();
        assert_eq!(first.number, 1);

        // A bad run: a commit, a change, a deletion and a stray file.
        std::fs::write(file("kept.txt"), "three\n").unwrap();
        env.git(&work.path, &["commit", "-q", "-am", "bad"]).unwrap();
        std::fs::remove_file(file("new.txt")).unwrap();
        std::fs::create_dir_all(file("junk")).unwrap();
        std::fs::write(file("junk/stray.txt"), "x").unwrap();
        assert_eq!(env.snapshot("job-1", "").unwrap().number, 2);

        env.rollback("job-1", Some(1)).unwrap();
        assert_eq!(std::fs::read_to_string(file("kept.txt")).unwrap().trim(), "two");
        assert_eq!(std::fs::read_to_string(file("new.txt")).unwrap().trim(), "new");
        assert!(!file("junk").exists());
        // The branch is back on the commit it started from, the bad one gone from it.
        assert_eq!(env.git(&work.path, &["rev-parse", "HEAD"]).unwrap(), work.base);
        // The repository it was cut from was never touched.
        assert_eq!(std::fs::read_to_string(Path::new(&repo).join("kept.txt")).unwrap().trim(), "one");

        // The later snapshot still holds the bad run, should it be wanted.
        env.rollback("job-1", None).unwrap();
        assert_eq!(std::fs::read_to_string(file("kept.txt")).unwrap().trim(), "three");
        assert!(file("junk/stray.txt").exists());
        assert!(!file("new.txt").exists());

        env.work_remove("job-1").unwrap();
        assert!(!Path::new(&work.path).exists());
        assert!(env.works.is_empty());
        assert!(env.git(&repo, &["rev-parse", "--verify", "maestro/job-1"]).is_ok());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_work_needs_a_recorded_repository_and_a_plain_name() {
        let (dir, repo) = scratch("refuse");
        let root = dir.join("work");
        let mut env = Environment::default();
        assert!(env.work_new(&root, &repo, "job").unwrap_err().contains("--env-mount"));
        env.mount(&repo).unwrap();
        assert!(env.mount(&repo).is_err());
        assert!(env.work_new(&root, &repo, "Bad Name").is_err());
        assert!(env.work_new(&root, &repo, "../up").is_err());
        env.work_new(&root, &repo, "job").unwrap();
        assert!(env.work_new(&root, &repo, "job").is_err());
        // A folder in use cannot be dropped from the record.
        assert!(env.unmount(&repo).is_err());
        env.work_remove("job").unwrap();
        env.unmount(&repo).unwrap();
        assert!(env.mounts.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

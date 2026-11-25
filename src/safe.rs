//! Putting a session somewhere it can be undone.
//!
//! A session that may change files is started in a work: a separate copy of
//! the repository on its own branch. A snapshot is taken first, so whatever
//! the session does can be listed afterwards and rolled back in one step.
//! Your own checkout is never touched.

use crate::environment::Environment;
use std::path::{Path, PathBuf};

/// The work a session runs in, and the snapshot taken before it started.
#[derive(Clone, Debug, PartialEq)]
pub struct Safe {
    pub work: String,
    pub snapshot: u32,
    /// The folder the session works in.
    pub path: String,
    pub branch: String,
    /// The repository the work was cut from.
    pub repo: String,
    /// True when the work was made for this session.
    pub fresh: bool,
}

/// Where works are kept.
pub fn root() -> PathBuf {
    let root = crate::paths::data_dir().join("work");
    std::path::absolute(&root).unwrap_or(root)
}

/// A name for a new work, from the first words of what was asked.
pub fn work_name(prompt: &str, taken: &[String]) -> String {
    let words: Vec<String> = prompt
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .take(3)
        .map(str::to_ascii_lowercase)
        .collect();
    let mut base = words.join("-");
    base.truncate(28);
    let base = base.trim_matches('-').to_string();
    let base = if base.is_empty() { "session".to_string() } else { base };
    let mut name = base.clone();
    let mut n = 2;
    while taken.contains(&name) {
        name = format!("{base}-{n}");
        n += 1;
    }
    name
}

fn same_folder(a: &str, b: &str) -> bool {
    let norm = |p: &str| p.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase();
    norm(a) == norm(b)
}

/// Get a folder ready for a session.
///
/// - A folder that is already a work gets a snapshot.
/// - With `copy`, a new work is cut from the repository in the folder, and
///   gets a snapshot. The folder is recorded as one the environment may see
///   if it was not already.
/// - Otherwise nothing is done and the session runs in the folder as it is.
pub fn prepare(
    env: &mut Environment,
    root: &Path,
    folder: &str,
    copy: bool,
    prompt: &str,
) -> Result<Option<Safe>, String> {
    let existing = env.works.iter().find(|w| same_folder(&w.path, folder)).cloned();
    let (work, fresh) = match existing {
        Some(work) => (work, false),
        None if copy => {
            if !crate::environment::covered(&env.mounts, folder) {
                env.mount(folder)?;
            }
            let taken: Vec<String> = env.works.iter().map(|w| w.name.clone()).collect();
            let name = work_name(prompt, &taken);
            let work = env.work_new(root, folder, &name).map_err(|problem| {
                format!(
                    "Could not make a safe copy of {folder}. {problem}\nIt has to be the top folder of a git \
                     repository with at least one commit. Untick the safe copy to work in the folder as it is."
                )
            })?;
            (work, true)
        }
        None => return Ok(None),
    };
    let snapshot = env.snapshot(&work.name, "before a session")?;
    Ok(Some(Safe {
        work: work.name,
        snapshot: snapshot.number,
        path: work.path,
        branch: work.branch,
        repo: work.repo,
        fresh,
    }))
}

/// What a session changed, in plain words per file.
pub fn changes(env: &Environment, safe: &Safe) -> Result<Vec<String>, String> {
    Ok(env
        .changes(&safe.work, safe.snapshot)?
        .iter()
        .map(|line| {
            let (code, file) = line.split_once('\t').unwrap_or(("?", line));
            let what = match code.chars().next() {
                Some('A') => "new",
                Some('M') => "changed",
                Some('D') => "deleted",
                Some('R') => "renamed",
                _ => "touched",
            };
            format!("{what}: {}", file.replace('\t', " -> "))
        })
        .collect())
}

/// Put the work back exactly as it was before the session started,
/// including anything the session committed.
pub fn rollback(env: &Environment, safe: &Safe) -> Result<(), String> {
    env.rollback(&safe.work, Some(safe.snapshot)).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn git(dir: &Path, args: &[&str]) {
        let mut command = Command::new("git");
        command
            .args(["-c", "user.name=Test", "-c", "user.email=test@localhost", "-C"])
            .arg(dir)
            .args(args);
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x0800_0000);
        }
        assert!(command.output().unwrap().status.success(), "git {args:?}");
    }

    #[test]
    fn names_come_from_the_request_and_never_clash() {
        assert_eq!(work_name("Fix the build, please!", &[]), "fix-the-build");
        assert_eq!(work_name("Fix the build", &["fix-the-build".into()]), "fix-the-build-2");
        assert_eq!(
            work_name("Fix the build", &["fix-the-build".into(), "fix-the-build-2".into()]),
            "fix-the-build-3"
        );
        assert_eq!(work_name("???", &[]), "session");
        assert!(work_name("supercalifragilisticexpialidocious antidisestablishmentarianism x", &[]).len() <= 28);
    }

    #[test]
    fn a_session_in_a_safe_copy_can_be_listed_and_rolled_back() {
        let base = std::env::temp_dir().join(format!("maestro-safe-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let repo = base.join("repo");
        let root = base.join("works");
        std::fs::create_dir_all(&repo).unwrap();
        std::fs::write(repo.join("kept.txt"), "one\n").unwrap();
        git(&repo, &["init", "-q"]);
        git(&repo, &["add", "-A"]);
        git(&repo, &["commit", "-q", "-m", "start"]);
        let folder = repo.to_string_lossy().to_string();

        let mut env = Environment::default();
        // Without a copy, a plain folder is left as it is.
        assert_eq!(prepare(&mut env, &root, &folder, false, "tidy up").unwrap(), None);

        let safe = prepare(&mut env, &root, &folder, true, "Tidy up the notes").unwrap().unwrap();
        assert_eq!((safe.work.as_str(), safe.snapshot, safe.fresh), ("tidy-up-the", 1, true));
        assert_eq!(safe.branch, "maestro/tidy-up-the");
        assert!(changes(&env, &safe).unwrap().is_empty());

        // What a session might do: change a file, add one, and commit.
        let work = Path::new(&safe.path);
        std::fs::write(work.join("kept.txt"), "two\n").unwrap();
        std::fs::write(work.join("new.txt"), "new\n").unwrap();
        git(work, &["add", "kept.txt"]);
        git(work, &["commit", "-q", "-m", "by the session"]);
        let mut seen = changes(&env, &safe).unwrap();
        seen.sort();
        assert_eq!(seen, ["changed: kept.txt", "new: new.txt"]);
        // The repository itself was never touched.
        assert_eq!(std::fs::read_to_string(repo.join("kept.txt")).unwrap().trim(), "one");

        rollback(&env, &safe).unwrap();
        assert_eq!(std::fs::read_to_string(work.join("kept.txt")).unwrap().trim(), "one");
        assert!(!work.join("new.txt").exists());
        assert!(changes(&env, &safe).unwrap().is_empty());

        // Starting again in the same work takes a new snapshot and makes no new copy.
        let again = prepare(&mut env, &root, &safe.path, false, "more").unwrap().unwrap();
        assert_eq!((again.work.as_str(), again.snapshot, again.fresh), ("tidy-up-the", 2, false));
        assert_eq!(env.works.len(), 1);

        // A folder that is not a repository says what it needs.
        let plain = base.join("plain");
        std::fs::create_dir_all(&plain).unwrap();
        let problem = prepare(&mut env, &root, &plain.to_string_lossy(), true, "x").unwrap_err();
        assert!(problem.contains("Untick the safe copy"), "{problem}");
        let _ = std::fs::remove_dir_all(&base);
    }
}

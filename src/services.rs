//! Which agent tools are on this computer, whether each can be used right
//! now, and what to do when one cannot.
//!
//! Every check is something you could do yourself in a terminal, and each
//! result says how it was reached. Maestro looks at whether a key is set,
//! never at the key: no value is read into the app, shown or stored.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    /// Installed and signed in, as far as the check can tell.
    Ready,
    /// Installed, but not signed in and no key set.
    SignIn,
    /// Not found on this computer.
    Missing,
    /// Installed and not running.
    Stopped,
    /// The app is installed, but not the part another program can start.
    AgentMissing,
    /// The check itself did not work.
    Unknown,
}

impl State {
    pub fn label(self) -> &'static str {
        match self {
            State::Ready => "ready",
            State::SignIn => "needs sign-in",
            State::Missing => "not installed",
            State::Stopped => "not running",
            State::AgentMissing => "editor only",
            State::Unknown => "could not check",
        }
    }
}

/// What Maestro can do with a tool today.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Reach {
    /// Follow its sessions and start and direct new ones.
    Direct,
    /// Counted among the running programs only.
    SeenOnly,
}

#[derive(Clone, Debug)]
pub struct Service {
    pub id: &'static str,
    pub name: &'static str,
    pub state: State,
    pub reach: Reach,
    /// What was found, in plain words.
    pub found: String,
    /// How it was checked.
    pub how: String,
    /// What to do about it, one step a line. Empty when nothing is needed.
    pub fix: Vec<String>,
    /// A command that signs in, when the tool has one.
    pub sign_in: Option<&'static str>,
    /// Named sign-ins kept apart from the tool's own, and whether each can be used.
    pub logins: Vec<LoginState>,
}

/// One named sign-in and whether a session can be started under it.
#[derive(Clone, Debug)]
pub struct LoginState {
    pub login: crate::logins::Login,
    pub ready: bool,
    /// Its state in plain words.
    pub text: String,
}

/// Find a command the way a terminal would: each folder on the path, with
/// each ending Windows treats as a program.
pub fn find_on_path(command: &str, path: &str, endings: &[&str]) -> Option<PathBuf> {
    let separator = if cfg!(windows) { ';' } else { ':' };
    path.split(separator)
        .filter(|dir| !dir.is_empty())
        .flat_map(|dir| {
            endings
                .iter()
                .map(move |ending| Path::new(dir).join(format!("{command}{ending}")))
        })
        .find(|candidate| candidate.is_file())
}

fn find(command: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    if cfg!(windows) {
        find_on_path(command, &path, &[".exe", ".cmd", ".bat"])
    } else {
        find_on_path(command, &path, &[""])
    }
}

/// True when the variable holds something. The value is dropped at once.
fn key_set(name: &str) -> bool {
    std::env::var_os(name).is_some_and(|value| !value.is_empty())
}

/// Run a command with no window and give back what it printed.
fn output(program: &Path, args: &[&str]) -> Option<String> {
    output_with(program, args, &[])
}

fn output_with(program: &Path, args: &[&str], env: &[(String, std::ffi::OsString)]) -> Option<String> {
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stderr(Stdio::null());
    command.envs(env.iter().map(|(k, v)| (k, v)));
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let out = command.output().ok()?;
    Some(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Read what `claude auth status` printed. `None` when it is not the
/// expected shape.
pub fn claude_signed_in(status: &str) -> Option<bool> {
    let value: serde_json::Value = serde_json::from_str(status.trim()).ok()?;
    value.get("loggedIn")?.as_bool()
}

/// Decide Claude Code's state from the pieces, so the rule can be tested.
pub fn claude_state(installed: bool, signed_in: Option<bool>, key: bool) -> State {
    match (installed, signed_in) {
        (false, _) => State::Missing,
        (true, Some(true)) => State::Ready,
        // A key in the environment is used in place of a sign-in.
        (true, _) if key => State::Ready,
        (true, Some(false)) => State::SignIn,
        (true, None) => State::Unknown,
    }
}

/// Whether one named sign-in can be used, checked the same way as the tool's own.
fn login_state(login: &crate::logins::Login, claude: Option<&Path>) -> LoginState {
    let (ready, text) = match (&login.key_var, login.key_set()) {
        (Some(var), Some(true)) => (true, format!("uses the key in {var}")),
        (Some(var), _) => (false, format!("its key variable {var} is not set")),
        (None, _) => {
            let folder = login.folder();
            // A folder that was never made has never been signed in to.
            let signed_in = folder.is_dir().then_some(()).and(claude).and_then(|claude| {
                let env = [(crate::logins::CONFIG_VAR.to_string(), folder.clone().into_os_string())];
                output_with(claude, &["auth", "status"], &env)
                    .as_deref()
                    .and_then(claude_signed_in)
            });
            match signed_in {
                Some(true) => (true, "signed in".to_string()),
                _ => (false, "not signed in yet".to_string()),
            }
        }
    };
    LoginState {
        login: login.clone(),
        ready,
        text,
    }
}

/// Every named sign-in with its state in words, for the command line.
pub fn login_states(logins: &crate::logins::Logins) -> Vec<(crate::logins::Login, String)> {
    let claude = find("claude");
    logins
        .claude
        .iter()
        .map(|l| {
            let state = login_state(l, claude.as_deref());
            (state.login, state.text)
        })
        .collect()
}

fn claude() -> Service {
    let path = find("claude");
    let status = path.as_deref().and_then(|p| output(p, &["auth", "status"]));
    let signed_in = status.as_deref().and_then(claude_signed_in);
    let key = key_set("ANTHROPIC_API_KEY");
    let state = claude_state(path.is_some(), signed_in, key);
    let found = match (&path, state) {
        (None, _) => "No `claude` command was found on the path.".to_string(),
        (Some(p), State::Ready) if signed_in == Some(true) => {
            format!("Signed in. The command is at {}.", p.display())
        }
        (Some(p), State::Ready) => format!(
            "Not signed in, but ANTHROPIC_API_KEY is set, so runs are billed to that key. The command is at {}.",
            p.display()
        ),
        (Some(p), State::SignIn) => format!(
            "Installed at {}, but it says it is not signed in, and ANTHROPIC_API_KEY is not set.",
            p.display()
        ),
        (Some(p), _) => format!(
            "Installed at {}, but its sign-in state could not be read.",
            p.display()
        ),
    };
    let fix = match state {
        State::Missing => vec![
            "Install Claude Code from https://claude.com/claude-code, then press Check again.".to_string(),
        ],
        State::SignIn => vec![
            "Press Sign in, or run `claude auth login` in a terminal. It opens your browser.".to_string(),
            "Or set ANTHROPIC_API_KEY in your environment and restart Maestro, to pay per use with a key.".to_string(),
        ],
        State::Unknown => vec![
            "Run `claude auth status` in a terminal to see what it reports.".to_string(),
        ],
        _ => Vec::new(),
    };
    Service {
        id: "claude-code",
        name: "Claude Code",
        state,
        reach: Reach::Direct,
        found,
        how: "Looked for `claude` on the path, ran `claude auth status`, and checked whether \
              ANTHROPIC_API_KEY is set (not what it holds)."
            .to_string(),
        fix,
        sign_in: (state == State::SignIn).then_some("claude auth login"),
        logins: crate::logins::Logins::load()
            .claude
            .iter()
            .map(|l| login_state(l, path.as_deref()))
            .collect(),
    }
}

/// A tool Maestro can see but cannot start or follow yet.
fn seen_only(
    id: &'static str,
    name: &'static str,
    commands: &[&str],
    key: &'static str,
) -> Service {
    let path = commands.iter().find_map(|c| find(c));
    let has_key = key_set(key);
    let (state, found) = match &path {
        None => (
            State::Missing,
            format!("No `{}` command was found on the path.", commands.join("` or `")),
        ),
        Some(p) if has_key => (
            State::Ready,
            format!("Installed at {}, and {key} is set.", p.display()),
        ),
        Some(p) => (
            State::SignIn,
            format!(
                "Installed at {}. {key} is not set. Whether it is signed in another way is not checked.",
                p.display()
            ),
        ),
    };
    let mut fix = Vec::new();
    if state == State::SignIn {
        fix.push(format!(
            "For runs started by Maestro, set {key} in your environment and restart Maestro. \
             This tool's terms do not say another program may start it on a subscription sign-in."
        ));
    }
    fix.push(format!(
        "Maestro cannot start or follow {name} yet, whatever its state. It is counted among the running programs."
    ));
    Service {
        id,
        name,
        state,
        reach: Reach::SeenOnly,
        found,
        how: format!(
            "Looked for `{}` on the path and checked whether {key} is set (not what it holds).",
            commands.join("` or `")
        ),
        fix,
        sign_in: None,
        logins: Vec::new(),
    }
}

fn ollama() -> Service {
    let path = find("ollama");
    // Its own port on this computer; nothing is sent, the connection is only opened.
    let address = std::net::SocketAddr::from(([127, 0, 0, 1], 11434));
    let up = std::net::TcpStream::connect_timeout(&address, std::time::Duration::from_millis(300)).is_ok();
    let (state, found, mut fix) = match (&path, up) {
        (_, true) => (
            State::Ready,
            "Running: it answers on this computer's port 11434.".to_string(),
            Vec::new(),
        ),
        (Some(p), false) => (
            State::Stopped,
            format!("Installed at {}, but nothing answers on port 11434.", p.display()),
            vec!["Start Ollama from its icon, or run `ollama serve` in a terminal.".to_string()],
        ),
        (None, false) => (
            State::Missing,
            "No `ollama` command was found on the path, and nothing answers on port 11434.".to_string(),
            Vec::new(),
        ),
    };
    fix.push("Maestro cannot send work to Ollama yet. It is counted among the running programs.".to_string());
    Service {
        id: "ollama",
        name: "Ollama",
        state,
        reach: Reach::SeenOnly,
        found,
        how: "Looked for `ollama` on the path and tried to open its port on this computer.".to_string(),
        fix,
        sign_in: None,
        logins: Vec::new(),
    }
}

/// Where the Cursor editor is installed, if it is. The editor is a separate
/// thing from Cursor's command-line agent, and only the agent can be started
/// by another program.
fn cursor_editor() -> Option<PathBuf> {
    let mut places: Vec<PathBuf> = Vec::new();
    for (var, tail) in [("ProgramFiles", "cursor"), ("LOCALAPPDATA", "Programs/cursor")] {
        if let Some(base) = std::env::var_os(var) {
            places.push(PathBuf::from(base).join(tail));
        }
    }
    places.push(PathBuf::from("/Applications/Cursor.app"));
    places.push(PathBuf::from("/opt/Cursor"));
    places
        .into_iter()
        .find(|p| p.is_dir())
        .or_else(|| find("cursor"))
}

/// Cursor: the editor and its command-line agent are reported separately.
fn cursor() -> Service {
    let mut service = seen_only("cursor", "Cursor", &["agent", "cursor-agent"], "CURSOR_API_KEY");
    let agent_missing = service.state == State::Missing;
    match (cursor_editor(), agent_missing) {
        (Some(editor), true) => {
            service.state = State::AgentMissing;
            service.found = format!(
                "The Cursor editor is installed at {}. Its command-line agent (`agent`) was not found on the path.",
                editor.display()
            );
            service.fix.insert(
                0,
                "The editor's window cannot be driven by another program; only the command-line agent can. \
                 Install it from Cursor's site if you want Maestro to start Cursor runs once that is built."
                    .to_string(),
            );
        }
        (Some(editor), false) => {
            service.found.push_str(&format!(" The Cursor editor is installed at {}.", editor.display()));
        }
        (None, _) => {}
    }
    service.how = "Looked for the Cursor editor in the usual install folders, for `agent` or `cursor-agent` \
                   on the path, and checked whether CURSOR_API_KEY is set (not what it holds)."
        .to_string();
    service
}

/// Check every tool Maestro knows. Takes a second or two; run it off the window's thread.
pub fn check() -> Vec<Service> {
    vec![
        claude(),
        seen_only("codex", "Codex", &["codex"], "OPENAI_API_KEY"),
        cursor(),
        seen_only("gemini", "Gemini CLI", &["gemini"], "GEMINI_API_KEY"),
        ollama(),
    ]
}

/// Open a terminal running a sign-in command, for the user to finish there.
/// This is the one place Maestro opens a window, and only when asked to.
pub fn open_sign_in(command: &str, env: &[(String, std::ffi::OsString)]) -> Result<(), String> {
    let result = if cfg!(windows) {
        // The terminal inherits what is set here, so the tool signs in to the right folder.
        Command::new("cmd")
            .args(["/c", "start", "Sign in", "cmd", "/k", command])
            .envs(env.iter().map(|(k, v)| (k, v)))
            .spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("osascript")
            .args(["-e", &format!("tell application \"Terminal\" to do script \"{command}\"")])
            .spawn()
    } else {
        Command::new("x-terminal-emulator")
            .args(["-e", command])
            .envs(env.iter().map(|(k, v)| (k, v)))
            .spawn()
    };
    result
        .map(|_| ())
        .map_err(|e| format!("Could not open a terminal. Run `{command}` in one yourself. ({e})"))
}

/// `--services`: print each tool's state and what to do about it.
pub fn print() {
    for s in check() {
        println!("{}: {}", s.name, s.state.label());
        println!("  {}", s.found);
        println!("  How this was checked: {}", s.how);
        for step in &s.fix {
            println!("  - {step}");
        }
        for l in &s.logins {
            println!("  Sign-in \"{}\": {}", l.login.name, l.text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_is_ready_when_signed_in_or_when_a_key_is_set() {
        assert_eq!(claude_state(false, None, true), State::Missing);
        assert_eq!(claude_state(true, Some(true), false), State::Ready);
        assert_eq!(claude_state(true, Some(false), true), State::Ready);
        assert_eq!(claude_state(true, Some(false), false), State::SignIn);
        assert_eq!(claude_state(true, None, false), State::Unknown);
        assert_eq!(claude_state(true, None, true), State::Ready);
    }

    #[test]
    fn the_sign_in_report_is_read_and_anything_else_is_not_guessed() {
        assert_eq!(claude_signed_in("{\n  \"loggedIn\": false,\n  \"authMethod\": \"none\"\n}"), Some(false));
        assert_eq!(claude_signed_in("{\"loggedIn\": true}"), Some(true));
        assert_eq!(claude_signed_in("{\"authMethod\": \"none\"}"), None);
        assert_eq!(claude_signed_in("Not logged in"), None);
    }

    #[test]
    fn a_command_is_found_only_where_the_path_points() {
        let dir = std::env::temp_dir().join(format!("maestro-path-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("tool.cmd"), "").unwrap();
        let separator = if cfg!(windows) { ";" } else { ":" };
        let path = format!("{}{separator}{}", dir.join("nowhere").display(), dir.display());
        assert_eq!(find_on_path("tool", &path, &[".exe", ".cmd"]), Some(dir.join("tool.cmd")));
        assert_eq!(find_on_path("tool", &path, &[".exe"]), None);
        assert_eq!(find_on_path("other", &path, &[".cmd"]), None);
        assert_eq!(find_on_path("tool", "", &[".cmd"]), None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}

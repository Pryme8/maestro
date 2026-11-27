//! Setup guides: for each agent tool that is on this computer but cannot be
//! used yet, the steps that get it working, with the ones already done
//! ticked. The aim is that nobody has to work out what is wrong: Maestro
//! says what it sees, what is missing and what to do next.

use crate::services::{Service, State};

/// Where one step stands.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mark {
    Done,
    /// For the user to do.
    ToDo,
    /// Maestro cannot tell whether this is done.
    NotChecked,
    /// Waiting on Maestro itself: the user can do nothing about it.
    OnMaestro,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub mark: Mark,
    pub title: String,
    pub detail: String,
    /// A line to paste into a terminal.
    pub command: Option<String>,
    /// An address to open in a browser.
    pub link: Option<&'static str>,
    /// A sign-in command Maestro can open a terminal for.
    pub sign_in: Option<&'static str>,
    /// A line from `RUN_LINES` that Maestro can run in a terminal, on a click.
    pub run: Option<&'static str>,
}

impl Step {
    fn new(mark: Mark, title: impl Into<String>, detail: impl Into<String>) -> Self {
        Self { mark, title: title.into(), detail: detail.into(), command: None, link: None, sign_in: None, run: None }
    }

    /// A line to show, which Maestro can also run for the user.
    fn run(mut self, shown: &str, line: &'static str) -> Self {
        debug_assert!(RUN_LINES.contains(&line));
        self.command = Some(shown.to_string());
        self.run = Some(line);
        self
    }

    fn link(mut self, address: &'static str) -> Self {
        self.link = Some(address);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Guide {
    /// The service's id.
    pub id: &'static str,
    pub name: &'static str,
    /// What is wrong, in one line.
    pub headline: String,
    pub steps: Vec<Step>,
}

impl Guide {
    /// How many steps are waiting on the user.
    pub fn left(&self) -> usize {
        self.steps.iter().filter(|s| s.mark == Mark::ToDo).count()
    }
}

const CURSOR_INSTALL_WINDOWS: &str = "irm 'https://cursor.com/install?win32=true' | iex";
const CURSOR_INSTALL_UNIX: &str = "curl https://cursor.com/install -fsS | bash";
// A terminal opened by Maestro has the path Maestro started with, so the
// folder the installer uses is added for the one command.
// Windows is asked for the path as it is now, which has the installer's folder in it.
const CURSOR_LOGIN_WINDOWS: &str =
    "$env:Path = [Environment]::GetEnvironmentVariable('Path','Machine') + ';' + [Environment]::GetEnvironmentVariable('Path','User'); agent login";
const CURSOR_LOGIN_UNIX: &str = "PATH=\"$HOME/.local/bin:$PATH\" agent login";
const OLLAMA_SERVE: &str = "ollama serve";
const CLAUDE_STATUS: &str = "claude auth status";

/// Every line Maestro will run in a terminal for a setup step. Each is fixed
/// here, comes from the tool maker's own documentation, and runs only on the
/// user's click after being shown. Two of them download and run a maker's
/// installer; a change to this list is a security change.
pub const RUN_LINES: &[&str] = &[
    CURSOR_INSTALL_WINDOWS,
    CURSOR_INSTALL_UNIX,
    CURSOR_LOGIN_WINDOWS,
    CURSOR_LOGIN_UNIX,
    OLLAMA_SERVE,
    CLAUDE_STATUS,
];

/// The line that installs Cursor's command-line agent, from Cursor's own page.
fn cursor_install() -> (&'static str, &'static str) {
    if cfg!(windows) {
        (CURSOR_INSTALL_WINDOWS, "It is a PowerShell line.")
    } else {
        (CURSOR_INSTALL_UNIX, "It is a terminal line.")
    }
}

fn cursor_sign_in(mark: Mark) -> Step {
    let detail = match mark {
        Mark::NotChecked => "This opens your browser to approve the sign-in. Maestro could not read whether it is done.",
        _ => "This opens your browser to approve the sign-in. Maestro ticks this when Cursor says you are signed in.",
    };
    Step::new(mark, "Sign in with your Cursor account", detail)
    .run("agent login", if cfg!(windows) { CURSOR_LOGIN_WINDOWS } else { CURSOR_LOGIN_UNIX })
}

fn on_maestro(name: &str) -> Step {
    Step::new(
        Mark::OnMaestro,
        format!("Maestro learns to start {name}"),
        format!(
            "Not built yet. Until it is, {name} is counted among the running programs and cannot be given \
             work from here. Nothing for you to do on this one."
        ),
    )
}

/// The guide for one tool, when something of it is here but it cannot be
/// used. `None` when it is ready, or simply not on this computer.
pub fn guide(service: &Service) -> Option<Guide> {
    let name = service.name;
    let found = service.found.clone();
    let (headline, steps) = match (service.id, service.state) {
        ("claude-code", State::Missing) => (
            "Claude Code is not installed, so Maestro has no agent it can start.".to_string(),
            vec![
                Step::new(Mark::ToDo, "Install Claude Code", "Get it from Claude's own page, then press Check again.")
                    .link("https://claude.com/claude-code"),
                Step::new(Mark::ToDo, "Sign in", "Once it is installed, a Sign in button appears on this step."),
            ],
        ),
        ("claude-code", State::SignIn) => {
            let mut sign_in = Step::new(
                Mark::ToDo,
                "Sign in",
                "Press Sign in. A terminal opens and your browser asks you to approve. Then press Check again. \
                 To pay per use with a key instead, set ANTHROPIC_API_KEY in your environment and restart Maestro.",
            );
            sign_in.sign_in = Some("claude auth login");
            (
                "Claude Code is installed but not signed in.".to_string(),
                vec![Step::new(Mark::Done, "Install Claude Code", found), sign_in],
            )
        }
        ("claude-code", State::Unknown) => (
            "Claude Code is installed, but its sign-in could not be read.".to_string(),
            vec![
                Step::new(Mark::Done, "Install Claude Code", found),
                Step::new(
                    Mark::ToDo,
                    "See what it reports",
                    "Paste this into a terminal. If it says you are not signed in, run `claude auth login`, then press Check again.",
                )
                .run(CLAUDE_STATUS, CLAUDE_STATUS),
            ],
        ),
        ("claude-code", State::Ready) => {
            let waiting: Vec<Step> = service
                .logins
                .iter()
                .filter(|l| !l.ready)
                .map(|l| {
                    let detail = match &l.login.key_var {
                        Some(var) => format!(
                            "It uses a key, and its variable {var} is not set. Set {var} in your environment and \
                             restart Maestro, or press Forget beside it under Services on the Live page."
                        ),
                        None => "Under Services on the Live page, press Sign in beside it and approve in your browser. \
                                 If you no longer want it, press Forget there instead."
                            .to_string(),
                    };
                    Step::new(Mark::ToDo, format!("Set up the sign-in \"{}\"", l.login.name), detail)
                })
                .collect();
            if waiting.is_empty() {
                return None;
            }
            let headline = match waiting.len() {
                1 => "Claude Code has a named sign-in that is not set up.".to_string(),
                n => format!("Claude Code has {n} named sign-ins that are not set up."),
            };
            let mut steps = vec![Step::new(Mark::Done, "Claude Code's own sign-in", found)];
            steps.extend(waiting);
            (headline, steps)
        }
        ("cursor", State::AgentMissing) => {
            let (line, where_to) = cursor_install();
            (
                "Cursor is installed, but only the editor. The part Maestro can work with is missing.".to_string(),
                vec![
                    // Only the sentence about the editor: the rest is the next step's news.
                    Step::new(Mark::Done, "Install the Cursor editor", found.split(". Its ").next().unwrap_or("").to_string()),
                    Step::new(
                        Mark::ToDo,
                        "Install Cursor's command-line agent",
                        format!(
                            "The editor's window cannot be driven by another program; Cursor's separate command, \
                             `agent`, can. {where_to} It downloads and runs Cursor's own installer."
                        ),
                    )
                    .run(line, line),
                    cursor_sign_in(Mark::ToDo),
                    on_maestro(name),
                ],
            )
        }
        ("cursor", State::SignIn | State::Unknown) => {
            let known = service.state == State::SignIn;
            let headline = if known {
                "Cursor's command-line agent is installed but not signed in."
            } else {
                "Cursor's command-line agent is installed. Whether it is signed in could not be read."
            };
            (
                headline.to_string(),
                vec![
                    Step::new(Mark::Done, "Install Cursor's command-line agent", found.split(", but ").next().unwrap_or("").to_string()),
                    cursor_sign_in(if known { Mark::ToDo } else { Mark::NotChecked }),
                    on_maestro(name),
                ],
            )
        }
        ("ollama", State::Stopped) => (
            "Ollama is installed but not running.".to_string(),
            vec![
                Step::new(Mark::Done, "Install Ollama", found),
                Step::new(Mark::ToDo, "Start it", "Start Ollama from its icon, or paste this into a terminal. Then press Check again.")
                    .run(OLLAMA_SERVE, OLLAMA_SERVE),
                on_maestro(name),
            ],
        ),
        _ => return None,
    };
    Some(Guide { id: service.id, name, headline, steps })
}

/// The guides worth showing: those with something for the user to do, the
/// tool Maestro can actually use first.
pub fn guides(services: &[Service], ignored: &[String]) -> Vec<Guide> {
    let mut found: Vec<Guide> = services
        .iter()
        .filter(|s| !ignored.iter().any(|id| id == s.id))
        .filter_map(guide)
        .filter(|g| g.left() > 0)
        .collect();
    found.sort_by_key(|g| g.id != "claude-code");
    found
}

/// What an agent is told when the user asks it to walk them through a setup.
/// It runs with every step asked first, so each command waits for a yes.
pub fn helper_brief(guide: &Guide) -> String {
    use std::fmt::Write as _;
    let mut out = format!(
        "You are helping the owner of this computer set up {} so it works with Maestro. Maestro found this: {}\n\nThe steps:\n",
        guide.name, guide.headline
    );
    for (at, step) in guide.steps.iter().enumerate() {
        let state = match step.mark {
            Mark::Done => "already done",
            Mark::ToDo => "to do",
            Mark::NotChecked => "not known whether done",
            Mark::OnMaestro => "not for you or the owner: Maestro has not built this yet",
        };
        let _ = writeln!(out, "{}. {} ({state}). {}", at + 1, step.title, step.detail);
        if let Some(line) = step.run.or(step.command.as_deref()) {
            let _ = writeln!(out, "   The line for it: {line}");
        }
        if let Some(link) = step.link {
            let _ = writeln!(out, "   The page for it: {link}");
        }
    }
    out.push_str(
        "\nRules:\n\
         - Take the steps in order, one at a time. Before each command say in one sentence what it does. The owner is asked to allow each command, so keep each one single and plain.\n\
         - Run only the lines given above, and commands that only look at whether a step worked, such as a version or status check. Do not install anything else, and do not change system or security settings.\n\
         - Never ask for, read, print or store a password, key or token. Signing in happens in the owner's browser or terminal; when a step needs them, say exactly what to do and wait for them to say it is done.\n\
         - If a step fails, show the error, say what it most likely means, and stop for the owner. Do not try a different way of installing.\n\
         - Do not touch a step marked as not for you.\n\
         - Finish by saying which steps are done, which are left, and that the owner should press Check again in Maestro.",
    );
    out
}

const IGNORED_FILE: &str = "setup-ignored.json";

/// The tools the user asked not to be reminded about.
pub fn load_ignored() -> Vec<String> {
    crate::paths::load(IGNORED_FILE).and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

pub fn save_ignored(ignored: &[String]) {
    if let Ok(text) = serde_json::to_string(ignored) {
        crate::paths::save(IGNORED_FILE, &text);
    }
}

/// `--setup` prints each tool that is here but not set up, and its steps.
pub fn print() {
    let found = guides(&crate::services::check(), &load_ignored());
    if found.is_empty() {
        println!("Every agent tool found on this computer is set up.");
    }
    for guide in found {
        println!("{}: {}", guide.name, guide.headline);
        for (at, step) in guide.steps.iter().enumerate() {
            let mark = match step.mark {
                Mark::Done => "done".to_string(),
                Mark::ToDo => format!("step {}", at + 1),
                Mark::NotChecked => "not checked".to_string(),
                Mark::OnMaestro => "waiting on Maestro".to_string(),
            };
            println!("  [{mark}] {}", step.title);
            println!("      {}", step.detail);
            for line in step.command.iter().map(String::as_str).chain(step.link) {
                println!("      > {line}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::services::{LoginState, Reach};

    fn service(id: &'static str, name: &'static str, state: State) -> Service {
        Service {
            id,
            name,
            state,
            reach: Reach::SeenOnly,
            found: "found it".to_string(),
            how: String::new(),
            fix: Vec::new(),
            sign_in: None,
            logins: Vec::new(),
        }
    }

    #[test]
    fn an_editor_without_its_agent_gets_every_step_spelled_out() {
        let guide = guide(&service("cursor", "Cursor", State::AgentMissing)).unwrap();
        let marks: Vec<Mark> = guide.steps.iter().map(|s| s.mark).collect();
        assert_eq!(marks, [Mark::Done, Mark::ToDo, Mark::ToDo, Mark::OnMaestro]);
        assert_eq!(guide.left(), 2);
        // Once the agent is in, signing in is the one step left, and it is the user's.
        let installed = super::guide(&service("cursor", "Cursor", State::SignIn)).unwrap();
        assert_eq!(installed.steps.iter().map(|s| s.mark).collect::<Vec<_>>(), [Mark::Done, Mark::ToDo, Mark::OnMaestro]);
        assert_eq!(crate::services::cursor_signed_in("Not logged in"), Some(false));
        assert_eq!(crate::services::cursor_signed_in("Logged in as someone"), Some(true));
        assert_eq!(crate::services::cursor_signed_in(""), None);
        let install = guide.steps[1].run.unwrap();
        assert_eq!(guide.steps[2].command.as_deref(), Some("agent login"));
        assert!(install.contains("https://cursor.com/install"), "the installer comes from Cursor's own site");
        // The guide never promises more than Maestro can do.
        assert!(guide.steps.last().unwrap().detail.contains("Not built yet"));
    }

    #[test]
    fn every_line_maestro_runs_is_on_the_fixed_list_and_comes_from_the_maker() {
        let states = [State::Ready, State::SignIn, State::Missing, State::Stopped, State::AgentMissing, State::Unknown];
        for (id, name) in [("claude-code", "Claude Code"), ("cursor", "Cursor"), ("ollama", "Ollama"), ("codex", "Codex")] {
            for state in states {
                for step in guide(&service(id, name, state)).into_iter().flat_map(|g| g.steps) {
                    if let Some(line) = step.run {
                        assert!(RUN_LINES.contains(&line), "{line} is run but not listed");
                        assert!(step.command.is_some(), "a line that can be run is always shown");
                    }
                }
            }
        }
        // A line that fetches anything fetches it from the tool's own maker, over https.
        for line in RUN_LINES.iter().filter(|l| l.contains("://")) {
            assert!(line.contains("https://cursor.com/install"), "{line}");
            assert_eq!(line.matches("://").count(), 1, "{line}");
        }
        assert!(crate::services::run_setup_line("echo not listed").is_err());
    }

    #[test]
    fn the_agents_brief_keeps_it_to_the_listed_steps() {
        let brief = helper_brief(&guide(&service("cursor", "Cursor", State::AgentMissing)).unwrap());
        for must in ["cursor.com/install", "already done", "Never ask for, read, print or store a password", "Run only the lines given above", "Do not install anything else", "not for you"] {
            assert!(brief.contains(must), "the brief no longer says: {must}");
        }
    }

    #[test]
    fn a_tool_that_is_ready_or_absent_gets_no_guide() {
        for (id, name, state) in [
            ("cursor", "Cursor", State::Missing),
            ("cursor", "Cursor", State::Ready),
            ("codex", "Codex", State::Missing),
            ("ollama", "Ollama", State::Ready),
            ("claude-code", "Claude Code", State::Ready),
        ] {
            assert_eq!(guide(&service(id, name, state)), None, "{id} {state:?}");
        }
    }

    #[test]
    fn the_one_agent_maestro_can_start_is_always_guided() {
        let missing = guide(&service("claude-code", "Claude Code", State::Missing)).unwrap();
        assert_eq!(missing.left(), 2);
        assert_eq!(missing.steps[0].link, Some("https://claude.com/claude-code"));
        let sign_in = guide(&service("claude-code", "Claude Code", State::SignIn)).unwrap();
        assert_eq!(sign_in.steps[0].mark, Mark::Done);
        assert_eq!(sign_in.steps[1].sign_in, Some("claude auth login"));
    }

    #[test]
    fn a_named_sign_in_that_is_not_ready_is_a_step() {
        let mut claude = service("claude-code", "Claude Code", State::Ready);
        let login = |name: &str, key_var: Option<&str>| crate::logins::Login {
            name: name.to_string(),
            key_var: key_var.map(str::to_string),
        };
        claude.logins = vec![
            LoginState { login: login("Home", None), ready: true, text: String::new() },
            LoginState { login: login("Work", None), ready: false, text: String::new() },
            LoginState { login: login("Paid", Some("WORK_KEY")), ready: false, text: String::new() },
        ];
        let guide = guide(&claude).unwrap();
        assert_eq!(guide.left(), 2);
        assert_eq!(guide.steps[1].title, "Set up the sign-in \"Work\"");
        assert!(guide.steps[2].detail.contains("WORK_KEY is not set"));
    }

    #[test]
    fn only_guides_with_something_to_do_are_shown_and_ignored_tools_are_left_out() {
        let services = [
            service("cursor", "Cursor", State::AgentMissing),
            // Nothing here is for the user to do, so it is not a reminder.
            service("cursor", "Cursor", State::Unknown),
            service("ollama", "Ollama", State::Stopped),
            service("claude-code", "Claude Code", State::SignIn),
        ];
        let ids = |ignored: &[String]| guides(&services, ignored).iter().map(|g| g.id).collect::<Vec<_>>();
        assert_eq!(ids(&[]), ["claude-code", "cursor", "ollama"]);
        assert_eq!(ids(&["cursor".to_string()]), ["claude-code", "ollama"]);
    }
}

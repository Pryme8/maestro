//! Phase 3: recognise AI agents among the running processes.
//!
//! An agent is found by its program name and where its file lives. Tools that
//! run inside a general interpreter, such as Node or Python, are found by
//! their command line, which is read once per process and not kept.

use crate::telemetry::{Proc, Snapshot};
use std::collections::HashMap;

pub struct Kind {
    /// Stable name used in saved settings and to match incoming telemetry.
    pub id: &'static str,
    pub label: &'static str,
    pub vendor: &'static str,
    pub what: &'static str,
}

pub const KINDS: &[Kind] = &[
    Kind { id: "claude-code", label: "Claude Code", vendor: "Anthropic", what: "coding agent" },
    Kind { id: "claude-app", label: "Claude", vendor: "Anthropic", what: "desktop app" },
    Kind { id: "cursor", label: "Cursor", vendor: "Anysphere", what: "editor and agent" },
    Kind { id: "codex", label: "Codex", vendor: "OpenAI", what: "coding agent" },
    Kind { id: "chatgpt", label: "ChatGPT", vendor: "OpenAI", what: "desktop app" },
    Kind { id: "copilot", label: "Copilot", vendor: "Microsoft", what: "assistant" },
    Kind { id: "copilot-cli", label: "Copilot CLI", vendor: "GitHub", what: "coding agent" },
    Kind { id: "gemini-cli", label: "Gemini CLI", vendor: "Google", what: "coding agent" },
    Kind { id: "aider", label: "Aider", vendor: "Aider", what: "coding agent" },
    Kind { id: "windsurf", label: "Windsurf", vendor: "Cognition", what: "editor and agent" },
    Kind { id: "ollama", label: "Ollama", vendor: "Ollama", what: "local models" },
    Kind { id: "lm-studio", label: "LM Studio", vendor: "LM Studio", what: "local models" },
];

pub fn kind_by_id(id: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.id == id)
}

/// A program name, optionally narrowed by where its file lives. All lower case.
struct Rule {
    kind: &'static str,
    names: &'static [&'static str],
    /// When not empty, the file's path must contain one of these.
    path_has: &'static [&'static str],
}

const RULES: &[Rule] = &[
    // The Claude desktop app and Claude Code share a program name, so the
    // file's location tells them apart. The narrower rule comes first.
    Rule { kind: "claude-code", names: &["claude.exe", "claude"], path_has: &["claude-code", "\\.local\\bin", "/.local/bin", "\\.claude\\", "/.claude/"] },
    Rule { kind: "claude-app", names: &["claude.exe", "claude"], path_has: &[] },
    Rule { kind: "cursor", names: &["cursor.exe", "cursor", "cursor-agent.exe", "cursor-agent"], path_has: &[] },
    Rule { kind: "codex", names: &["codex.exe", "codex"], path_has: &[] },
    Rule { kind: "chatgpt", names: &["chatgpt.exe", "chatgpt"], path_has: &[] },
    Rule { kind: "copilot", names: &["copilotapp.exe", "copilotapphost.exe"], path_has: &[] },
    Rule { kind: "windsurf", names: &["windsurf.exe", "windsurf"], path_has: &[] },
    Rule { kind: "ollama", names: &["ollama.exe", "ollama app.exe", "ollama"], path_has: &[] },
    Rule { kind: "lm-studio", names: &["lm studio.exe", "lms.exe", "lm-studio"], path_has: &[] },
];

/// Interpreters whose command line says what they are really running.
const INTERPRETERS: &[&str] = &[
    "node.exe", "node", "bun.exe", "bun", "deno.exe", "deno", "python.exe", "pythonw.exe",
    "python", "python3",
];

/// Text in an interpreter's command line that marks an agent. All lower case.
const COMMAND_RULES: &[(&str, &[&str])] = &[
    ("claude-code", &["@anthropic-ai/claude-code", "claude-code/cli"]),
    ("codex", &["@openai/codex"]),
    ("copilot-cli", &["@github/copilot"]),
    ("gemini-cli", &["@google/gemini-cli", "gemini-cli"]),
    ("cursor", &["cursor-agent"]),
    ("aider", &["aider"]),
];

/// One kind of agent found running, with everything it is using.
pub struct Agent<'a> {
    pub kind: &'static Kind,
    /// The agent's own processes.
    pub procs: Vec<&'a Proc>,
    /// Processes the agent started: shells, tools and scripts working for it.
    pub helpers: Vec<&'a Proc>,
    /// How many separate copies are running.
    pub sessions: usize,
    /// Processor share and memory of the agent and its helpers together.
    pub cpu: f32,
    pub mem: u64,
}

#[derive(Default)]
pub struct Recogniser {
    /// What each interpreter process turned out to be, so its command line is
    /// read only once. Keyed by process number and start time.
    interpreters: HashMap<(u32, u64), Option<&'static str>>,
}

impl Recogniser {
    pub fn find<'a>(&mut self, snap: &'a Snapshot) -> Vec<Agent<'a>> {
        let mut kind_of: HashMap<u32, &'static str> = HashMap::new();
        let mut still_here = HashMap::new();
        for p in &snap.processes {
            let name = p.name.to_lowercase();
            let kind = if INTERPRETERS.contains(&name.as_str()) {
                let key = (p.pid, p.start_secs);
                let kind = *self
                    .interpreters
                    .entry(key)
                    .or_insert_with(|| from_command_line(p.pid));
                still_here.insert(key, kind);
                kind
            } else {
                from_name(&name, p.exe.as_deref())
            };
            if let Some(kind) = kind {
                kind_of.insert(p.pid, kind);
            }
        }
        self.interpreters = still_here;
        build(snap, &kind_of)
    }
}

fn from_name(name: &str, exe: Option<&str>) -> Option<&'static str> {
    let path = exe.map(str::to_lowercase).unwrap_or_default();
    RULES
        .iter()
        .find(|rule| {
            rule.names.contains(&name)
                && (rule.path_has.is_empty() || rule.path_has.iter().any(|part| path.contains(part)))
        })
        .map(|rule| rule.kind)
}

fn from_command_line(pid: u32) -> Option<&'static str> {
    let line = command_line(pid)?.to_lowercase();
    match_command_line(&line)
}

fn match_command_line(line: &str) -> Option<&'static str> {
    COMMAND_RULES
        .iter()
        .find(|(_, marks)| marks.iter().any(|mark| line.contains(mark)))
        .map(|(kind, _)| *kind)
}

#[cfg(all(windows, target_pointer_width = "64"))]
fn command_line(pid: u32) -> Option<String> {
    crate::winproc::command_line(pid)
}

#[cfg(not(all(windows, target_pointer_width = "64")))]
fn command_line(_pid: u32) -> Option<String> {
    None
}

/// Group the recognised processes by kind and add the processes they started.
fn build<'a>(snap: &'a Snapshot, kind_of: &HashMap<u32, &'static str>) -> Vec<Agent<'a>> {
    let by_pid: HashMap<u32, &Proc> = snap.processes.iter().map(|p| (p.pid, p)).collect();
    let parent_of = |p: &Proc| {
        p.parent_pid
            .and_then(|pid| by_pid.get(&pid).copied())
            // Process numbers are reused, so a parent that started later is unrelated.
            .filter(|parent| parent.pid != p.pid && parent.start_secs <= p.start_secs)
    };
    let mut children: HashMap<u32, Vec<&Proc>> = HashMap::new();
    for p in &snap.processes {
        if let Some(parent) = parent_of(p) {
            children.entry(parent.pid).or_default().push(p);
        }
    }

    let mut agents = Vec::new();
    for kind in KINDS {
        let procs: Vec<&Proc> = snap
            .processes
            .iter()
            .filter(|p| kind_of.get(&p.pid) == Some(&kind.id))
            .collect();
        if procs.is_empty() {
            continue;
        }
        // A copy starts wherever the process that started it is not the same agent.
        let sessions = procs
            .iter()
            .filter(|p| parent_of(p).is_none_or(|parent| kind_of.get(&parent.pid) != Some(&kind.id)))
            .count();
        // Helpers are everything started beneath the agent, stopping at any
        // other agent: that one's helpers are its own.
        let mut helpers = Vec::new();
        let mut queue: Vec<&Proc> = procs.clone();
        let mut depth = 0;
        while let Some(p) = queue.pop() {
            depth += 1;
            if depth > 100_000 {
                break;
            }
            for kid in children.get(&p.pid).into_iter().flatten() {
                if kind_of.contains_key(&kid.pid) {
                    continue;
                }
                helpers.push(*kid);
                queue.push(kid);
            }
        }
        let all = procs.iter().chain(&helpers);
        let cpu = all.clone().map(|p| p.cpu_percent).sum();
        let mem = all.map(|p| p.memory_bytes).sum();
        agents.push(Agent {
            kind,
            procs,
            helpers,
            sessions,
            cpu,
            mem,
        });
    }
    agents
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claude_code_and_the_claude_app_are_told_apart_by_location() {
        let code = "C:\\Users\\someone\\AppData\\Local\\Claude-Data\\x\\claude-code\\2.1.330\\abc\\claude.exe";
        let app = "C:\\Program Files\\WindowsApps\\Claude_6.2_x64__abc\\app\\claude.exe";
        assert_eq!(from_name("claude.exe", Some(code)), Some("claude-code"));
        assert_eq!(from_name("claude.exe", Some(app)), Some("claude-app"));
        // With no readable path the general rule applies.
        assert_eq!(from_name("claude.exe", None), Some("claude-app"));
        assert_eq!(from_name("ollama.exe", None), Some("ollama"));
        assert_eq!(from_name("notepad.exe", None), None);
    }

    #[test]
    fn interpreters_are_recognised_by_command_line() {
        let line = "node c:\\tools\\node_modules\\@openai\\codex\\bin\\codex.js".replace('\\', "/");
        assert_eq!(match_command_line(&line), Some("codex"));
        assert_eq!(
            match_command_line("node /usr/lib/node_modules/@anthropic-ai/claude-code/cli.js"),
            Some("claude-code")
        );
        assert_eq!(match_command_line("node server.js"), None);
    }

    #[test]
    fn every_rule_names_a_known_kind() {
        for rule in RULES {
            assert!(kind_by_id(rule.kind).is_some(), "{}", rule.kind);
        }
        for (kind, _) in COMMAND_RULES {
            assert!(kind_by_id(kind).is_some(), "{kind}");
        }
    }
}

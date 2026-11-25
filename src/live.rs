//! What agents are doing, step by step, as it happens.
//!
//! Two sources feed the same list of steps. Sessions started anywhere are
//! followed by reading the session files Claude Code writes on this computer.
//! Sessions started from Maestro are read from the tool's own event stream,
//! and can be sent messages, interrupted and stopped.
//!
//! Steps are kept in memory only, and nothing read here is written to disk
//! or sent anywhere.

use serde_json::{json, Value};
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{Arc, Mutex};

/// Steps kept per session; older ones are dropped.
const STEPS_KEPT: usize = 600;
/// A session file untouched for this long is not picked up.
const FRESH_MS: u64 = 60 * 60 * 1000;
/// How much of a file's end is read when it is first seen.
const BACKFILL: u64 = 256 * 1024;
/// Lines longer than this hold embedded files and are skipped unread.
const LINE_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub enum What {
    /// The tool started up.
    Start { model: String },
    /// Something a person, or Maestro for them, told the agent.
    Asked(String),
    /// The agent's own words.
    Said(String),
    Thinking,
    /// The agent called a tool. `detail` is the part of the call worth
    /// showing: the command, the file, the search.
    Tool { id: String, name: String, detail: String },
    /// A tool call came back.
    ToolDone { name: String, ok: bool, size: usize, preview: String },
    /// One request was answered in full.
    Finished { ok: bool, cost: Option<f64>, text: String },
    /// The agent wants to take a step and is waiting to be told it may.
    NeedsYou { tool: String, detail: String },
    /// A person answered that question.
    Decided { tool: String, allowed: bool },
    /// Something went wrong outside the conversation.
    Problem(String),
    /// The process ended.
    Ended,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Step {
    pub at_ms: u64,
    pub what: What,
}

/// What one line of output or one line of a session file says.
#[derive(Default, Debug)]
pub struct Parsed {
    pub session: Option<String>,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub steps: Vec<Step>,
}

fn clip(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let head: String = text.chars().take(max).collect();
        format!("{head}…")
    }
}

/// The part of a tool call worth a line on screen.
fn detail(input: &Value) -> String {
    const KEYS: [&str; 10] = [
        "command", "file_path", "path", "pattern", "url", "query", "description", "prompt", "skill", "message",
    ];
    let Some(map) = input.as_object() else {
        return String::new();
    };
    let found = KEYS
        .iter()
        .find_map(|key| map.get(*key).and_then(Value::as_str))
        .or_else(|| map.values().find_map(Value::as_str));
    clip(found.unwrap_or(""), 300)
}

/// The size and first words of what a tool sent back.
fn result_text(content: &Value) -> (usize, String) {
    match content {
        Value::String(text) => (text.len(), clip(text, 300)),
        Value::Array(blocks) => {
            let mut size = 0;
            let mut first = String::new();
            for block in blocks {
                match block.get("text").and_then(Value::as_str) {
                    Some(text) => {
                        size += text.len();
                        if first.is_empty() {
                            first = clip(text, 300);
                        }
                    }
                    // An image or a document: count it, do not show it.
                    None => size += block.to_string().len(),
                }
            }
            (size, first)
        }
        _ => (0, String::new()),
    }
}

/// Milliseconds since 1970 from a time such as `2026-11-25T19:16:03.804Z`.
fn parse_time(text: &str) -> Option<u64> {
    let num = |range: std::ops::Range<usize>| text.get(range)?.parse::<u64>().ok();
    let (year, month, day) = (num(0..4)?, num(5..7)?, num(8..10)?);
    let (hour, minute, second) = (num(11..13)?, num(14..16)?, num(17..19)?);
    let millis = if text.as_bytes().get(19) == Some(&b'.') {
        num(20..23).unwrap_or(0)
    } else {
        0
    };
    // Days from the civil date, counting years from March.
    let y = if month <= 2 { year - 1 } else { year };
    let era = y / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = (era * 146_097 + doe).checked_sub(719_468)?;
    Some(((days * 24 + hour) * 60 + minute) * 60_000 + second * 1000 + millis)
}

/// Read one line of Claude Code's event stream or of one of its session
/// files. Both use the same shapes for what was said and which tools ran.
pub fn parse_claude(line: &str, now_ms: u64) -> Option<Parsed> {
    if line.len() > LINE_LIMIT {
        return None;
    }
    let v: Value = serde_json::from_str(line).ok()?;
    let kind = v.get("type")?.as_str()?;
    let text_of = |key: &str| v.get(key).and_then(Value::as_str).map(str::to_string);
    let mut out = Parsed {
        session: text_of("session_id").or_else(|| text_of("sessionId")),
        cwd: text_of("cwd"),
        ..Default::default()
    };
    let at_ms = v
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_time)
        .unwrap_or(now_ms);
    let mut push = |what: What| out.steps.push(Step { at_ms, what });
    let flag = |key: &str| v.get(key).and_then(Value::as_bool).unwrap_or(false);

    match kind {
        "system" if v.get("subtype").and_then(Value::as_str) == Some("init") => push(What::Start {
            model: text_of("model").unwrap_or_default(),
        }),
        "assistant" => {
            let blocks = v.pointer("/message/content").and_then(Value::as_array);
            for block in blocks.into_iter().flatten() {
                match block.get("type").and_then(Value::as_str) {
                    Some("text") => {
                        let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                        if !text.trim().is_empty() {
                            push(What::Said(clip(text, 4000)));
                        }
                    }
                    Some("thinking") => push(What::Thinking),
                    Some("tool_use") => push(What::Tool {
                        id: block.get("id").and_then(Value::as_str).unwrap_or("").to_string(),
                        name: block.get("name").and_then(Value::as_str).unwrap_or("tool").to_string(),
                        detail: detail(block.get("input").unwrap_or(&Value::Null)),
                    }),
                    _ => {}
                }
            }
        }
        // Notes the tool writes to itself are not something anyone asked.
        "user" if !flag("isMeta") && !flag("isCompactSummary") => {
            match v.pointer("/message/content") {
                Some(Value::String(text)) => push(What::Asked(clip(text, 2000))),
                Some(Value::Array(blocks)) => {
                    for block in blocks {
                        match block.get("type").and_then(Value::as_str) {
                            Some("tool_result") => {
                                let (size, preview) = result_text(block.get("content").unwrap_or(&Value::Null));
                                push(What::ToolDone {
                                    // The call's id for now; `Feed` swaps in the tool's name.
                                    name: block.get("tool_use_id").and_then(Value::as_str).unwrap_or("").to_string(),
                                    ok: !block.get("is_error").and_then(Value::as_bool).unwrap_or(false),
                                    size,
                                    preview,
                                });
                            }
                            Some("text") => {
                                let text = block.get("text").and_then(Value::as_str).unwrap_or("");
                                push(What::Asked(clip(text, 2000)));
                            }
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
        }
        "result" => push(What::Finished {
            ok: !flag("is_error"),
            cost: v.get("total_cost_usd").and_then(Value::as_f64),
            text: clip(v.get("result").and_then(Value::as_str).unwrap_or(""), 2000),
        }),
        "custom-title" => out.title = text_of("customTitle"),
        _ => {}
    }
    (out.session.is_some() || !out.steps.is_empty() || out.title.is_some()).then_some(out)
}

/// One agent session and what it has done lately.
pub struct Session {
    pub id: String,
    pub title: Option<String>,
    pub cwd: String,
    /// Started from Maestro, so it can be directed from here.
    pub ours: bool,
    pub steps: VecDeque<Step>,
    pub last_ms: u64,
    /// How often each tool has been called, in the steps seen.
    pub tool_counts: HashMap<String, u32>,
    /// Tool calls not yet answered, by call id.
    calls: HashMap<String, String>,
    /// Steps waiting for a yes or a no, oldest first.
    pub asks: Vec<Ask>,
}

impl Session {
    fn new(id: &str, ours: bool) -> Self {
        Self {
            id: id.to_string(),
            title: None,
            cwd: String::new(),
            ours,
            steps: VecDeque::new(),
            last_ms: 0,
            tool_counts: HashMap::new(),
            calls: HashMap::new(),
            asks: Vec::new(),
        }
    }

    /// A name for the list: the title if it has one, else the folder.
    pub fn name(&self) -> String {
        if let Some(title) = &self.title {
            return title.clone();
        }
        let folder = self.cwd.rsplit(['\\', '/']).find(|part| !part.is_empty());
        folder.unwrap_or(&self.id).to_string()
    }

    /// Tools the agent has called and not heard back from.
    pub fn waiting_on(&self) -> Vec<&str> {
        self.calls.values().map(String::as_str).collect()
    }
}

/// Every session being followed.
#[derive(Default)]
pub struct Feed {
    pub sessions: Vec<Session>,
    /// Real session ids of runs started here. Their files are not followed
    /// as well, or every step would show twice.
    started: HashSet<String>,
}

impl Feed {
    pub fn add(&mut self, id: &str, ours: bool, parsed: Parsed) {
        let index = match self.sessions.iter().position(|s| s.id == id) {
            Some(index) => index,
            None => {
                self.sessions.push(Session::new(id, ours));
                self.sessions.len() - 1
            }
        };
        let session = &mut self.sessions[index];
        if let Some(cwd) = parsed.cwd {
            session.cwd = cwd;
        }
        if parsed.title.is_some() {
            session.title = parsed.title;
        }
        for mut step in parsed.steps {
            match &mut step.what {
                What::Tool { id, name, .. } => {
                    *session.tool_counts.entry(name.clone()).or_default() += 1;
                    session.calls.insert(id.clone(), name.clone());
                }
                What::ToolDone { name, .. } => {
                    *name = session.calls.remove(name.as_str()).unwrap_or_else(|| "tool".to_string());
                }
                // Whatever was still open when a request ended will not come back.
                What::Finished { .. } | What::Ended => session.calls.clear(),
                _ => {}
            }
            session.last_ms = session.last_ms.max(step.at_ms);
            session.steps.push_back(step);
            if session.steps.len() > STEPS_KEPT {
                session.steps.pop_front();
            }
        }
    }

    fn session(&mut self, id: &str) -> &mut Session {
        let index = match self.sessions.iter().position(|s| s.id == id) {
            Some(index) => index,
            None => {
                self.sessions.push(Session::new(id, true));
                self.sessions.len() - 1
            }
        };
        &mut self.sessions[index]
    }

    /// A step is waiting for a yes or a no.
    pub fn ask(&mut self, id: &str, ask: Ask) {
        let session = self.session(id);
        session.last_ms = session.last_ms.max(ask.at_ms);
        session.steps.push_back(Step {
            at_ms: ask.at_ms,
            what: What::NeedsYou {
                tool: ask.tool.clone(),
                detail: ask.detail.clone(),
            },
        });
        session.asks.push(ask);
    }

    /// Take a question off the list. `allowed` is the person's answer, or
    /// `None` when the tool withdrew the question itself.
    pub fn settle(&mut self, id: &str, request_id: &str, allowed: Option<bool>) -> Option<Ask> {
        let session = self.sessions.iter_mut().find(|s| s.id == id)?;
        let at = session.asks.iter().position(|a| a.request_id == request_id)?;
        let ask = session.asks.remove(at);
        if let Some(allowed) = allowed {
            let now = now_ms();
            session.last_ms = session.last_ms.max(now);
            session.steps.push_back(Step {
                at_ms: now,
                what: What::Decided {
                    tool: ask.tool.clone(),
                    allowed,
                },
            });
        }
        Some(ask)
    }

    /// Forget sessions that have been quiet for a long time, except ones started here.
    fn tidy(&mut self, now_ms: u64) {
        self.sessions
            .retain(|s| s.ours || now_ms.saturating_sub(s.last_ms) < 4 * FRESH_MS);
    }
}

pub fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ---- following sessions started anywhere ---------------------------------

/// Where Claude Code keeps its session files.
fn claude_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("MAESTRO_CLAUDE_DIR").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME"))?;
    Some(PathBuf::from(home).join(".claude").join("projects"))
}

/// How far into one session file has been read.
#[derive(Default)]
struct Tail {
    /// True when reading began mid-file, so the first line read is a fragment.
    skip_first: bool,
    offset: u64,
    /// The start of a line whose end has not been written yet.
    rest: Vec<u8>,
}

impl Tail {
    /// Read whatever has been added to the file and return the whole lines.
    fn read(&mut self, path: &Path) -> Vec<String> {
        let Ok(mut file) = std::fs::File::open(path) else {
            return Vec::new();
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // The file was replaced by a shorter one.
            self.offset = 0;
            self.rest.clear();
        }
        if len == self.offset || file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut added = Vec::new();
        // No more than 8 MB a look, so one huge file cannot stall the rest.
        let _ = file.take(8 * 1024 * 1024).read_to_end(&mut added);
        self.offset += added.len() as u64;
        self.rest.extend_from_slice(&added);
        let mut lines = Vec::new();
        if self.skip_first {
            match self.rest.iter().position(|b| *b == b'\n') {
                Some(end) => {
                    self.rest.drain(..=end);
                    self.skip_first = false;
                }
                None => return lines,
            }
        }
        while let Some(end) = self.rest.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.rest.drain(..=end).collect();
            if line.len() <= LINE_LIMIT {
                lines.push(String::from_utf8_lossy(&line).trim().to_string());
            }
        }
        if self.rest.len() > LINE_LIMIT {
            // An oversized line still being written: drop what there is of it.
            self.rest.clear();
        }
        lines
    }
}

/// Follows session files as they grow.
#[derive(Default)]
pub struct Watcher {
    tails: HashMap<PathBuf, Tail>,
}

impl Watcher {
    /// Look for session files touched lately and start following new ones
    /// from near their end.
    fn scan(&mut self, dir: &Path, now_ms: u64) {
        let Ok(projects) = std::fs::read_dir(dir) else { return };
        for project in projects.flatten() {
            let Ok(files) = std::fs::read_dir(project.path()) else { continue };
            for file in files.flatten() {
                let path = file.path();
                if path.extension().is_none_or(|e| e != "jsonl") || self.tails.contains_key(&path) {
                    continue;
                }
                let Ok(meta) = file.metadata() else { continue };
                let modified = meta
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |d| d.as_millis() as u64);
                if now_ms.saturating_sub(modified) > FRESH_MS {
                    continue;
                }
                let offset = meta.len().saturating_sub(BACKFILL);
                self.tails.insert(
                    path,
                    Tail {
                        skip_first: offset > 0,
                        offset,
                        rest: Vec::new(),
                    },
                );
            }
        }
    }

    /// Read what every followed file has gained and add it to the feed.
    /// Returns true when anything new was added.
    pub fn poll(&mut self, dir: &Path, feed: &Mutex<Feed>, rescan: bool) -> bool {
        let now = now_ms();
        if rescan {
            self.scan(dir, now);
        }
        let mut changed = false;
        for (path, tail) in &mut self.tails {
            let lines = tail.read(path);
            if lines.is_empty() {
                continue;
            }
            let id = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            let Ok(mut feed) = feed.lock() else { return changed };
            if feed.started.contains(&id) {
                continue;
            }
            for line in lines {
                if let Some(parsed) = parse_claude(&line, now) {
                    changed |= !parsed.steps.is_empty() || parsed.title.is_some();
                    feed.add(&id, false, parsed);
                }
            }
        }
        if rescan {
            if let Ok(mut feed) = feed.lock() {
                feed.tidy(now);
            }
        }
        changed
    }
}

/// Follow Claude Code's session files on a background thread.
pub fn watch(feed: Arc<Mutex<Feed>>, wake: impl Fn() + Send + 'static) {
    let Some(dir) = claude_dir() else { return };
    std::thread::spawn(move || {
        let mut watcher = Watcher::default();
        for tick in 0u64.. {
            if watcher.poll(&dir, &feed, tick % 5 == 0) {
                wake();
            }
            std::thread::sleep(std::time::Duration::from_millis(1000));
        }
    });
}

// ---- sessions started from Maestro ----------------------------------------

/// What a run started here may do without asking.
#[derive(Clone, Copy, PartialEq)]
pub enum Leave {
    /// Read and search. Anything that would need approval is refused.
    ReadOnly,
    /// Every step that needs approval is put to the person directing it.
    AskFirst,
    /// Create and change files in its folder without asking; everything
    /// else that needs approval is still asked.
    EditFiles,
}

/// A Claude Code session started by Maestro and still attached to it.
pub struct Run {
    /// The session's id in the feed.
    pub id: String,
    pub pid: u32,
    child: Child,
    stdin: Option<ChildStdin>,
    feed: Arc<Mutex<Feed>>,
    requests: u32,
}

/// The arguments for a run that reports every step and takes messages while
/// it works. Claude Code's own terms allow another program to start the
/// unmodified tool on your own login; see docs/research/relay.md.
pub fn claude_args(leave: Leave) -> Vec<&'static str> {
    let mut args = vec![
        "-p",
        "--output-format",
        "stream-json",
        "--input-format",
        "stream-json",
        "--verbose",
    ];
    match leave {
        // Nobody answers, so the tool refuses whatever it would have asked.
        Leave::ReadOnly => args.extend(["--permission-prompts", "none"]),
        // Questions come to whoever started it, over the same stream.
        Leave::AskFirst => args.extend(["--permission-prompt-tool", "stdio"]),
        Leave::EditFiles => args.extend([
            "--permission-mode",
            "acceptEdits",
            "--permission-prompt-tool",
            "stdio",
        ]),
    }
    args
}

fn user_line(text: &str) -> String {
    json!({"type": "user", "message": {"role": "user", "content": [{"type": "text", "text": text}]}}).to_string()
}

static RUNS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(1);

fn one_step(what: What) -> Parsed {
    Parsed {
        steps: vec![Step {
            at_ms: now_ms(),
            what,
        }],
        ..Default::default()
    }
}

impl Run {
    /// Start Claude Code in `cwd` with a first request. No window is opened.
    pub fn start(
        feed: Arc<Mutex<Feed>>,
        cwd: &Path,
        prompt: &str,
        leave: Leave,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Result<Run, String> {
        if !cwd.is_dir() {
            return Err(format!("{} is not a folder.", cwd.display()));
        }
        if prompt.trim().is_empty() {
            return Err("There is nothing to ask.".to_string());
        }
        let mut command = Command::new("claude");
        command
            .args(claude_args(leave))
            .current_dir(cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            // Without this, Windows opens a console window for the tool.
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            command.creation_flags(CREATE_NO_WINDOW);
        }
        let mut child = command.spawn().map_err(|e| {
            format!("Could not start Claude Code. Is the `claude` command installed and on the path? ({e})")
        })?;
        let number = RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let id = format!("maestro-run-{number}");
        let wake = Arc::new(wake);

        if let Ok(mut feed) = feed.lock() {
            let mut first = one_step(What::Asked(clip(prompt, 2000)));
            first.cwd = Some(cwd.to_string_lossy().into_owned());
            first.title = Some(format!("Run {number}: {}", clip(prompt, 40)));
            feed.add(&id, true, first);
        }

        if let Some(stdout) = child.stdout.take() {
            let (feed, id, wake) = (feed.clone(), id.clone(), wake.clone());
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                    // A question about a step, or the tool taking one back.
                    if let Some(control) = parse_control(&line, now_ms()) {
                        if let Ok(mut feed) = feed.lock() {
                            match control {
                                Control::Ask(ask) => feed.ask(&id, ask),
                                Control::Withdrawn(request) => {
                                    feed.settle(&id, &request, None);
                                }
                            }
                        }
                        wake();
                        continue;
                    }
                    let Some(mut parsed) = parse_claude(&line, now_ms()) else { continue };
                    // What was asked is added when it is sent, so it is not taken from the echo.
                    parsed.steps.retain(|s| !matches!(s.what, What::Asked(_)));
                    parsed.title = None;
                    if let Ok(mut feed) = feed.lock() {
                        if let Some(real) = &parsed.session {
                            feed.started.insert(real.clone());
                        }
                        feed.add(&id, true, parsed);
                    }
                    wake();
                }
                if let Ok(mut feed) = feed.lock() {
                    feed.add(&id, true, one_step(What::Ended));
                }
                wake();
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let (feed, id) = (feed.clone(), id.clone());
            std::thread::spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Ok(mut feed) = feed.lock() {
                        feed.add(&id, true, one_step(What::Problem(clip(&line, 500))));
                    }
                    wake();
                }
            });
        }

        let mut run = Run {
            id,
            pid: child.id(),
            stdin: child.stdin.take(),
            child,
            feed,
            requests: 0,
        };
        run.write(&user_line(prompt))?;
        Ok(run)
    }

    fn write(&mut self, line: &str) -> Result<(), String> {
        let stdin = self.stdin.as_mut().ok_or("This run is no longer taking messages.")?;
        writeln!(stdin, "{line}")
            .and_then(|()| stdin.flush())
            .map_err(|e| format!("The run did not take the message. ({e})"))
    }

    /// Tell the agent something. It is read when the agent next looks for input.
    pub fn say(&mut self, text: &str) -> Result<(), String> {
        self.write(&user_line(text))?;
        if let Ok(mut feed) = self.feed.lock() {
            feed.add(&self.id, true, one_step(What::Asked(clip(text, 2000))));
        }
        Ok(())
    }

    /// Ask the agent to stop what it is doing now and wait for the next message.
    pub fn interrupt(&mut self) -> Result<(), String> {
        self.requests += 1;
        let line = json!({
            "type": "control_request",
            "request_id": format!("maestro-{}", self.requests),
            "request": {"subtype": "interrupt"},
        });
        self.write(&line.to_string())
    }

    /// Answer a question the agent asked before a step. `why` goes back to
    /// the agent with a refusal, so it can try another way.
    pub fn answer(&mut self, request_id: &str, allow: bool, why: &str) -> Result<(), String> {
        let ask = self
            .feed
            .lock()
            .ok()
            .and_then(|feed| {
                let session = feed.sessions.iter().find(|s| s.id == self.id)?;
                session.asks.iter().find(|a| a.request_id == request_id).cloned()
            })
            .ok_or("That question is no longer waiting.")?;
        self.write(&answer_line(&ask, allow, why))?;
        if let Ok(mut feed) = self.feed.lock() {
            feed.settle(&self.id, request_id, Some(allow));
        }
        Ok(())
    }

    /// True while the run still takes messages.
    pub fn open(&self) -> bool {
        self.stdin.is_some()
    }

    /// Take no more messages. The agent finishes what it has and exits.
    pub fn finish(&mut self) {
        self.stdin = None;
    }

    /// End the process now.
    pub fn stop(&mut self) {
        self.stdin = None;
        let _ = self.child.kill();
    }

    pub fn running(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }
}

impl Drop for Run {
    /// A run must not outlive the window that can stop it.
    fn drop(&mut self) {
        self.stop();
    }
}

// ---- questions the agent asks before a step ---------------------------------

/// A step the agent may not take until someone says yes or no.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    /// The tool's own number for the question, sent back with the answer.
    pub request_id: String,
    pub tool: String,
    /// The part of the call worth a line: the command, the file, the site.
    pub detail: String,
    /// The tool's own words for what it wants, when it gives any.
    pub description: String,
    /// Why it has to ask, when it says.
    pub reason: String,
    /// The whole call, handed back unchanged when it is allowed.
    pub input: Value,
    pub tool_use_id: Option<String>,
    pub at_ms: u64,
}

/// What a line on the stream says about permission, if anything.
#[derive(Debug, PartialEq)]
pub enum Control {
    Ask(Ask),
    /// The tool no longer needs an answer to this question.
    Withdrawn(String),
}

/// Read a permission question, or its withdrawal, from one line of the stream.
pub fn parse_control(line: &str, now_ms: u64) -> Option<Control> {
    let v: Value = serde_json::from_str(line).ok()?;
    let text = |v: &Value, key: &str| v.get(key).and_then(Value::as_str).unwrap_or("").to_string();
    match v.get("type")?.as_str()? {
        "control_request" => {
            let request = v.get("request")?;
            if request.get("subtype")?.as_str()? != "can_use_tool" {
                return None;
            }
            let input = request.get("input").cloned().unwrap_or(Value::Null);
            Some(Control::Ask(Ask {
                request_id: text(&v, "request_id"),
                tool: text(request, "tool_name"),
                detail: detail(&input),
                description: clip(&text(request, "description"), 600),
                reason: clip(&text(request, "decision_reason"), 600),
                input,
                tool_use_id: request.get("tool_use_id").and_then(Value::as_str).map(str::to_string),
                at_ms: now_ms,
            }))
        }
        "control_cancel_request" => Some(Control::Withdrawn(text(&v, "request_id"))),
        _ => None,
    }
}

/// The line that answers a permission question.
pub fn answer_line(ask: &Ask, allow: bool, why: &str) -> String {
    let mut decision = if allow {
        json!({"behavior": "allow", "updatedInput": ask.input})
    } else {
        let why = if why.trim().is_empty() {
            "The person directing this session refused this step."
        } else {
            why.trim()
        };
        json!({"behavior": "deny", "message": why})
    };
    if let Some(id) = &ask.tool_use_id {
        decision["toolUseID"] = json!(id);
    }
    json!({
        "type": "control_response",
        "response": {"subtype": "success", "request_id": ask.request_id, "response": decision},
    })
    .to_string()
}

// ---- the session as a map ---------------------------------------------------

/// One thing a tool was pointed at: a file, a command, a site.
#[derive(Debug, PartialEq)]
pub struct Leaf {
    pub label: String,
    /// The whole of what the label was cut from.
    pub full: String,
    pub count: u32,
    pub last_ms: u64,
}

/// One tool and what it has been used on.
#[derive(Debug, PartialEq)]
pub struct Branch {
    pub tool: String,
    pub count: u32,
    pub last_ms: u64,
    /// When a call to it last failed.
    pub failed_ms: Option<u64>,
    /// A call to it has not come back yet.
    pub waiting: bool,
    /// Most recently used first.
    pub leaves: Vec<Leaf>,
}

/// A tool's name without the prefix connectors add, such as `mcp__server__`.
pub fn short_tool(name: &str) -> &str {
    name.rsplit("__").next().filter(|s| !s.is_empty()).unwrap_or(name)
}

/// A short name for what a tool call was pointed at.
pub fn target(tool: &str, detail: &str) -> String {
    let detail = detail.trim();
    if detail.is_empty() {
        return String::new();
    }
    // A site: show who it is, not the whole address.
    if let Some((_, rest)) = detail.split_once("://") {
        let host = rest.split(['/', '?', '#', ' ']).next().unwrap_or(rest);
        return clip(host, 30);
    }
    // A command: the program and its first word say what kind of thing ran.
    if matches!(tool, "Bash" | "PowerShell") {
        // After `cd somewhere &&` comes the command that matters.
        let command = match detail.strip_prefix("cd ") {
            Some(rest) => rest.split_once("&&").or_else(|| rest.split_once(';')).map_or(detail, |(_, after)| after),
            None => detail,
        };
        let words: Vec<&str> = command
            .split_whitespace()
            .filter(|w| !w.contains('='))
            .take(2)
            .collect();
        return clip(&words.join(" "), 26);
    }
    // A path: the last part is the file or folder.
    let last = detail.rsplit(['/', '\\']).find(|part| !part.is_empty()).unwrap_or(detail);
    if last.len() < detail.len() && !last.contains(' ') {
        return clip(last, 30);
    }
    clip(detail.lines().next().unwrap_or(detail), 26)
}

/// Sort a session's steps into tools and what each was used on, in the order
/// the tools were first called.
pub fn map(session: &Session) -> Vec<Branch> {
    let mut branches: Vec<Branch> = Vec::new();
    for step in &session.steps {
        match &step.what {
            What::Tool { name, detail, .. } => {
                let at = match branches.iter().position(|b| &b.tool == name) {
                    Some(at) => at,
                    None => {
                        branches.push(Branch {
                            tool: name.clone(),
                            count: 0,
                            last_ms: 0,
                            failed_ms: None,
                            waiting: false,
                            leaves: Vec::new(),
                        });
                        branches.len() - 1
                    }
                };
                let branch = &mut branches[at];
                branch.count += 1;
                branch.last_ms = step.at_ms;
                let label = target(name, detail);
                if label.is_empty() {
                    continue;
                }
                match branch.leaves.iter_mut().find(|l| l.label == label) {
                    Some(leaf) => {
                        leaf.count += 1;
                        leaf.last_ms = step.at_ms;
                        leaf.full = detail.clone();
                    }
                    None => branch.leaves.push(Leaf {
                        label,
                        full: detail.clone(),
                        count: 1,
                        last_ms: step.at_ms,
                    }),
                }
            }
            What::ToolDone { name, ok: false, .. } => {
                if let Some(branch) = branches.iter_mut().find(|b| &b.tool == name) {
                    branch.failed_ms = Some(step.at_ms);
                }
            }
            _ => {}
        }
    }
    let waiting = session.waiting_on();
    for branch in &mut branches {
        branch.waiting = waiting.contains(&branch.tool.as_str());
        branch.leaves.sort_by(|a, b| b.last_ms.cmp(&a.last_ms));
    }
    branches
}

// ---- command line ----------------------------------------------------------

fn print_step(name: &str, step: &Step) {
    let secs = step.at_ms / 1000 % 86_400;
    let clock = format!("{:02}:{:02}:{:02}", secs / 3600, secs / 60 % 60, secs % 60);
    let line = match &step.what {
        What::Start { model } => format!("started on {model}"),
        What::Asked(text) => format!("asked: {}", clip(text, 160)),
        What::Said(text) => format!("said: {}", clip(text, 160)),
        What::Thinking => "thinking".to_string(),
        What::Tool { name, detail, .. } => format!("tool {name}: {}", clip(detail, 140)),
        What::ToolDone { name, ok, size, .. } => {
            format!("{name} {} ({size} bytes)", if *ok { "came back" } else { "failed" })
        }
        What::Finished { ok, cost, text } => format!(
            "{}{}: {}",
            if *ok { "finished" } else { "finished with an error" },
            cost.map(|c| format!(" (${c:.4})")).unwrap_or_default(),
            clip(text, 160)
        ),
        What::NeedsYou { tool, detail } => format!("asks to use {tool}: {}", clip(detail, 140)),
        What::Decided { tool, allowed } => {
            format!("{tool} was {}", if *allowed { "allowed" } else { "refused" })
        }
        What::Problem(text) => format!("problem: {text}"),
        What::Ended => "the process ended".to_string(),
    };
    println!("{clock} UTC  [{}]  {}", clip(name, 28), line.replace('\n', " "));
}

/// `--watch`: print what agent sessions on this computer are doing, as it happens.
/// `--watch-once` prints what the last hour's sessions did lately and exits.
/// `--run <folder> <request>` starts Claude Code there, read-only, and prints its steps.
pub fn cli(args: &[String]) -> Option<i32> {
    let after = |flag: &str| args.iter().position(|a| a == flag);
    let feed = Arc::new(Mutex::new(Feed::default()));
    let mut printed: HashMap<String, usize> = HashMap::new();
    let mut show = |feed: &Mutex<Feed>, last: usize| {
        let Ok(feed) = feed.lock() else { return };
        for session in &feed.sessions {
            let seen = printed
                .entry(session.id.clone())
                .or_insert_with(|| session.steps.len().saturating_sub(last));
            // Steps dropped from the front shift the count; start again from what is left.
            *seen = (*seen).min(session.steps.len());
            for step in session.steps.iter().skip(*seen) {
                print_step(&session.name(), step);
            }
            *seen = session.steps.len();
        }
    };

    if let Some(at) = after("--run") {
        let (Some(folder), Some(prompt)) = (args.get(at + 1), args.get(at + 2)) else {
            eprintln!("--run needs a folder and a request.");
            return Some(2);
        };
        // `--answer allow` or `--answer refuse` answers every question the
        // agent asks the same way. Without it the run is read-only.
        let answer = after("--answer").and_then(|i| args.get(i + 1)).map(|a| a == "allow");
        let leave = if answer.is_some() { Leave::AskFirst } else { Leave::ReadOnly };
        let mut run = match Run::start(feed.clone(), Path::new(folder), prompt, leave, || {}) {
            Ok(run) => run,
            Err(problem) => {
                eprintln!("{problem}");
                return Some(1);
            }
        };
        if answer.is_none() {
            // One request, then no more: the tool exits when it has answered.
            run.finish();
        }
        while run.running() {
            std::thread::sleep(std::time::Duration::from_millis(200));
            show(&feed, usize::MAX);
            let (asks, answered) = match feed.lock() {
                Ok(feed) => {
                    let session = feed.sessions.iter().find(|s| s.id == run.id);
                    (
                        session.map(|s| s.asks.clone()).unwrap_or_default(),
                        session.is_some_and(|s| s.steps.iter().any(|s| matches!(s.what, What::Finished { .. }))),
                    )
                }
                Err(_) => break,
            };
            for ask in asks {
                let allow = answer.unwrap_or(false);
                println!("          why it asks: {}", if ask.reason.is_empty() { "-" } else { &ask.reason });
                if let Err(problem) = run.answer(&ask.request_id, allow, "Refused from the command line.") {
                    eprintln!("{problem}");
                }
            }
            // The questions have to be answered over the same line, so it is
            // only closed once the request has been answered in full.
            if answered {
                run.finish();
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(300));
        show(&feed, usize::MAX);
        return Some(0);
    }

    let once = after("--watch-once").is_some();
    if !once && after("--watch").is_none() {
        return None;
    }
    let Some(dir) = claude_dir() else {
        eprintln!("No home folder is set, so Claude Code's session files cannot be found.");
        return Some(1);
    };
    let mut watcher = Watcher::default();
    watcher.poll(&dir, &feed, true);
    show(&feed, if once { 12 } else { 3 });
    if once {
        if feed.lock().is_ok_and(|f| f.sessions.is_empty()) {
            println!("No Claude Code session has been active in the last hour.");
        }
        return Some(0);
    }
    for tick in 1u64.. {
        std::thread::sleep(std::time::Duration::from_millis(1000));
        watcher.poll(&dir, &feed, tick % 5 == 0);
        show(&feed, 0);
    }
    Some(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn times_are_read_to_the_millisecond() {
        assert_eq!(parse_time("1970-01-01T00:00:00.000Z"), Some(0));
        assert_eq!(parse_time("2000-03-01T00:00:00Z"), Some(951_868_800_000));
        assert_eq!(parse_time("2001-09-09T01:46:40.250Z"), Some(1_000_000_000_250));
        assert_eq!(parse_time("yesterday"), None);
    }

    #[test]
    fn an_assistant_line_gives_words_and_tool_calls() {
        let line = r#"{"type":"assistant","sessionId":"s1","cwd":"Z:\\work\\app","timestamp":"2001-09-09T01:46:40.250Z","message":{"role":"assistant","content":[{"type":"thinking","thinking":""},{"type":"text","text":"Looking."},{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"cargo test","description":"Run tests"}},{"type":"tool_use","id":"t2","name":"Odd","input":{"n":3,"where":"here"}}]}}"#;
        let parsed = parse_claude(line, 5).unwrap();
        assert_eq!(parsed.session.as_deref(), Some("s1"));
        assert_eq!(parsed.cwd.as_deref(), Some("Z:\\work\\app"));
        let whats: Vec<&What> = parsed.steps.iter().map(|s| &s.what).collect();
        assert_eq!(whats[0], &What::Thinking);
        assert_eq!(whats[1], &What::Said("Looking.".into()));
        assert_eq!(whats[2], &What::Tool { id: "t1".into(), name: "Bash".into(), detail: "cargo test".into() });
        // With no well-known field, the first text in the call is shown.
        assert_eq!(whats[3], &What::Tool { id: "t2".into(), name: "Odd".into(), detail: "here".into() });
        assert_eq!(parsed.steps[0].at_ms, 1_000_000_000_250);
    }

    #[test]
    fn results_are_matched_to_their_calls_and_counted() {
        let mut feed = Feed::default();
        let lines = [
            r#"{"type":"user","sessionId":"s1","message":{"role":"user","content":"fix the build"}}"#,
            r#"{"type":"user","sessionId":"s1","isMeta":true,"message":{"role":"user","content":"a note to itself"}}"#,
            r#"{"type":"assistant","sessionId":"s1","message":{"content":[{"type":"tool_use","id":"t1","name":"Read","input":{"file_path":"a.rs"}}]}}"#,
            r#"{"type":"assistant","sessionId":"s1","message":{"content":[{"type":"tool_use","id":"t2","name":"Bash","input":{"command":"ls"}}]}}"#,
            r#"{"type":"user","sessionId":"s1","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":[{"type":"text","text":"fn main() {}"}]}]}}"#,
            r#"{"type":"user","sessionId":"s1","message":{"content":[{"type":"tool_result","tool_use_id":"t2","is_error":true,"content":"no such folder"}]}}"#,
            r#"{"type":"custom-title","sessionId":"s1","customTitle":"Fix the build"}"#,
            r#"{"type":"file-history-snapshot","messageId":"m"}"#,
            "not json",
        ];
        for line in lines {
            if let Some(parsed) = parse_claude(line, 7) {
                feed.add("s1", false, parsed);
            }
        }
        let s = &feed.sessions[0];
        assert_eq!(s.name(), "Fix the build");
        let whats: Vec<&What> = s.steps.iter().map(|s| &s.what).collect();
        assert_eq!(whats.len(), 5);
        assert_eq!(whats[0], &What::Asked("fix the build".into()));
        assert!(matches!(whats[3], What::ToolDone { name, ok: true, size: 12, preview } if name == "Read" && preview == "fn main() {}"));
        assert!(matches!(whats[4], What::ToolDone { name, ok: false, size: 14, .. } if name == "Bash"));
        assert_eq!(s.tool_counts["Read"], 1);
        assert!(s.waiting_on().is_empty());
    }

    #[test]
    fn the_event_stream_gives_a_start_and_an_end() {
        let init = r#"{"type":"system","subtype":"init","cwd":"C:\\x","session_id":"abc","model":"claude-opus-5-5"}"#;
        let done = r#"{"type":"result","subtype":"success","is_error":true,"total_cost_usd":0.25,"result":"Failed to authenticate","session_id":"abc"}"#;
        let start = parse_claude(init, 1).unwrap();
        assert_eq!(start.steps[0].what, What::Start { model: "claude-opus-5-5".into() });
        assert_eq!(start.session.as_deref(), Some("abc"));
        let end = parse_claude(done, 1).unwrap();
        assert_eq!(
            end.steps[0].what,
            What::Finished { ok: false, cost: Some(0.25), text: "Failed to authenticate".into() }
        );
    }

    #[test]
    fn a_session_file_is_followed_as_it_grows() {
        let dir = std::env::temp_dir().join(format!("maestro-live-{}", std::process::id()));
        let project = dir.join("Z--work-app");
        std::fs::create_dir_all(&project).unwrap();
        let path = project.join("sess-1.jsonl");
        let said = |text: &str| {
            let line = json!({"type": "assistant", "sessionId": "sess-1", "cwd": "Z:\\work\\app",
                "message": {"content": [{"type": "text", "text": text}]}});
            format!("{line}\n")
        };
        std::fs::write(&path, said("one")).unwrap();

        let feed = Mutex::new(Feed::default());
        let mut watcher = Watcher::default();
        assert!(watcher.poll(&dir, &feed, true));
        // Half a line arrives, then the rest of it with another line.
        let two = said("two");
        let (head, rest) = two.split_at(30);
        let mut file = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        file.write_all(head.as_bytes()).unwrap();
        assert!(!watcher.poll(&dir, &feed, false));
        file.write_all(format!("{rest}{}", said("three")).as_bytes()).unwrap();
        assert!(watcher.poll(&dir, &feed, false));

        let feed = feed.lock().unwrap();
        let session = &feed.sessions[0];
        assert_eq!(session.id, "sess-1");
        assert_eq!(session.name(), "app");
        let words: Vec<&What> = session.steps.iter().map(|s| &s.what).collect();
        assert_eq!(
            words,
            [&What::Said("one".into()), &What::Said("two".into()), &What::Said("three".into())]
        );
        drop(feed);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_run_asks_for_every_step_and_edits_only_when_allowed() {
        let read_only = claude_args(Leave::ReadOnly);
        assert!(read_only.windows(2).any(|w| w == ["--output-format", "stream-json"]));
        assert!(read_only.windows(2).any(|w| w == ["--input-format", "stream-json"]));
        assert!(!read_only.contains(&"--permission-mode"));
        assert!(!read_only.contains(&"--permission-prompt-tool"));
        // The login must be the user's own, which this flag would bypass.
        assert!(!read_only.contains(&"--bare"));
        assert!(claude_args(Leave::EditFiles)
            .windows(2)
            .any(|w| w == ["--permission-mode", "acceptEdits"]));
        let line: Value = serde_json::from_str(&user_line("say \"hi\"\nplease")).unwrap();
        assert_eq!(line["message"]["content"][0]["text"], "say \"hi\"\nplease");
    }

    #[test]
    fn targets_are_named_by_what_matters() {
        assert_eq!(target("Read", "Z:\\Work\\app\\src\\main.rs"), "main.rs");
        assert_eq!(target("Glob", "src/**/*.rs"), "*.rs");
        assert_eq!(target("WebFetch", "https://docs.rs/egui/latest/egui/?x=1"), "docs.rs");
        assert_eq!(target("Bash", "cargo test --release 2>&1 | tail"), "cargo test");
        assert_eq!(target("Bash", "cd /z/work/app && git status --short"), "git status");
        assert_eq!(target("Bash", "FOO=1 make"), "make");
        assert_eq!(target("Grep", "fn main"), "fn main");
        assert_eq!(target("Agent", ""), "");
        assert_eq!(short_tool("mcp__ccd_turn__update_plan"), "update_plan");
        assert_eq!(short_tool("Read"), "Read");
    }

    #[test]
    fn a_session_becomes_tools_and_what_they_touched() {
        let mut feed = Feed::default();
        let tool = |at: u64, id: &str, name: &str, detail: &str| Step {
            at_ms: at,
            what: What::Tool { id: id.into(), name: name.into(), detail: detail.into() },
        };
        let done = |at: u64, id: &str, ok: bool| Step {
            at_ms: at,
            what: What::ToolDone { name: id.into(), ok, size: 1, preview: String::new() },
        };
        let steps = vec![
            tool(10, "a", "Read", "src/main.rs"),
            done(11, "a", true),
            tool(20, "b", "Bash", "cargo build"),
            done(21, "b", false),
            tool(30, "c", "Read", "src/lib.rs"),
            done(31, "c", true),
            tool(40, "d", "Read", "other/main.rs"),
            tool(50, "e", "Bash", "cargo test"),
        ];
        feed.add("s", false, Parsed { steps, ..Default::default() });
        let map = map(&feed.sessions[0]);
        let tools: Vec<&str> = map.iter().map(|b| b.tool.as_str()).collect();
        assert_eq!(tools, ["Read", "Bash"]);
        let read = &map[0];
        assert_eq!((read.count, read.last_ms, read.failed_ms, read.waiting), (3, 40, None, true));
        // Two files share a name, so they share a leaf; the latest use comes first.
        let leaves: Vec<(&str, u32)> = read.leaves.iter().map(|l| (l.label.as_str(), l.count)).collect();
        assert_eq!(leaves, [("main.rs", 2), ("lib.rs", 1)]);
        assert_eq!(read.leaves[0].full, "other/main.rs");
        let bash = &map[1];
        assert_eq!((bash.count, bash.failed_ms, bash.waiting), (2, Some(21), true));
    }

    #[test]
    fn a_permission_question_is_read_and_answered_in_the_tools_own_shape() {
        let line = r#"{"type":"control_request","request_id":"req-7","request":{"subtype":"can_use_tool","tool_name":"Bash","display_name":"Bash","input":{"command":"rm -rf build","description":"Clear the build folder"},"description":"Remove the build folder","permission_suggestions":[],"decision_reason":"No rule allows this command","tool_use_id":"toolu_1"}}"#;
        let Some(Control::Ask(ask)) = parse_control(line, 9) else {
            panic!("not read as a question");
        };
        assert_eq!((ask.request_id.as_str(), ask.tool.as_str()), ("req-7", "Bash"));
        assert_eq!(ask.detail, "rm -rf build");
        assert_eq!(ask.description, "Remove the build folder");
        assert_eq!(ask.reason, "No rule allows this command");

        // Allowing hands the call back exactly as it was asked.
        let yes: Value = serde_json::from_str(&answer_line(&ask, true, "")).unwrap();
        assert_eq!(yes["type"], "control_response");
        assert_eq!(yes["response"]["subtype"], "success");
        assert_eq!(yes["response"]["request_id"], "req-7");
        assert_eq!(yes["response"]["response"]["behavior"], "allow");
        assert_eq!(yes["response"]["response"]["updatedInput"], ask.input);
        assert_eq!(yes["response"]["response"]["toolUseID"], "toolu_1");

        // Refusing always says something, so the agent knows it was a person.
        let no: Value = serde_json::from_str(&answer_line(&ask, false, "  ")).unwrap();
        assert_eq!(no["response"]["response"]["behavior"], "deny");
        assert!(no["response"]["response"]["message"].as_str().unwrap().contains("refused"));
        let told: Value = serde_json::from_str(&answer_line(&ask, false, "use cargo clean")).unwrap();
        assert_eq!(told["response"]["response"]["message"], "use cargo clean");
        assert!(told["response"]["response"].get("updatedInput").is_none());
    }

    #[test]
    fn other_control_lines_are_not_questions() {
        let cancel = r#"{"type":"control_cancel_request","request_id":"req-7"}"#;
        assert_eq!(parse_control(cancel, 0), Some(Control::Withdrawn("req-7".into())));
        let reply = r#"{"type":"control_response","response":{"subtype":"success","request_id":"maestro-1"}}"#;
        assert_eq!(parse_control(reply, 0), None);
        let other = r#"{"type":"control_request","request_id":"x","request":{"subtype":"hook_callback"}}"#;
        assert_eq!(parse_control(other, 0), None);
        assert_eq!(parse_control(r#"{"type":"assistant"}"#, 0), None);
    }

    #[test]
    fn questions_wait_on_their_session_until_answered_or_withdrawn() {
        let mut feed = Feed::default();
        let ask = |id: &str| Ask {
            request_id: id.into(),
            tool: "Bash".into(),
            detail: "ls".into(),
            description: String::new(),
            reason: String::new(),
            input: json!({"command": "ls"}),
            tool_use_id: None,
            at_ms: 5,
        };
        feed.ask("run", ask("a"));
        feed.ask("run", ask("b"));
        assert_eq!(feed.sessions[0].asks.len(), 2);
        assert!(matches!(feed.sessions[0].steps.back().unwrap().what, What::NeedsYou { .. }));
        assert!(feed.settle("run", "a", Some(true)).is_some());
        assert!(matches!(
            &feed.sessions[0].steps.back().unwrap().what,
            What::Decided { allowed: true, .. }
        ));
        // Withdrawn by the tool: gone, and nothing is claimed to have been decided.
        let steps = feed.sessions[0].steps.len();
        assert!(feed.settle("run", "b", None).is_some());
        assert_eq!(feed.sessions[0].steps.len(), steps);
        assert!(feed.sessions[0].asks.is_empty());
        assert!(feed.settle("run", "b", Some(true)).is_none());
    }

    #[test]
    fn asking_first_is_what_makes_the_tool_send_its_questions() {
        let ask = claude_args(Leave::AskFirst);
        assert!(ask.windows(2).any(|w| w == ["--permission-prompt-tool", "stdio"]));
        assert!(!ask.contains(&"--permission-mode"));
        let edits = claude_args(Leave::EditFiles);
        assert!(edits.windows(2).any(|w| w == ["--permission-prompt-tool", "stdio"]));
        // Read-only has nobody to ask, so the tool refuses by itself.
        assert!(claude_args(Leave::ReadOnly).windows(2).any(|w| w == ["--permission-prompts", "none"]));
    }
}

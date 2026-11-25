//! Phase 2: the actions that change the machine, and the rules that guard them.
//!
//! Every action here goes through `check` first, is recorded so it can be
//! undone, and is written to disk so a crash never leaves programs frozen
//! with no record of them.

use crate::classify::Class;
use crate::telemetry::{Proc, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Whether Maestro will act on a process of this class. `Err` carries the
/// reason in plain words.
pub fn check(class: Class, pid: u32) -> Result<(), &'static str> {
    if SYSTEM_PIDS.contains(&pid) {
        return Err("This is the system itself.");
    }
    if pid == std::process::id() {
        return Err("This is Maestro.");
    }
    match class {
        Class::Critical => Err(concat!(os!(), " stops working without this, so it is locked.")),
        Class::System => Err(concat!(
            "Part of ",
            os!(),
            ". Actions on system components are locked in this build."
        )),
        Class::Service | Class::App | Class::Suspect => Ok(()),
    }
}

/// Process numbers that are the system itself: the idle counter and kernel
/// on Windows, and the kernel and first process elsewhere.
#[cfg(windows)]
pub const SYSTEM_PIDS: &[u32] = &[0, 4];
#[cfg(not(windows))]
pub const SYSTEM_PIDS: &[u32] = &[0, 1, 2];

/// A process Maestro has frozen. The start time guards against the system
/// handing the same number to a different process later.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Paused {
    pub pid: u32,
    pub start_secs: u64,
    pub name: String,
}

impl Paused {
    pub fn of(p: &Proc) -> Self {
        Self {
            pid: p.pid,
            start_secs: p.start_secs,
            name: p.name.clone(),
        }
    }
}

/// What Maestro has changed and not yet undone, kept on disk.
#[derive(Serialize, Deserialize, Default)]
pub struct Journal {
    /// Processes currently frozen by Maestro.
    #[serde(default)]
    pub paused: Vec<Paused>,
    /// Programs with a Maestro firewall rule, by program name.
    #[serde(default)]
    pub blocked: Vec<String>,
    /// Network adapters Maestro has switched off.
    #[serde(default)]
    pub adapters_off: Vec<String>,
    /// Programs never paused when clearing the machine.
    #[serde(default)]
    pub keep: Vec<String>,
    /// Daily spending limit per agent, in US dollars, by agent id.
    #[serde(default)]
    pub budgets: HashMap<String, f64>,
    #[serde(default)]
    pub quests: Vec<Quest>,
}

/// A named piece of work, and what its agent reported spending on it.
#[derive(Serialize, Deserialize, Clone)]
pub struct Quest {
    pub title: String,
    /// Agent id, as in `agents::KINDS`.
    pub agent: String,
    /// The agent's lifetime totals when the quest started.
    pub base_cost: f64,
    pub base_tokens: f64,
    /// Spent since then. Frozen when the quest is marked done.
    pub cost: f64,
    pub tokens: f64,
    pub done: bool,
}

impl Journal {
    pub fn load() -> Self {
        crate::paths::load("state.json")
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(text) = serde_json::to_string_pretty(self) {
            crate::paths::save("state.json", &text);
        }
    }

    /// Forget frozen processes that no longer exist.
    pub fn drop_gone(&mut self, snap: &Snapshot) {
        let alive: HashSet<(u32, u64)> = snap
            .processes
            .iter()
            .map(|p| (p.pid, p.start_secs))
            .collect();
        self.paused
            .retain(|p| alive.contains(&(p.pid, p.start_secs)));
    }
}

/// What clearing the machine for a task would do, by program name.
#[derive(Debug, Default, PartialEq)]
pub struct FocusPlan {
    /// Left running: the task, what started it, what it started, anything on
    /// the keep list, and Maestro's own chain.
    pub kept: Vec<String>,
    /// Ordinary apps that would be paused.
    pub apps: Vec<String>,
    /// Background services that could also be paused. Not paused by default.
    pub services: Vec<String>,
}

/// Work out what can be paused to clear the machine for `task`.
/// Windows itself and anything locked is never listed at all.
pub fn focus_plan(
    snap: &Snapshot,
    classes: &HashMap<u32, Class>,
    task: &str,
    keep: &[String],
) -> FocusPlan {
    let by_pid: HashMap<u32, &Proc> = snap.processes.iter().map(|p| (p.pid, p)).collect();
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in &snap.processes {
        if let Some(parent) = real_parent(p, &by_pid) {
            children.entry(parent.pid).or_default().push(p.pid);
        }
    }

    // Everything in the task's family tree stays, and so does Maestro's.
    let own = std::process::id();
    let mut family: HashSet<u32> = HashSet::new();
    let mut seeds: Vec<u32> = snap
        .processes
        .iter()
        .filter(|p| p.name == task)
        .map(|p| p.pid)
        .collect();
    let mut below = seeds.clone();
    while let Some(pid) = below.pop() {
        if family.insert(pid) {
            below.extend(children.get(&pid).into_iter().flatten().copied());
        }
    }
    seeds.push(own);
    for seed in seeds {
        let mut current = by_pid.get(&seed).copied();
        for _ in 0..64 {
            let Some(p) = current else { break };
            family.insert(p.pid);
            current = real_parent(p, &by_pid);
        }
    }

    // Decide per program: one kept or locked process keeps the whole program.
    #[derive(Default)]
    struct Program {
        class: Option<Class>,
        kept: bool,
        locked: bool,
    }
    let mut programs: HashMap<&str, Program> = HashMap::new();
    for p in &snap.processes {
        let class = classes.get(&p.pid).copied().unwrap_or(Class::App);
        let program = programs.entry(p.name.as_str()).or_default();
        program.class = Some(program.class.map_or(class, |c| c.max(class)));
        program.kept |= family.contains(&p.pid) || keep.iter().any(|k| k == &p.name);
        program.locked |= check(class, p.pid).is_err();
    }

    let mut plan = FocusPlan::default();
    for (name, program) in programs {
        if program.kept {
            plan.kept.push(name.to_string());
        } else if !program.locked {
            match program.class {
                Some(Class::Service) => plan.services.push(name.to_string()),
                _ => plan.apps.push(name.to_string()),
            }
        }
    }
    plan.kept.sort_unstable();
    plan.apps.sort_unstable();
    plan.services.sort_unstable();
    plan
}

/// The process that started `p`, ignoring reused process numbers.
fn real_parent<'a>(p: &Proc, by_pid: &HashMap<u32, &'a Proc>) -> Option<&'a Proc> {
    p.parent_pid
        .and_then(|pid| by_pid.get(&pid).copied())
        .filter(|parent| parent.pid != p.pid && parent.start_secs <= p.start_secs)
}

// ---- process actions ---------------------------------------------------

#[cfg(windows)]
mod sys {
    use std::ffi::c_void;

    #[link(name = "ntdll")]
    extern "system" {
        fn NtSuspendProcess(process: *mut c_void) -> i32;
        fn NtResumeProcess(process: *mut c_void) -> i32;
    }

    #[link(name = "kernel32")]
    extern "system" {
        fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        fn TerminateProcess(process: *mut c_void, exit_code: u32) -> i32;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetLastError() -> u32;
    }

    const PROCESS_TERMINATE: u32 = 0x0001;
    const PROCESS_SUSPEND_RESUME: u32 = 0x0800;
    const ERROR_ACCESS_DENIED: u32 = 5;
    const ERROR_INVALID_PARAMETER: u32 = 87;

    fn with_process(
        pid: u32,
        access: u32,
        act: impl FnOnce(*mut c_void) -> bool,
    ) -> Result<(), String> {
        // SAFETY: plain system calls; the handle is checked and always closed.
        unsafe {
            let handle = OpenProcess(access, 0, pid);
            if handle.is_null() {
                return Err(match GetLastError() {
                    ERROR_ACCESS_DENIED => {
                        "Windows refused. This one needs Maestro to run as administrator."
                            .to_string()
                    }
                    ERROR_INVALID_PARAMETER => "It is no longer running.".to_string(),
                    code => format!("Windows error {code}."),
                });
            }
            let ok = act(handle);
            CloseHandle(handle);
            if ok {
                Ok(())
            } else {
                Err("Windows did not carry out the action.".to_string())
            }
        }
    }

    pub fn suspend(pid: u32) -> Result<(), String> {
        with_process(pid, PROCESS_SUSPEND_RESUME, |h| unsafe { NtSuspendProcess(h) >= 0 })
    }

    pub fn resume(pid: u32) -> Result<(), String> {
        with_process(pid, PROCESS_SUSPEND_RESUME, |h| unsafe { NtResumeProcess(h) >= 0 })
    }

    pub fn end(pid: u32) -> Result<(), String> {
        with_process(pid, PROCESS_TERMINATE, |h| unsafe { TerminateProcess(h, 1) != 0 })
    }
}

/// Linux and macOS: the standard stop, continue and kill signals.
#[cfg(unix)]
mod sys {
    extern "C" {
        fn kill(pid: i32, signal: i32) -> i32;
    }

    const SIGKILL: i32 = 9;
    // The numbers on x86 and ARM Linux, and on macOS and the BSDs.
    #[cfg(target_os = "linux")]
    const SIGSTOP: i32 = 19;
    #[cfg(target_os = "linux")]
    const SIGCONT: i32 = 18;
    #[cfg(not(target_os = "linux"))]
    const SIGSTOP: i32 = 17;
    #[cfg(not(target_os = "linux"))]
    const SIGCONT: i32 = 19;

    const EPERM: i32 = 1;
    const ESRCH: i32 = 3;

    fn send(pid: u32, signal: i32) -> Result<(), String> {
        // Numbers of 0 and below address whole groups of processes, and 1 is
        // the first process. None of them is ever signalled.
        let pid = i32::try_from(pid).ok().filter(|pid| *pid > 1);
        let Some(pid) = pid else {
            return Err("This is the system itself.".to_string());
        };
        // SAFETY: a plain system call with two integers.
        if unsafe { kill(pid, signal) } == 0 {
            return Ok(());
        }
        Err(match std::io::Error::last_os_error().raw_os_error() {
            Some(EPERM) => {
                "The system refused. It belongs to another user, so this one needs Maestro \
                 to run as root."
                    .to_string()
            }
            Some(ESRCH) => "It is no longer running.".to_string(),
            code => format!("System error {}.", code.unwrap_or(0)),
        })
    }

    pub fn suspend(pid: u32) -> Result<(), String> {
        send(pid, SIGSTOP)
    }

    pub fn resume(pid: u32) -> Result<(), String> {
        send(pid, SIGCONT)
    }

    pub fn end(pid: u32) -> Result<(), String> {
        send(pid, SIGKILL)
    }
}

#[cfg(not(any(windows, unix)))]
mod sys {
    const TODO: &str = "Process actions are not built for this system yet.";
    pub fn suspend(_pid: u32) -> Result<(), String> {
        Err(TODO.to_string())
    }
    pub fn resume(_pid: u32) -> Result<(), String> {
        Err(TODO.to_string())
    }
    pub fn end(_pid: u32) -> Result<(), String> {
        Err(TODO.to_string())
    }
}

/// Freeze a process. Nothing is lost; `resume` continues it.
pub fn suspend(pid: u32) -> Result<(), String> {
    sys::suspend(pid)
}

pub fn resume(pid: u32) -> Result<(), String> {
    sys::resume(pid)
}

/// Close a process at once. Unsaved work in it is lost.
pub fn end(pid: u32) -> Result<(), String> {
    sys::end(pid)
}

// ---- actions that need the system's permission ----------------------------

/// The name of the firewall rule Maestro makes for a program.
#[cfg_attr(not(windows), allow(dead_code))]
pub fn rule_name(program: &str) -> String {
    let safe: String = program
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ' ') {
                c
            } else {
                '_'
            }
        })
        .collect();
    format!("Maestro block {safe}")
}

#[cfg(windows)]
mod net {
    use super::{base64, rule_name};

    /// A PowerShell literal: single quotes, with any quote inside doubled.
    fn ps_quote(text: &str) -> Result<String, String> {
        if text.chars().any(|c| c.is_control()) {
            return Err("The name contains characters that cannot be used safely.".to_string());
        }
        Ok(format!("'{}'", text.replace('\'', "''")))
    }

    pub fn firewall_block_script(program: &str, exe: &str) -> Result<String, String> {
        let name = ps_quote(&rule_name(program))?;
        let exe = ps_quote(exe)?;
        Ok(format!(
            "New-NetFirewallRule -DisplayName {name} -Group 'Maestro' -Direction Outbound -Program {exe} -Action Block | Out-Null\n\
             New-NetFirewallRule -DisplayName {name} -Group 'Maestro' -Direction Inbound -Program {exe} -Action Block | Out-Null"
        ))
    }

    pub fn firewall_unblock_script(program: &str) -> Result<String, String> {
        let name = ps_quote(&rule_name(program))?;
        Ok(format!("Remove-NetFirewallRule -DisplayName {name}"))
    }

    pub fn adapter_script(adapter: &str, enable: bool) -> Result<String, String> {
        let name = ps_quote(adapter)?;
        let verb = if enable { "Enable" } else { "Disable" };
        Ok(format!("{verb}-NetAdapter -Name {name} -Confirm:$false"))
    }

    /// Windows shows its own permission prompt first.
    pub fn run_elevated(script: &str) -> Result<(), String> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        const DECLINED: i32 = 1223;

        let inner =
            format!("$ErrorActionPreference = 'Stop'\ntry {{\n{script}\n}} catch {{ exit 1 }}");
        let utf16: Vec<u8> = inner.encode_utf16().flat_map(u16::to_le_bytes).collect();
        // Base64 holds only letters, digits, + / and =, so it is safe inside quotes.
        let encoded = base64(&utf16);
        let outer = format!(
            "try {{ $p = Start-Process powershell -Verb RunAs -WindowStyle Hidden -Wait -PassThru \
             -ArgumentList '-NoProfile','-NonInteractive','-EncodedCommand','{encoded}'; \
             exit $p.ExitCode }} catch {{ exit {DECLINED} }}"
        );
        let status = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &outer])
            .creation_flags(CREATE_NO_WINDOW)
            .status()
            .map_err(|e| format!("Could not start PowerShell: {e}"))?;
        match status.code() {
            Some(0) => Ok(()),
            Some(DECLINED) => Err("Permission was not given.".to_string()),
            _ => Err("Windows reported an error carrying it out.".to_string()),
        }
    }

    pub const BLOCK_HINT: &str = "Adds Windows Firewall rules so this program cannot send or \
        receive. Windows will ask for permission first.";
}

/// Linux and macOS. Adapters are switched with the system's own tool, run
/// through the system's own permission prompt. Blocking one program is not
/// built on these systems yet; the error says what it takes.
#[cfg(unix)]
mod net {
    /// An adapter name that is safe to place in a command: letters, digits
    /// and `. _ - :` only.
    pub fn adapter_name(name: &str) -> Result<&str, String> {
        let ok = !name.is_empty()
            && name.len() <= 32
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'));
        if ok {
            Ok(name)
        } else {
            Err("The adapter's name contains characters that cannot be used safely.".to_string())
        }
    }

    #[cfg(target_os = "macos")]
    const NO_BLOCK: &str = "Blocking one program on macOS takes a Network Extension content \
        filter, which has to be signed with an Apple developer identity. That part is not \
        built yet.";
    #[cfg(not(target_os = "macos"))]
    const NO_BLOCK: &str = "Blocking one program on Linux takes a control group for the \
        program and an nftables rule that matches it. That part is not built yet.";

    pub fn firewall_block_script(_program: &str, _exe: &str) -> Result<String, String> {
        Err(NO_BLOCK.to_string())
    }

    pub fn firewall_unblock_script(_program: &str) -> Result<String, String> {
        Err(NO_BLOCK.to_string())
    }

    pub fn adapter_script(adapter: &str, enable: bool) -> Result<String, String> {
        let name = adapter_name(adapter)?;
        let state = if enable { "up" } else { "down" };
        Ok(if cfg!(target_os = "macos") {
            format!("/sbin/ifconfig {name} {state}")
        } else {
            format!("ip link set dev {name} {state}")
        })
    }

    /// macOS asks through its own password prompt.
    #[cfg(target_os = "macos")]
    pub fn run_elevated(script: &str) -> Result<(), String> {
        // The script holds only the fixed command and a checked adapter name,
        // so it cannot contain a quote or a backslash.
        if script.contains(['"', '\\']) {
            return Err("The command contains characters that cannot be used safely.".to_string());
        }
        let ask = format!("do shell script \"{script}\" with administrator privileges");
        let output = std::process::Command::new("/usr/bin/osascript")
            .args(["-e", &ask])
            .output()
            .map_err(|e| format!("Could not start osascript: {e}"))?;
        if output.status.success() {
            Ok(())
        } else if String::from_utf8_lossy(&output.stderr).contains("-128") {
            Err("Permission was not given.".to_string())
        } else {
            Err("macOS reported an error carrying it out.".to_string())
        }
    }

    /// Linux asks through polkit, the desktop's own permission prompt.
    #[cfg(not(target_os = "macos"))]
    pub fn run_elevated(script: &str) -> Result<(), String> {
        let status = std::process::Command::new("pkexec")
            .args(["/bin/sh", "-c", script])
            .status()
            .map_err(|_| {
                "This needs pkexec, part of polkit, to ask for permission. It was not found."
                    .to_string()
            })?;
        match status.code() {
            Some(0) => Ok(()),
            // pkexec's own codes for "not authorised" and "dismissed".
            Some(126 | 127) => Err("Permission was not given.".to_string()),
            _ => Err("Linux reported an error carrying it out.".to_string()),
        }
    }

    pub const BLOCK_HINT: &str = "Stops this program from sending or receiving.";
}

#[cfg(not(any(windows, unix)))]
mod net {
    const TODO: &str = "Network actions are not built for this system yet.";
    pub fn firewall_block_script(_program: &str, _exe: &str) -> Result<String, String> {
        Err(TODO.to_string())
    }
    pub fn firewall_unblock_script(_program: &str) -> Result<String, String> {
        Err(TODO.to_string())
    }
    pub fn adapter_script(_adapter: &str, _enable: bool) -> Result<String, String> {
        Err(TODO.to_string())
    }
    pub fn run_elevated(_script: &str) -> Result<(), String> {
        Err(TODO.to_string())
    }
    pub const BLOCK_HINT: &str = "Stops this program from sending or receiving.";
}

/// Script that stops a program from using the network, in both directions.
pub fn firewall_block_script(program: &str, exe: &str) -> Result<String, String> {
    net::firewall_block_script(program, exe)
}

pub fn firewall_unblock_script(program: &str) -> Result<String, String> {
    net::firewall_unblock_script(program)
}

pub fn adapter_script(adapter: &str, enable: bool) -> Result<String, String> {
    net::adapter_script(adapter, enable)
}

/// Run a script with administrator rights. The system shows its own
/// permission prompt first; declining it is reported as an error. Blocks
/// until done.
pub fn run_elevated(script: &str) -> Result<(), String> {
    net::run_elevated(script)
}

/// What the "Block network" button says it will do on this system.
pub const BLOCK_HINT: &str = net::BLOCK_HINT;

#[cfg_attr(not(windows), allow(dead_code))]
fn base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let n = (chunk[0] as u32) << 16
            | (*chunk.get(1).unwrap_or(&0) as u32) << 8
            | *chunk.get(2).unwrap_or(&0) as u32;
        out.push(TABLE[(n >> 18) as usize & 63] as char);
        out.push(TABLE[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            TABLE[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{Cpu, Memory, SystemInfo};

    fn proc(pid: u32, parent: u32, name: &str, start: u64) -> Proc {
        Proc {
            pid,
            parent_pid: Some(parent),
            name: name.to_string(),
            exe: None,
            cpu_percent: 0.0,
            memory_bytes: 0,
            start_secs: start,
            disk_read_bytes_per_sec: 0.0,
            disk_write_bytes_per_sec: 0.0,
        }
    }

    fn snapshot(processes: Vec<Proc>) -> Snapshot {
        Snapshot {
            taken_ms: 0,
            interval_ms: 1000,
            system: SystemInfo {
                os_name: None,
                os_version: None,
                uptime_secs: 0,
            },
            cpu: Cpu {
                brand: String::new(),
                total_percent: 0.0,
                core_percent: Vec::new(),
            },
            memory: Memory {
                total_bytes: 0,
                used_bytes: 0,
                swap_total_bytes: 0,
                swap_used_bytes: 0,
            },
            disks: Vec::new(),
            networks: Vec::new(),
            processes,
            connections: Vec::new(),
        }
    }

    #[test]
    fn base64_matches_known_values() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn locked_classes_are_refused() {
        assert!(check(Class::Critical, 900).is_err());
        assert!(check(Class::System, 900).is_err());
        assert!(check(Class::App, SYSTEM_PIDS[1]).is_err());
        assert!(check(Class::App, std::process::id()).is_err());
        assert!(check(Class::App, 900).is_ok());
        assert!(check(Class::Service, 900).is_ok());
        assert!(check(Class::Suspect, 900).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn adapter_names_cannot_carry_a_command() {
        assert!(adapter_script("eth0; reboot", false).is_err());
        assert!(adapter_script("eth0\nreboot", false).is_err());
        assert!(adapter_script("$(reboot)", false).is_err());
        assert!(adapter_script("", false).is_err());
        let script = adapter_script("wlp3s0", false).unwrap();
        assert!(script.ends_with("wlp3s0 down"), "{script}");
        // Blocking one program says what it takes, and does nothing.
        assert!(firewall_block_script("a", "/usr/bin/a").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn scripts_cannot_be_broken_out_of() {
        // A quote in a name is doubled, so it stays inside the literal.
        let script = firewall_block_script("a'b.exe", "C:\\x'; Remove-Item C:\\ -Recurse; '.exe")
            .expect("quotes are allowed once doubled");
        assert!(script.contains("-Program 'C:\\x''; Remove-Item C:\\ -Recurse; ''.exe'"));
        // The rule name drops anything unusual altogether.
        assert_eq!(rule_name("a'b;$(x).exe"), "Maestro block a_b___x_.exe");
        // Line breaks would start a new command, so they are refused.
        assert!(adapter_script("Ethernet\nStop-Computer", false).is_err());
        assert_eq!(
            adapter_script("Ethernet 2", true).unwrap(),
            "Enable-NetAdapter -Name 'Ethernet 2' -Confirm:$false"
        );
    }

    #[test]
    fn focus_keeps_the_task_family_and_the_keep_list() {
        let snap = snapshot(vec![
            proc(100, 1, "shell.exe", 10),
            proc(200, 100, "task.exe", 20),
            proc(300, 200, "helper.exe", 30),
            proc(400, 1, "music.exe", 40),
            proc(500, 1, "chat.exe", 50),
            proc(600, 1, "sync.exe", 60),
            proc(700, 1, "winthing.exe", 70),
        ]);
        let classes: HashMap<u32, Class> = [
            (100, Class::App),
            (200, Class::App),
            (300, Class::App),
            (400, Class::App),
            (500, Class::App),
            (600, Class::Service),
            (700, Class::System),
        ]
        .into();
        let plan = focus_plan(&snap, &classes, "task.exe", &["chat.exe".to_string()]);
        // The task, what started it, what it started, and the kept program stay.
        assert_eq!(plan.kept, ["chat.exe", "helper.exe", "shell.exe", "task.exe"]);
        assert_eq!(plan.apps, ["music.exe"]);
        assert_eq!(plan.services, ["sync.exe"]);
        // Windows components are never offered.
        assert!(!plan.apps.iter().chain(&plan.services).any(|n| n == "winthing.exe"));
    }

    #[test]
    fn a_reused_process_number_is_not_a_parent() {
        // "old.exe" has the number task.exe names as its parent, but started
        // later, so it is unrelated and must not be kept as family.
        let snap = snapshot(vec![
            proc(200, 100, "task.exe", 20),
            proc(100, 1, "old.exe", 99),
        ]);
        let classes: HashMap<u32, Class> = [(200, Class::App), (100, Class::App)].into();
        let plan = focus_plan(&snap, &classes, "task.exe", &[]);
        assert_eq!(plan.kept, ["task.exe"]);
        assert_eq!(plan.apps, ["old.exe"]);
    }
}

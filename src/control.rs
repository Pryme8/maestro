//! Phase 2: the actions that change the machine, and the rules that guard them.
//!
//! Every action here goes through `check` first, is recorded so it can be
//! undone, and is written to disk so a crash never leaves programs frozen
//! with no record of them.

use crate::classify::Class;
use crate::telemetry::{Proc, Snapshot};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

/// Whether Maestro will act on a process of this class. `Err` carries the
/// reason in plain words.
pub fn check(class: Class, pid: u32) -> Result<(), &'static str> {
    if pid == 0 || pid == 4 {
        return Err("This is the system itself.");
    }
    if pid == std::process::id() {
        return Err("This is Maestro.");
    }
    match class {
        Class::Critical => Err("Windows stops working without this, so it is locked."),
        Class::Windows => {
            Err("Part of Windows. Actions on Windows components are locked in this build.")
        }
        Class::Service | Class::App | Class::Suspect => Ok(()),
    }
}

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
}

impl Journal {
    fn path() -> PathBuf {
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_STATE_HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir);
        base.join("Maestro").join("state.json")
    }

    pub fn load() -> Self {
        std::fs::read_to_string(Self::path())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = Self::path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(text) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, text);
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

#[cfg(not(windows))]
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

// ---- actions that need Windows' permission -------------------------------

/// The name of the firewall rule Maestro makes for a program.
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

/// A PowerShell literal: single quotes, with any quote inside doubled.
fn ps_quote(text: &str) -> Result<String, String> {
    if text.chars().any(|c| c.is_control()) {
        return Err("The name contains characters that cannot be used safely.".to_string());
    }
    Ok(format!("'{}'", text.replace('\'', "''")))
}

/// Script that stops a program from using the network, in both directions.
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

/// Run a script with administrator rights. Windows shows its own permission
/// prompt first; declining it is reported as an error. Blocks until done.
#[cfg(windows)]
pub fn run_elevated(script: &str) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const DECLINED: i32 = 1223;

    let inner = format!("$ErrorActionPreference = 'Stop'\ntry {{\n{script}\n}} catch {{ exit 1 }}");
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

#[cfg(not(windows))]
pub fn run_elevated(_script: &str) -> Result<(), String> {
    Err("Network actions are not built for this system yet.".to_string())
}

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
        assert!(check(Class::Windows, 900).is_err());
        assert!(check(Class::App, 4).is_err());
        assert!(check(Class::App, std::process::id()).is_err());
        assert!(check(Class::App, 900).is_ok());
        assert!(check(Class::Service, 900).is_ok());
        assert!(check(Class::Suspect, 900).is_ok());
    }

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
            (700, Class::Windows),
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

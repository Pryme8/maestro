//! Spotting programs that have stopped doing anything: windows that no
//! longer answer the system, and processes that have sat idle for a long time.
//!
//! Read-only. Both are judged from the outside, so they are hints: a program
//! waiting quietly for you looks the same as one that has been forgotten.

use crate::telemetry::{Proc, Snapshot};
use std::collections::{HashMap, HashSet};

/// How long a program must do nothing before it is called cold.
pub const COLD_AFTER_MS: u64 = 15 * 60 * 1000;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Stale {
    /// One of its windows has stopped answering the system.
    Hung,
    /// No processor or disk activity worth the name for this many minutes.
    Cold { minutes: u32 },
}

impl Stale {
    pub fn short(self) -> String {
        match self {
            Stale::Hung => "not responding".to_string(),
            Stale::Cold { minutes } if minutes >= 120 => format!("cold {}h", minutes / 60),
            Stale::Cold { minutes } => format!("cold {minutes}m"),
        }
    }

    pub fn explain(self) -> String {
        match self {
            Stale::Hung => "Not responding: one of its windows has stopped answering the system."
                .to_string(),
            Stale::Cold { minutes } => format!(
                "Cold: none of its processes has used the processor or the disk for {minutes} \
                 minutes, counted only while Maestro has been running. It may be forgotten, or \
                 it may just be waiting for you."
            ),
        }
    }
}

/// Whether a process did anything worth the name during one sample. Almost
/// every program wakes for an instant now and then, so the bar is above zero.
pub fn busy(p: &Proc) -> bool {
    p.cpu_percent >= 0.05 || p.disk_read_bytes_per_sec + p.disk_write_bytes_per_sec >= 16_384.0
}

struct Seen {
    start_secs: u64,
    /// When it was last busy, or first seen if it never has been.
    active_ms: u64,
}

/// Remembers when each process was last busy. Fed every sample.
#[derive(Default)]
pub struct Tracker {
    seen: HashMap<u32, Seen>,
    hung: HashSet<u32>,
}

impl Tracker {
    pub fn update(&mut self, snap: &Snapshot) {
        self.note(snap);
        self.hung = hung_pids();
    }

    fn note(&mut self, snap: &Snapshot) {
        let now = snap.taken_ms;
        for p in &snap.processes {
            match self.seen.get_mut(&p.pid) {
                // The same number and start time: the process seen before.
                Some(seen) if seen.start_secs == p.start_secs => {
                    if busy(p) {
                        seen.active_ms = now;
                    }
                }
                _ => {
                    self.seen.insert(
                        p.pid,
                        Seen {
                            start_secs: p.start_secs,
                            active_ms: now,
                        },
                    );
                }
            }
        }
        if self.seen.len() > snap.processes.len() {
            let alive: HashSet<u32> = snap.processes.iter().map(|p| p.pid).collect();
            self.seen.retain(|pid, _| alive.contains(pid));
        }
    }

    /// Judge a program from all of its processes. It is hung if any window
    /// is, and cold only if every process has been idle long enough.
    pub fn judge(&self, pids: impl IntoIterator<Item = u32>, now_ms: u64) -> Option<Stale> {
        let mut least_idle = u64::MAX;
        for pid in pids {
            if self.hung.contains(&pid) {
                return Some(Stale::Hung);
            }
            // A process not seen yet counts as busy now.
            let idle = self
                .seen
                .get(&pid)
                .map_or(0, |seen| now_ms.saturating_sub(seen.active_ms));
            least_idle = least_idle.min(idle);
        }
        (least_idle != u64::MAX && least_idle >= COLD_AFTER_MS).then(|| Stale::Cold {
            minutes: (least_idle / 60_000) as u32,
        })
    }
}

/// Processes that own a visible window the system reports as not responding.
#[cfg(windows)]
fn hung_pids() -> HashSet<u32> {
    #[link(name = "user32")]
    extern "system" {
        fn EnumWindows(each: extern "system" fn(isize, isize) -> i32, param: isize) -> i32;
        fn IsHungAppWindow(window: isize) -> i32;
        fn IsWindowVisible(window: isize) -> i32;
        fn GetWindowThreadProcessId(window: isize, pid: *mut u32) -> u32;
    }

    extern "system" fn each(window: isize, param: isize) -> i32 {
        // SAFETY: `param` is the set passed to EnumWindows below, which
        // outlives the call, and the other calls only read window state.
        unsafe {
            if IsWindowVisible(window) != 0 && IsHungAppWindow(window) != 0 {
                let mut pid = 0u32;
                GetWindowThreadProcessId(window, &mut pid);
                if pid != 0 {
                    (*(param as *mut HashSet<u32>)).insert(pid);
                }
            }
        }
        1
    }

    let mut found = HashSet::new();
    // SAFETY: the callback only touches `found`, which lives until the call returns.
    unsafe {
        EnumWindows(each, &mut found as *mut HashSet<u32> as isize);
    }
    found
}

/// Linux and macOS have no single call for this; it is not read there yet.
#[cfg(not(windows))]
fn hung_pids() -> HashSet<u32> {
    HashSet::new()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{Cpu, Memory, SystemInfo};

    fn proc(pid: u32, start: u64, cpu: f32, disk: f64) -> Proc {
        Proc {
            pid,
            parent_pid: None,
            name: format!("p{pid}"),
            exe: None,
            cpu_percent: cpu,
            memory_bytes: 0,
            start_secs: start,
            disk_read_bytes_per_sec: 0.0,
            disk_write_bytes_per_sec: disk,
        }
    }

    fn at(minute: u64, processes: Vec<Proc>) -> Snapshot {
        Snapshot {
            taken_ms: minute * 60_000,
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
            gpu: Default::default(),
        }
    }

    #[test]
    fn a_program_is_cold_only_when_every_process_has_been_idle_long_enough() {
        let mut t = Tracker::default();
        t.note(&at(0, vec![proc(1, 5, 0.0, 0.0), proc(2, 5, 0.0, 0.0), proc(3, 5, 0.0, 0.0)]));
        // Process 2 writes to disk at minute 10; process 3 uses the processor throughout.
        t.note(&at(10, vec![proc(1, 5, 0.0, 0.0), proc(2, 5, 0.0, 50_000.0), proc(3, 5, 2.0, 0.0)]));
        t.note(&at(20, vec![proc(1, 5, 0.01, 100.0), proc(2, 5, 0.0, 0.0), proc(3, 5, 2.0, 0.0)]));
        let now = 20 * 60_000;
        assert_eq!(t.judge([1], now), Some(Stale::Cold { minutes: 20 }));
        // Ten minutes idle is not long enough yet.
        assert_eq!(t.judge([2], now), None);
        assert_eq!(t.judge([1, 2], now), None);
        assert_eq!(t.judge([3], now), None);
        // Nothing is known about a process never seen, so it is not called cold.
        assert_eq!(t.judge([99], now), None);
        assert_eq!(t.judge([], now), None);
        t.note(&at(26, vec![proc(1, 5, 0.0, 0.0), proc(2, 5, 0.0, 0.0), proc(3, 5, 2.0, 0.0)]));
        assert_eq!(t.judge([1, 2], 26 * 60_000), Some(Stale::Cold { minutes: 16 }));
    }

    #[test]
    fn a_reused_process_number_starts_its_count_again_and_the_dead_are_forgotten() {
        let mut t = Tracker::default();
        t.note(&at(0, vec![proc(1, 5, 0.0, 0.0), proc(2, 5, 0.0, 0.0)]));
        // Number 1 now belongs to a process that started later.
        t.note(&at(30, vec![proc(1, 900, 0.0, 0.0)]));
        assert_eq!(t.judge([1], 30 * 60_000), None);
        assert_eq!(t.seen.len(), 1);
    }

    #[test]
    fn a_hung_window_outranks_everything() {
        let mut t = Tracker::default();
        t.note(&at(0, vec![proc(1, 5, 9.0, 0.0)]));
        t.hung.insert(1);
        assert_eq!(t.judge([1, 2], 0), Some(Stale::Hung));
        assert_eq!(Stale::Cold { minutes: 180 }.short(), "cold 3h");
        assert_eq!(Stale::Cold { minutes: 42 }.short(), "cold 42m");
    }
}

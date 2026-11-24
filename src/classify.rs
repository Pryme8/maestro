//! Sorts every process into a plain-language class: what Windows cannot run
//! without, what is part of Windows, what is a background service someone
//! installed, and what is an ordinary app.
//!
//! This is a first pass from names, file locations and who started what. It
//! does not check signatures yet, so it is a guide and not proof.

use crate::telemetry::{Proc, Snapshot};
use std::collections::HashMap;

/// Ordered from least to most important to notice.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Class {
    /// Started by you or by another app.
    App,
    /// Runs in the background under the service manager, but is not part of Windows.
    Service,
    /// Lives in the Windows folder.
    Windows,
    /// Windows stops working without it.
    Critical,
    /// Uses the name of a critical Windows process from somewhere else on disk.
    Suspect,
}

impl Class {
    pub const ALL: [Class; 5] = [
        Class::Suspect,
        Class::Critical,
        Class::Windows,
        Class::Service,
        Class::App,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Class::App => "App",
            Class::Service => "Background service",
            Class::Windows => "Part of Windows",
            Class::Critical => "Critical to Windows",
            Class::Suspect => "Suspicious name",
        }
    }

    pub fn explain(self) -> &'static str {
        match self {
            Class::App => "Started by you or by another app. Not needed by the system.",
            Class::Service => {
                "Runs in the background under the service manager but is not part of Windows. \
                 Usually installed alongside an app or a device."
            }
            Class::Windows => "Its file is in the Windows folder. Part of the operating system.",
            Class::Critical => "Windows stops working without this. It must not be shut down.",
            Class::Suspect => {
                "Has the name of a critical Windows process but runs from outside the Windows \
                 folder. Worth checking."
            }
        }
    }
}

/// Names of processes Windows cannot run without, in lower case.
const CRITICAL: [&str; 16] = [
    "system",
    "system idle process",
    "idle",
    "registry",
    "secure system",
    "memory compression",
    "memcompression",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "svchost.exe",
    "dwm.exe",
];

pub fn classify(snap: &Snapshot) -> HashMap<u32, Class> {
    let windows_dir = std::env::var("SystemRoot")
        .or_else(|_| std::env::var("windir"))
        .unwrap_or_else(|_| "C:\\Windows".to_string())
        .to_lowercase();
    let by_pid: HashMap<u32, &Proc> = snap.processes.iter().map(|p| (p.pid, p)).collect();

    snap.processes
        .iter()
        .map(|p| {
            let name = p.name.to_lowercase();
            let exe = p.exe.as_ref().map(|e| e.to_lowercase());
            let in_windows = exe.as_ref().is_some_and(|e| e.starts_with(&windows_dir));
            let class = if CRITICAL.contains(&name.as_str()) {
                // When the path cannot be read the name is all there is to go on.
                if exe.is_some() && !in_windows {
                    Class::Suspect
                } else {
                    Class::Critical
                }
            } else if in_windows {
                Class::Windows
            } else if under_service_manager(p, &by_pid) {
                Class::Service
            } else {
                Class::App
            };
            (p.pid, class)
        })
        .collect()
}

/// True when the process was started, directly or not, by the service manager.
fn under_service_manager(p: &Proc, by_pid: &HashMap<u32, &Proc>) -> bool {
    let mut current = p;
    for _ in 0..64 {
        let Some(parent) = current.parent_pid.and_then(|pid| by_pid.get(&pid)) else {
            return false;
        };
        // Process numbers are reused, so a parent that started later is unrelated.
        if parent.pid == current.pid || parent.start_secs > current.start_secs {
            return false;
        }
        if parent.name.eq_ignore_ascii_case("services.exe") {
            return true;
        }
        current = parent;
    }
    false
}

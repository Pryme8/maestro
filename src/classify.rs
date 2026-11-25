//! Sorts every process into a plain-language class: what the system cannot
//! run without, what is part of the system, what is a background service
//! someone installed, and what is an ordinary app.
//!
//! This is a first pass from names, file locations and who started what. It
//! does not check signatures yet, so it is a guide and not proof. The rules
//! differ per operating system and live in `rules` below.

use crate::telemetry::{Proc, Snapshot};
use std::collections::HashMap;

/// Ordered from least to most important to notice.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub enum Class {
    /// Started by you or by another app.
    App,
    /// Runs in the background under the service manager, but is not part of the system.
    Service,
    /// Its file lives in a system folder.
    System,
    /// The system stops working without it.
    Critical,
    /// Uses the name of a critical process from somewhere else on disk.
    Suspect,
}

impl Class {
    pub const ALL: [Class; 5] = [
        Class::Suspect,
        Class::Critical,
        Class::System,
        Class::Service,
        Class::App,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Class::App => "App",
            Class::Service => "Background service",
            Class::System => concat!("Part of ", os!()),
            Class::Critical => concat!("Critical to ", os!()),
            Class::Suspect => "Suspicious name",
        }
    }

    pub fn explain(self) -> &'static str {
        match self {
            Class::App => "Started by you or by another app. Not needed by the system.",
            Class::Service => concat!(
                "Runs in the background under the service manager but is not part of ",
                os!(),
                ". Usually installed alongside an app or a device."
            ),
            Class::System => "Its file is in a system folder. Part of the operating system.",
            Class::Critical => concat!(os!(), " stops working without this. It must not be shut down."),
            Class::Suspect => {
                "Has the name of a critical system process but runs from outside the system \
                 folders. Worth checking."
            }
        }
    }
}

pub fn classify(snap: &Snapshot) -> HashMap<u32, Class> {
    let places = rules::Places::read();
    let by_pid: HashMap<u32, &Proc> = snap.processes.iter().map(|p| (p.pid, p)).collect();

    snap.processes
        .iter()
        .map(|p| {
            let name = p.name.to_lowercase();
            let exe = p.exe.as_ref().map(|e| e.to_lowercase());
            let class = if rules::kernel(p) {
                Class::Critical
            } else if rules::CRITICAL.contains(&name.as_str()) {
                // When the path cannot be read the name is all there is to go on.
                match &exe {
                    Some(exe) if !places.trusted(exe) => Class::Suspect,
                    _ => Class::Critical,
                }
            } else if exe.as_ref().is_some_and(|e| places.system(e)) {
                Class::System
            } else if rules::service_like(exe.as_deref()) && under_service_manager(p, &by_pid) {
                Class::Service
            } else {
                Class::App
            };
            (p.pid, class)
        })
        .collect()
}

/// True when the process was started by the service manager, within as many
/// steps as this system's rules allow.
fn under_service_manager(p: &Proc, by_pid: &HashMap<u32, &Proc>) -> bool {
    let mut current = p;
    for _ in 0..rules::SERVICE_DEPTH {
        let Some(parent) = current.parent_pid.and_then(|pid| by_pid.get(&pid)) else {
            return false;
        };
        // Process numbers are reused, so a parent that started later is unrelated.
        if parent.pid == current.pid || parent.start_secs > current.start_secs {
            return false;
        }
        if rules::service_manager(parent) {
            return true;
        }
        current = parent;
    }
    false
}

#[cfg(windows)]
mod rules {
    use crate::telemetry::Proc;

    /// Names of processes Windows cannot run without, in lower case.
    pub const CRITICAL: &[&str] = &[
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

    /// Anything started beneath the service manager is a service.
    pub const SERVICE_DEPTH: usize = 64;

    pub struct Places {
        windows_dir: String,
    }

    impl Places {
        pub fn read() -> Self {
            let windows_dir = std::env::var("SystemRoot")
                .or_else(|_| std::env::var("windir"))
                .unwrap_or_else(|_| "C:\\Windows".to_string())
                .to_lowercase();
            Self { windows_dir }
        }

        /// `exe` is in lower case.
        pub fn system(&self, exe: &str) -> bool {
            exe.starts_with(&self.windows_dir)
        }

        pub fn trusted(&self, exe: &str) -> bool {
            self.system(exe)
        }
    }

    pub fn kernel(_p: &Proc) -> bool {
        false
    }

    pub fn service_manager(p: &Proc) -> bool {
        p.name.eq_ignore_ascii_case("services.exe")
    }

    pub fn service_like(_exe: Option<&str>) -> bool {
        true
    }
}

/// Linux. Written from how systemd-based desktops are laid out and run on
/// Ubuntu under WSL; other distributions are untested.
#[cfg(all(unix, not(target_os = "macos")))]
mod rules {
    use crate::telemetry::Proc;

    /// In lower case. Linux cuts process names to 15 characters.
    pub const CRITICAL: &[&str] = &[
        "systemd",
        "init",
        "kthreadd",
        "systemd-journal",
        "systemd-logind",
        "systemd-udevd",
        "dbus-daemon",
        "dbus-broker",
        "xorg",
        "xwayland",
        "gnome-shell",
        "kwin_wayland",
        "kwin_x11",
        "sddm",
        "gdm",
        "gdm3",
        "lightdm",
    ];

    /// Only what the first process starts directly: desktops and logins also
    /// descend from it, so going deeper would call every app a service.
    pub const SERVICE_DEPTH: usize = 1;

    pub struct Places;

    impl Places {
        pub fn read() -> Self {
            Places
        }

        pub fn system(&self, exe: &str) -> bool {
            ["/usr/lib/systemd/", "/lib/systemd/", "/usr/sbin/", "/sbin/", "/usr/libexec/"]
                .iter()
                .any(|dir| exe.starts_with(dir))
        }

        /// Where a critical process may legitimately run from.
        pub fn trusted(&self, exe: &str) -> bool {
            ["/usr/", "/lib/", "/lib64/", "/bin/", "/sbin/", "/init"]
                .iter()
                .any(|dir| exe.starts_with(dir))
        }
    }

    /// The first process, whatever it is called, and the kernel's threads,
    /// which are all started by process 2.
    pub fn kernel(p: &Proc) -> bool {
        p.pid <= 2 || p.parent_pid == Some(2)
    }

    pub fn service_manager(p: &Proc) -> bool {
        p.pid == 1
    }

    pub fn service_like(_exe: Option<&str>) -> bool {
        true
    }
}

/// macOS. Written from documentation and not yet run on a Mac.
#[cfg(target_os = "macos")]
mod rules {
    use crate::telemetry::Proc;

    /// In lower case.
    pub const CRITICAL: &[&str] = &[
        "kernel_task",
        "launchd",
        "windowserver",
        "loginwindow",
        "opendirectoryd",
        "securityd",
        "configd",
        "logd",
        "notifyd",
        "powerd",
        "diskarbitrationd",
        "coreservicesd",
        "cfprefsd",
        "trustd",
        "syspolicyd",
        "usereventagent",
        "distnoted",
        "coreaudiod",
        "dock",
        "finder",
        "systemuiserver",
    ];

    /// launchd starts apps as well as services, so only its direct children
    /// are looked at, and `service_like` leaves out the apps.
    pub const SERVICE_DEPTH: usize = 1;

    pub struct Places;

    impl Places {
        pub fn read() -> Self {
            Places
        }

        /// `exe` is in lower case.
        pub fn system(&self, exe: &str) -> bool {
            ["/system/", "/usr/libexec/", "/usr/sbin/", "/sbin/", "/usr/bin/", "/bin/"]
                .iter()
                .any(|dir| exe.starts_with(dir))
        }

        pub fn trusted(&self, exe: &str) -> bool {
            self.system(exe)
        }
    }

    pub fn kernel(p: &Proc) -> bool {
        p.pid <= 1
    }

    pub fn service_manager(p: &Proc) -> bool {
        p.pid == 1
    }

    /// An app bundle is an app, however it was started.
    pub fn service_like(exe: Option<&str>) -> bool {
        exe.is_none_or(|e| !e.contains(".app/"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{Cpu, Memory, SystemInfo};

    fn proc(pid: u32, parent: u32, name: &str, exe: Option<&str>) -> Proc {
        Proc {
            pid,
            parent_pid: Some(parent),
            name: name.to_string(),
            exe: exe.map(str::to_string),
            cpu_percent: 0.0,
            memory_bytes: 0,
            start_secs: pid as u64,
            disk_read_bytes_per_sec: 0.0,
            disk_write_bytes_per_sec: 0.0,
        }
    }

    fn classes(processes: Vec<Proc>) -> HashMap<u32, Class> {
        classify(&Snapshot {
            taken_ms: 0,
            interval_ms: 1000,
            system: SystemInfo { os_name: None, os_version: None, uptime_secs: 0 },
            cpu: Cpu { brand: String::new(), total_percent: 0.0, core_percent: Vec::new() },
            memory: Memory { total_bytes: 0, used_bytes: 0, swap_total_bytes: 0, swap_used_bytes: 0 },
            disks: Vec::new(),
            networks: Vec::new(),
            processes,
            connections: Vec::new(),
        })
    }

    #[cfg(windows)]
    #[test]
    fn windows_processes_are_sorted_by_name_place_and_parent() {
        let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".to_string());
        let c = classes(vec![
            proc(600, 500, "services.exe", Some(&format!("{root}\\System32\\services.exe"))),
            proc(700, 600, "updater.exe", Some("C:\\Program Files\\Thing\\updater.exe")),
            proc(710, 700, "helper.exe", Some("C:\\Program Files\\Thing\\helper.exe")),
            proc(800, 500, "notepad.exe", Some(&format!("{root}\\notepad.exe"))),
            proc(900, 500, "game.exe", Some("D:\\Games\\game.exe")),
            proc(950, 500, "lsass.exe", Some("C:\\Users\\x\\lsass.exe")),
            proc(960, 500, "csrss.exe", None),
        ]);
        assert_eq!(c[&600], Class::Critical);
        assert_eq!(c[&700], Class::Service);
        // Anything a service started is counted with it.
        assert_eq!(c[&710], Class::Service);
        assert_eq!(c[&800], Class::System);
        assert_eq!(c[&900], Class::App);
        assert_eq!(c[&950], Class::Suspect);
        assert_eq!(c[&960], Class::Critical);
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn linux_processes_are_sorted_by_name_place_and_parent() {
        let c = classes(vec![
            proc(1, 0, "systemd", Some("/usr/lib/systemd/systemd")),
            proc(2, 0, "kthreadd", None),
            proc(30, 2, "kworker", None),
            proc(400, 1, "cron", Some("/usr/sbin/cron")),
            proc(500, 1, "tailscaled", Some("/opt/tailscale/tailscaled")),
            proc(600, 1, "systemd", Some("/usr/lib/systemd/systemd")),
            proc(700, 600, "firefox", Some("/usr/lib/firefox/firefox")),
            proc(800, 700, "systemd", Some("/tmp/systemd")),
        ]);
        assert_eq!(c[&1], Class::Critical);
        assert_eq!(c[&2], Class::Critical);
        assert_eq!(c[&30], Class::Critical);
        assert_eq!(c[&400], Class::System);
        assert_eq!(c[&500], Class::Service);
        // Started by the user's own systemd, not the first process: an app.
        assert_eq!(c[&700], Class::App);
        assert_eq!(c[&800], Class::Suspect);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn mac_processes_are_sorted_by_name_place_and_parent() {
        let c = classes(vec![
            proc(1, 0, "launchd", Some("/sbin/launchd")),
            proc(300, 1, "mds", Some("/System/Library/Frameworks/x/mds")),
            proc(400, 1, "Safari", Some("/Applications/Safari.app/Contents/MacOS/Safari")),
            proc(500, 1, "backupd-helper", Some("/Library/Thing/backupd-helper")),
            proc(600, 1, "launchd", Some("/Users/x/launchd")),
        ]);
        assert_eq!(c[&1], Class::Critical);
        assert_eq!(c[&300], Class::System);
        assert_eq!(c[&400], Class::App);
        assert_eq!(c[&500], Class::Service);
        assert_eq!(c[&600], Class::Suspect);
    }
}

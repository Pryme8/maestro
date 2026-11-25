//! Read-only system telemetry for phase 1.
//!
//! Everything here observes the machine. Nothing in this module changes it.

use netstat2::{get_sockets_info, AddressFamilyFlags, ProtocolFlags, ProtocolSocketInfo};
use serde::Serialize;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
#[cfg(not(windows))]
use sysinfo::ProcessesToUpdate;
use sysinfo::{CpuRefreshKind, Disks, Networks, System};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Unix time in milliseconds when the sample was taken.
    pub taken_ms: u64,
    /// Time since the previous sample. Rates are averaged over this window.
    pub interval_ms: u64,
    pub system: SystemInfo,
    pub cpu: Cpu,
    pub memory: Memory,
    pub disks: Vec<Disk>,
    pub networks: Vec<Net>,
    pub processes: Vec<Proc>,
    pub connections: Vec<Conn>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub os_name: Option<String>,
    pub os_version: Option<String>,
    pub uptime_secs: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Cpu {
    pub brand: String,
    /// Whole-machine load, 0 to 100.
    pub total_percent: f32,
    /// Load per logical core, 0 to 100.
    pub core_percent: Vec<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Memory {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub swap_total_bytes: u64,
    pub swap_used_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Disk {
    pub name: String,
    pub mount: String,
    pub kind: String,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Net {
    pub name: String,
    pub down_bytes_per_sec: f64,
    pub up_bytes_per_sec: f64,
    pub total_down_bytes: u64,
    pub total_up_bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proc {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    /// Path of the file on disk behind the process, when the OS lets us read it.
    pub exe: Option<String>,
    /// Share of the whole machine's CPU, 0 to 100.
    pub cpu_percent: f32,
    pub memory_bytes: u64,
    pub start_secs: u64,
    pub disk_read_bytes_per_sec: f64,
    pub disk_write_bytes_per_sec: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Conn {
    pub pid: Option<u32>,
    /// "tcp" or "udp".
    pub protocol: &'static str,
    pub local_addr: String,
    pub local_port: u16,
    /// Remote end. UDP sockets have none.
    pub remote_addr: Option<String>,
    pub remote_port: Option<u16>,
    /// TCP state such as "Established" or "Listen". UDP sockets have none.
    pub state: Option<String>,
}

pub struct Sampler {
    #[cfg(windows)]
    core_times: Vec<wincpu::CoreTimes>,
    #[cfg(windows)]
    procs: crate::winproc::Reader,
    /// Processor name and logical core count, read once at start.
    brand: String,
    cores: usize,
    sys: System,
    disks: Disks,
    networks: Networks,
    last: Instant,
}

impl Sampler {
    pub fn new() -> Self {
        let mut sys = System::new();
        // List the cores without asking for their load: on Windows the load
        // query can take seconds per call, and core times are read directly.
        sys.refresh_cpu_list(CpuRefreshKind::nothing());
        let brand = sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_default();
        let cores = sys.cpus().len().max(1);
        #[cfg(not(windows))]
        sys.refresh_cpu_usage();
        sys.refresh_memory();
        #[cfg(not(windows))]
        sys.refresh_processes(ProcessesToUpdate::All, true);
        Self {
            #[cfg(windows)]
            core_times: wincpu::read(),
            #[cfg(windows)]
            procs: {
                let mut reader = crate::winproc::Reader::new();
                reader.read(1.0, 1.0);
                reader
            },
            brand,
            cores,
            sys,
            disks: Disks::new_with_refreshed_list(),
            networks: Networks::new_with_refreshed_list(),
            last: Instant::now(),
        }
    }

    /// Time each part of a sample, in milliseconds. Used by `--profile` to see
    /// what the monitor itself costs.
    pub fn profile(&mut self) -> Vec<(&'static str, f64)> {
        fn timed(name: &'static str, out: &mut Vec<(&'static str, f64)>, f: impl FnOnce()) {
            let start = Instant::now();
            f();
            out.push((name, start.elapsed().as_secs_f64() * 1000.0));
        }
        let mut out = Vec::new();
        #[cfg(not(windows))]
        timed("cpu", &mut out, || self.sys.refresh_cpu_usage());
        #[cfg(windows)]
        timed("cpu", &mut out, || {
            wincpu::read();
        });
        timed("memory", &mut out, || self.sys.refresh_memory());
        #[cfg(not(windows))]
        timed("processes", &mut out, || {
            self.sys.refresh_processes(ProcessesToUpdate::All, true);
        });
        #[cfg(windows)]
        timed("processes", &mut out, || {
            self.procs.read(1.0, 1.0);
        });
        timed("disks", &mut out, || self.disks.refresh(true));
        timed("networks", &mut out, || self.networks.refresh(true));
        timed("sockets", &mut out, || {
            connections();
        });
        timed("whole sample", &mut out, || {
            self.sample();
        });
        out
    }

    pub fn sample(&mut self) -> Snapshot {
        let elapsed = self.last.elapsed();
        self.last = Instant::now();
        // Guard the first call and very fast repeat calls against dividing by ~0.
        let secs = elapsed.as_secs_f64().max(0.05);

        #[cfg(not(windows))]
        self.sys.refresh_cpu_usage();
        self.sys.refresh_memory();
        #[cfg(not(windows))]
        self.sys.refresh_processes(ProcessesToUpdate::All, true);
        self.disks.refresh(true);
        self.networks.refresh(true);

        let cores = self.cores as f32;

        #[cfg(not(windows))]
        let (total_percent, core_percent) = (
            self.sys.global_cpu_usage(),
            self.sys.cpus().iter().map(|c| c.cpu_usage()).collect(),
        );
        // On Windows the per-core counters from sysinfo can read zero, so the
        // core times are read from the system directly.
        #[cfg(windows)]
        let (total_percent, core_percent) = {
            let now = wincpu::read();
            let load = wincpu::load(&self.core_times, &now);
            self.core_times = now;
            load
        };

        let cpu = Cpu {
            brand: self.brand.clone(),
            total_percent,
            core_percent,
        };

        let memory = Memory {
            total_bytes: self.sys.total_memory(),
            used_bytes: self.sys.used_memory(),
            swap_total_bytes: self.sys.total_swap(),
            swap_used_bytes: self.sys.used_swap(),
        };

        let disks = self
            .disks
            .iter()
            .map(|d| Disk {
                name: d.name().to_string_lossy().into_owned(),
                mount: d.mount_point().to_string_lossy().into_owned(),
                kind: format!("{:?}", d.kind()),
                total_bytes: d.total_space(),
                free_bytes: d.available_space(),
            })
            .collect();

        let mut networks: Vec<Net> = self
            .networks
            .iter()
            .map(|(name, data)| Net {
                name: name.clone(),
                down_bytes_per_sec: data.received() as f64 / secs,
                up_bytes_per_sec: data.transmitted() as f64 / secs,
                total_down_bytes: data.total_received(),
                total_up_bytes: data.total_transmitted(),
            })
            .collect();
        networks.sort_by(|a, b| a.name.cmp(&b.name));

        #[cfg(windows)]
        let processes = self.procs.read(secs, cores);
        #[cfg(not(windows))]
        let processes: Vec<Proc> = self
            .sys
            .processes()
            .iter()
            // Linux lists each thread as well; only whole processes are wanted.
            .filter(|(_, p)| p.thread_kind().is_none())
            .map(|(pid, p)| {
                let io = p.disk_usage();
                let parent_pid = p.parent().map(|pp| pp.as_u32());
                let mut name = p.name().to_string_lossy().into_owned();
                // Linux kernel workers are numbered, such as "kworker/3:1".
                // Dropping the number shows them as one program.
                if cfg!(target_os = "linux") && parent_pid == Some(2) {
                    if let Some((kind, _)) = name.split_once('/') {
                        name = kind.to_string();
                    }
                }
                Proc {
                    pid: pid.as_u32(),
                    parent_pid,
                    name,
                    exe: p.exe().map(|e| e.to_string_lossy().into_owned()),
                    cpu_percent: p.cpu_usage() / cores,
                    memory_bytes: p.memory(),
                    start_secs: p.start_time(),
                    disk_read_bytes_per_sec: io.read_bytes as f64 / secs,
                    disk_write_bytes_per_sec: io.written_bytes as f64 / secs,
                }
            })
            .collect();

        Snapshot {
            taken_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0),
            interval_ms: elapsed.as_millis() as u64,
            system: SystemInfo {
                os_name: System::name(),
                os_version: System::os_version(),
                uptime_secs: System::uptime(),
            },
            cpu,
            memory,
            disks,
            networks,
            processes,
            connections: connections(),
        }
    }
}

/// Every open TCP and UDP socket, tied to the process that owns it.
fn connections() -> Vec<Conn> {
    let families = AddressFamilyFlags::IPV4 | AddressFamilyFlags::IPV6;
    let protocols = ProtocolFlags::TCP | ProtocolFlags::UDP;
    let Ok(sockets) = get_sockets_info(families, protocols) else {
        return Vec::new();
    };
    sockets
        .into_iter()
        .map(|s| {
            let pid = s.associated_pids.first().copied();
            match s.protocol_socket_info {
                ProtocolSocketInfo::Tcp(t) => Conn {
                    pid,
                    protocol: "tcp",
                    local_addr: t.local_addr.to_string(),
                    local_port: t.local_port,
                    remote_addr: Some(t.remote_addr.to_string()),
                    remote_port: Some(t.remote_port),
                    state: Some(t.state.to_string()),
                },
                ProtocolSocketInfo::Udp(u) => Conn {
                    pid,
                    protocol: "udp",
                    local_addr: u.local_addr.to_string(),
                    local_port: u.local_port,
                    remote_addr: None,
                    remote_port: None,
                    state: None,
                },
            }
        })
        .collect()
}

/// Per-core processor times read straight from Windows. Read-only.
#[cfg(windows)]
mod wincpu {
    use std::ffi::c_void;

    /// One core's cumulative times in 100 ns units. `kernel` includes `idle`.
    /// Mirrors SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION.
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct CoreTimes {
        idle: i64,
        kernel: i64,
        user: i64,
        dpc: i64,
        interrupt: i64,
        interrupt_count: u32,
    }

    #[link(name = "ntdll")]
    extern "system" {
        fn NtQuerySystemInformation(
            class: u32,
            info: *mut c_void,
            len: u32,
            returned: *mut u32,
        ) -> i32;
    }

    const SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION: u32 = 8;
    const MAX_CORES: usize = 256;

    pub fn read() -> Vec<CoreTimes> {
        let mut buf = vec![CoreTimes::default(); MAX_CORES];
        let mut returned = 0u32;
        // SAFETY: the buffer is valid for the length passed, and the call only
        // writes whole CoreTimes records into it, reporting the bytes written.
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_PROCESSOR_PERFORMANCE_INFORMATION,
                buf.as_mut_ptr().cast(),
                (buf.len() * size_of::<CoreTimes>()) as u32,
                &mut returned,
            )
        };
        if status < 0 {
            return Vec::new();
        }
        buf.truncate(returned as usize / size_of::<CoreTimes>());
        buf
    }

    /// Whole-machine load and load per core, 0 to 100, between two readings.
    pub fn load(before: &[CoreTimes], now: &[CoreTimes]) -> (f32, Vec<f32>) {
        if before.len() != now.len() {
            return (0.0, vec![0.0; now.len()]);
        }
        let (mut all_total, mut all_idle) = (0i64, 0i64);
        let cores = before
            .iter()
            .zip(now)
            .map(|(a, b)| {
                let total = (b.kernel - a.kernel) + (b.user - a.user);
                let idle = b.idle - a.idle;
                all_total += total;
                all_idle += idle;
                percent(total, idle)
            })
            .collect();
        (percent(all_total, all_idle), cores)
    }

    fn percent(total: i64, idle: i64) -> f32 {
        if total <= 0 {
            0.0
        } else {
            (100.0 * (total - idle) as f32 / total as f32).clamp(0.0, 100.0)
        }
    }
}

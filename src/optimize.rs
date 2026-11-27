//! Optimize: a report on this machine, the rules an agent is given for
//! reading it, and the findings that come back.
//!
//! Maestro gathers the report itself, from the system's own records. The
//! agent is handed the report as text and nothing else: it runs in an empty
//! folder, may not run commands or change files, and only advises. Nothing
//! is changed by this module.
//!
//! The report leaves the computer when the agent reads it, under the user's
//! own sign-in, so it is shown first and holds no user name, computer name,
//! serial number or file path.

use crate::telemetry::Snapshot;
use serde::Deserialize;
use std::fmt::Write as _;

/// How many programs the report names.
const PROGRAMS: usize = 40;

/// The rules the agent works by. Changing these changes what every user is
/// advised to do with their machine, so each line should be defensible.
pub const RULES: &str = "\
You are reviewing one computer for its owner. Below is a report Maestro gathered from the \
system's own records. Find what would make this machine faster, cooler, quieter or more reliable.

Rules:
1. Use only the report. Do not use any tool, read any file or look anything up. If the report \
does not show something, do not assume it.
2. Never advise switching off or weakening security: antivirus, the firewall, Secure Boot, TPM, \
disk encryption, user account prompts, memory integrity or system updates.
3. Never advise a registry cleaner, a driver-updater program, a 'debloat' or 'tweak' script, or \
any download that is not from the maker of the part or of the system.
4. Firmware (BIOS or UEFI): advise an update only from the board or computer maker's own page, \
only when the version shown is old enough to matter or fixes a stability or security problem you \
can name, and always say that a failed update can stop the machine from starting. You cannot see \
which version is newest, so say 'check whether a newer one exists', never 'you are out of date'.
5. Drivers: the same. Judge by the date shown. Graphics, chipset, storage and network drivers \
matter most. A driver supplied by the system's own update service is normally right as it is.
6. Memory: if modules run slower than the speed they are rated for, say the memory profile (XMP, \
EXPO or DOCP) may be off in firmware. The rated speed is often only in the part number, for example \
3600 in F4-3600C18; say this only when the part number or the reported speed shows a gap.
7. Settings: power plan, a drive that is nearly full, a hard disk holding the system, a device \
reporting a problem, a display running at a low refresh rate, and too many programs starting \
with the system are all fair findings when the report shows them.
8. Programs: a program marked 'not responding' or 'cold' is a candidate to close. A program using \
much memory or processor is a finding only if it looks unneeded; heavy use by something the owner \
is plainly using is not waste. Never advise closing a program marked critical or part of the \
system, and never advise ending security software.
9. Do not advise overclocking, undervolting, or anything that voids a warranty.
10. Do not pad. Three real findings beat ten weak ones. If the machine is in good order, say so \
with an empty list.

Answer with a JSON array and nothing else: no words before or after it, no code fence. Each item:
{\"title\": short name of the finding,
 \"area\": one of \"firmware\", \"drivers\", \"hardware\", \"settings\", \"programs\",
 \"gain\": one of \"high\", \"medium\", \"low\",
 \"risk\": one of \"none\", \"low\", \"medium\", \"high\",
 \"why\": one or two sentences citing what the report shows,
 \"how\": the steps, in plain words,
 \"program\": the program's name exactly as the report gives it, for a finding about one program, else \"\"}
Order the array with the largest gain first.";

/// One program as the report names it.
pub struct Program {
    pub name: String,
    pub processes: usize,
    pub cpu: f32,
    pub mem: u64,
    pub gpu_mem: u64,
    /// "App", "Background service" and so on.
    pub class: &'static str,
    /// "not responding" or "cold 40m".
    pub stale: Option<String>,
    pub paused: bool,
}

fn gb(bytes: u64) -> String {
    format!("{:.1} GB", bytes as f64 / 1_073_741_824.0)
}

/// The part of the report that comes from what Maestro already measures.
pub fn measured(snap: &Snapshot, programs: &[Program]) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "## System");
    let _ = writeln!(
        out,
        "{} {}, running for {} hours",
        snap.system.os_name.as_deref().unwrap_or("Unknown system"),
        snap.system.os_version.as_deref().unwrap_or(""),
        snap.system.uptime_secs / 3600
    );
    let _ = writeln!(
        out,
        "processor: {}, {} threads, {:.0}% busy now",
        snap.cpu.brand.trim(),
        snap.cpu.core_percent.len(),
        snap.cpu.total_percent
    );
    let _ = writeln!(out, "memory: {} of {} in use", gb(snap.memory.used_bytes), gb(snap.memory.total_bytes));
    if snap.memory.swap_total_bytes > 0 {
        let _ = writeln!(
            out,
            "swap or page file: {} of {} in use",
            gb(snap.memory.swap_used_bytes),
            gb(snap.memory.swap_total_bytes)
        );
    }
    for disk in &snap.disks {
        let _ = writeln!(
            out,
            "volume {} ({}): {} free of {}",
            disk.mount,
            disk.kind,
            gb(disk.free_bytes),
            gb(disk.total_bytes)
        );
    }
    for card in &snap.gpu.adapters {
        let _ = writeln!(
            out,
            "graphics card: {}, {:.0}% busy, {} of {} of its memory in use",
            card.name,
            card.load_percent,
            gb(card.dedicated_used_bytes),
            gb(card.dedicated_total_bytes)
        );
    }

    let _ = writeln!(out, "\n## Programs running now ({} in all; the stale and the largest are listed)", programs.len());
    let mut order: Vec<&Program> = programs.iter().collect();
    // Stale first, then by memory, so nothing flagged is cut off the end.
    order.sort_by(|a, b| b.stale.is_some().cmp(&a.stale.is_some()).then(b.mem.cmp(&a.mem)));
    let mut listed: Vec<&Program> = order.iter().take(PROGRAMS).copied().collect();
    // The busiest are worth naming even when they are small.
    let mut busy: Vec<&Program> = programs.iter().filter(|p| p.cpu >= 2.0).collect();
    busy.sort_by(|a, b| b.cpu.total_cmp(&a.cpu));
    for p in busy.into_iter().take(8) {
        if !listed.iter().any(|l| l.name == p.name) {
            listed.push(p);
        }
    }
    for p in listed {
        let mut line = format!("{}: {} processes, {:.1}% processor, {} memory", p.name, p.processes, p.cpu, gb(p.mem));
        if p.gpu_mem > 0 {
            let _ = write!(line, ", {} on the graphics card", gb(p.gpu_mem));
        }
        let _ = write!(line, ", {}", p.class.to_lowercase());
        if let Some(stale) = &p.stale {
            let _ = write!(line, ", {stale}");
        }
        if p.paused {
            line.push_str(", paused by Maestro");
        }
        let _ = writeln!(out, "{line}");
    }
    out
}

/// Take out anything that names the person or this particular machine.
pub fn scrub(text: &str, user: Option<&str>) -> String {
    const DROP: &[&str] = &["serial", "uuid", "udid", "user name", "computer name", "hostname", "host name"];
    let mut out = String::new();
    for line in text.lines() {
        let lower = line.to_lowercase();
        if DROP.iter().any(|word| lower.contains(word)) {
            continue;
        }
        let line = match user {
            Some(user) if user.len() >= 3 => line.replace(user, "(user)"),
            _ => line.to_string(),
        };
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// The whole text handed to the agent.
pub fn prompt(report: &str) -> String {
    format!("{RULES}\n\n# The report\n\n{report}")
}

/// One thing the agent advises.
#[derive(Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Finding {
    pub title: String,
    pub area: String,
    pub gain: String,
    pub risk: String,
    pub why: String,
    pub how: String,
    pub program: String,
}

impl Finding {
    fn rank(&self) -> u8 {
        match self.gain.as_str() {
            "high" => 0,
            "medium" => 1,
            _ => 2,
        }
    }
}

/// Read the agent's answer. `Err` holds the answer as it came, to be shown
/// as plain text, when it is not the list that was asked for.
pub fn read_findings(answer: &str) -> Result<Vec<Finding>, String> {
    let (Some(from), Some(to)) = (answer.find('['), answer.rfind(']')) else {
        return Err(answer.trim().to_string());
    };
    if to < from {
        return Err(answer.trim().to_string());
    }
    let mut found: Vec<Finding> =
        serde_json::from_str(&answer[from..=to]).map_err(|_| answer.trim().to_string())?;
    found.retain(|f| !f.title.trim().is_empty());
    found.sort_by_key(Finding::rank);
    Ok(found)
}

/// Run a fixed command and give back what it printed, or nothing if it
/// fails or takes too long. No window is opened.
#[cfg(any(windows, target_os = "macos"))]
fn printed(program: &str, args: &[&str], seconds: u64) -> Option<String> {
    use std::io::Read;
    use std::process::{Command, Stdio};
    let mut command = Command::new(program);
    command.args(args).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().ok()?;
    let mut pipe = child.stdout.take()?;
    // Read on another thread, so a full pipe cannot hold the command up.
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    });
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed().as_secs() < seconds => {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    let bytes = reader.join().ok()?;
    let text = String::from_utf8_lossy(&bytes).to_string();
    (!text.trim().is_empty()).then_some(text)
}

/// What Windows is asked. Every word of it is fixed here; nothing from
/// outside the program is put into it, and it changes nothing.
#[cfg(windows)]
const WINDOWS_QUESTIONS: &str = r###"
$ErrorActionPreference = 'SilentlyContinue'
function S($t) { ''; "## $t" }
function D($d) { if ($d) { $d.ToString('yyyy-MM-dd') } else { 'no date' } }
S 'Firmware (BIOS or UEFI)'
Get-CimInstance Win32_BIOS | ForEach-Object { "maker: $($_.Manufacturer)"; "version: $($_.SMBIOSBIOSVersion)"; "released: $(D $_.ReleaseDate)" }
S 'Computer and board'
Get-CimInstance Win32_ComputerSystem | ForEach-Object { "computer: $($_.Manufacturer) $($_.Model)" }
Get-CimInstance Win32_BaseBoard | ForEach-Object { "board: $($_.Manufacturer) $($_.Product)" }
S 'Processor'
Get-CimInstance Win32_Processor | ForEach-Object { "$("$($_.Name)".Trim()), $($_.NumberOfCores) cores, $($_.NumberOfLogicalProcessors) threads, virtualization on in firmware: $($_.VirtualizationFirmwareEnabled)" }
S 'Memory modules'
Get-CimInstance Win32_PhysicalMemory | ForEach-Object { "$([math]::Round($_.Capacity / 1GB)) GB, reported speed $($_.Speed) MHz, running at $($_.ConfiguredClockSpeed) MHz, $("$($_.Manufacturer)".Trim()) $("$($_.PartNumber)".Trim()), slot $($_.DeviceLocator)" }
S 'Drives'
Get-PhysicalDisk | ForEach-Object { "$($_.FriendlyName), $($_.MediaType), $($_.BusType), $([math]::Round($_.Size / 1GB)) GB, health $($_.HealthStatus)" }
S 'Displays'
Get-CimInstance Win32_VideoController | ForEach-Object { "$($_.Name), $($_.CurrentHorizontalResolution) by $($_.CurrentVerticalResolution) at $($_.CurrentRefreshRate) Hz" }
S 'Drivers not supplied by Microsoft'
Get-CimInstance Win32_PnPSignedDriver | Where-Object { $_.DeviceName -and $_.DriverProviderName -ne 'Microsoft' -and ('DISPLAY','NET','MEDIA','SCSIADAPTER','HDC','USB','BLUETOOTH','SYSTEM' -contains $_.DeviceClass) } | Sort-Object DeviceClass, DeviceName -Unique | Select-Object -First 60 | ForEach-Object { "$($_.DeviceClass): $($_.DeviceName), driver $($_.DriverVersion) dated $(D $_.DriverDate), from $($_.DriverProviderName)" }
S 'Devices reporting a problem'
$bad = @(Get-CimInstance Win32_PnPEntity | Where-Object { $_.ConfigManagerErrorCode -ne 0 } | Select-Object -First 20)
if ($bad.Count -eq 0) { 'none' } else { $bad | ForEach-Object { "$($_.Name): error code $($_.ConfigManagerErrorCode)" } }
S 'Power plan'
if ("$(powercfg /getactivescheme)" -match '^[^(]*\((.*)\)\s*$') { $Matches[1] } else { 'not known' }
S 'Programs set to start with the system'
Get-CimInstance Win32_StartupCommand | Select-Object -First 40 | ForEach-Object { "$($_.Name)" }
"###;

/// The part of the report that has to be asked of the system: firmware,
/// drivers and settings. Slow, so it is called off the window's thread.
#[cfg(windows)]
pub fn machine() -> String {
    let user = std::env::var("USERNAME").ok();
    match printed("powershell", &["-NoProfile", "-NonInteractive", "-Command", WINDOWS_QUESTIONS], 60) {
        Some(text) => scrub(&text, user.as_deref()),
        None => "## Firmware, drivers and settings\nWindows did not answer, so these are not known.\n".to_string(),
    }
}

/// On Linux everything asked for is a file the kernel keeps; no program is started.
#[cfg(target_os = "linux")]
pub fn machine() -> String {
    let read = |path: &str| std::fs::read_to_string(path).ok().map(|t| t.trim().to_string()).filter(|t| !t.is_empty());
    let mut out = String::new();
    let line = |out: &mut String, label: &str, path: &str| {
        if let Some(text) = read(path) {
            let _ = writeln!(out, "{label}: {}", text.lines().next().unwrap_or(""));
        }
    };
    let _ = writeln!(out, "\n## Firmware (BIOS or UEFI)");
    line(&mut out, "maker", "/sys/class/dmi/id/bios_vendor");
    line(&mut out, "version", "/sys/class/dmi/id/bios_version");
    line(&mut out, "released", "/sys/class/dmi/id/bios_date");
    let _ = writeln!(out, "\n## Computer and board");
    line(&mut out, "computer maker", "/sys/class/dmi/id/sys_vendor");
    line(&mut out, "computer", "/sys/class/dmi/id/product_name");
    line(&mut out, "board maker", "/sys/class/dmi/id/board_vendor");
    line(&mut out, "board", "/sys/class/dmi/id/board_name");
    let _ = writeln!(out, "\n## Kernel and drivers");
    line(&mut out, "kernel", "/proc/sys/kernel/osrelease");
    line(&mut out, "NVIDIA driver", "/proc/driver/nvidia/version");
    for module in ["nvidia", "amdgpu", "i915", "xe", "nouveau", "nvme", "r8169", "e1000e", "igc", "iwlwifi", "mt7921e"] {
        if std::path::Path::new(&format!("/sys/module/{module}")).is_dir() {
            let version = read(&format!("/sys/module/{module}/version")).unwrap_or_else(|| "comes with the kernel".to_string());
            let _ = writeln!(out, "driver {module}: loaded, version {version}");
        }
    }
    let _ = writeln!(out, "\n## Settings");
    line(&mut out, "processor governor", "/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor");
    line(&mut out, "processor driver", "/sys/devices/system/cpu/cpu0/cpufreq/scaling_driver");
    line(&mut out, "energy preference", "/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference");
    line(&mut out, "swappiness", "/proc/sys/vm/swappiness");
    line(&mut out, "transparent huge pages", "/sys/kernel/mm/transparent_hugepage/enabled");
    line(&mut out, "power profile", "/sys/firmware/acpi/platform_profile");
    let _ = writeln!(out, "\n## Drives");
    if let Ok(blocks) = std::fs::read_dir("/sys/block") {
        for block in blocks.flatten() {
            let name = block.file_name().to_string_lossy().to_string();
            if name.starts_with("loop") || name.starts_with("ram") || name.starts_with("zram") || name.starts_with("dm-") {
                continue;
            }
            let at = |leaf: &str| read(&format!("/sys/block/{name}/{leaf}")).unwrap_or_default();
            let kind = if at("queue/rotational") == "1" { "spinning disk" } else { "solid state" };
            let _ = writeln!(out, "{name}: {} {kind}, scheduler {}", at("device/model"), at("queue/scheduler"));
        }
    }
    scrub(&out, std::env::var("USER").ok().as_deref())
}

/// On macOS the system's own profiler is asked. Written from Apple's
/// documentation and not yet run on a Mac.
#[cfg(target_os = "macos")]
pub fn machine() -> String {
    let mut out = String::new();
    let asked = printed(
        "/usr/sbin/system_profiler",
        &["-detailLevel", "mini", "SPHardwareDataType", "SPSoftwareDataType", "SPDisplaysDataType", "SPStorageDataType", "SPMemoryDataType"],
        60,
    );
    let _ = writeln!(out, "\n## Hardware, firmware and software, from the system profiler");
    out.push_str(asked.as_deref().unwrap_or("The system profiler did not answer, so these are not known.\n"));
    if let Some(power) = printed("/usr/bin/pmset", &["-g"], 15) {
        let _ = writeln!(out, "\n## Power settings");
        out.push_str(&power);
    }
    scrub(&out, std::env::var("USER").ok().as_deref())
}

#[cfg(not(any(windows, target_os = "linux", target_os = "macos")))]
pub fn machine() -> String {
    "## Firmware, drivers and settings\nNot read on this system.\n".to_string()
}

/// `--optimize-report` prints the report an analysis would send, and sends nothing.
pub fn print_report() {
    let mut sampler = crate::telemetry::Sampler::new();
    sampler.sample();
    std::thread::sleep(std::time::Duration::from_millis(600));
    let snap = sampler.sample();
    let classes = crate::classify::classify(&snap);
    let mut by_name: std::collections::HashMap<&str, Program> = std::collections::HashMap::new();
    for p in &snap.processes {
        let program = by_name.entry(p.name.as_str()).or_insert_with(|| Program {
            name: p.name.clone(),
            processes: 0,
            cpu: 0.0,
            mem: 0,
            gpu_mem: 0,
            class: "App",
            stale: None,
            paused: false,
        });
        program.processes += 1;
        program.cpu += p.cpu_percent;
        program.mem += p.memory_bytes;
        program.gpu_mem += snap.gpu.of(p.pid).dedicated_bytes;
        if let Some(class) = classes.get(&p.pid) {
            program.class = class.label();
        }
    }
    let programs: Vec<Program> = by_name.into_values().collect();
    println!("{}{}", measured(&snap, &programs), machine());
    println!("\n(How long each program has been idle is known only to a running Maestro, so none is marked cold here.)");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_is_read_whether_or_not_words_surround_it() {
        let list = r#"[{"title":"Old graphics driver","area":"drivers","gain":"low","risk":"low","why":"w","how":"h"},
                       {"title":"Memory profile is off","area":"firmware","gain":"high","risk":"low","why":"w","how":"h","program":""},
                       {"title":"  "}]"#;
        for answer in [list.to_string(), format!("Here you go:\n```json\n{list}\n```\nThat is all.")] {
            let found = read_findings(&answer).unwrap();
            // Largest gain first, an item with no title dropped, a missing field left empty.
            assert_eq!(found.iter().map(|f| f.title.as_str()).collect::<Vec<_>>(), ["Memory profile is off", "Old graphics driver"]);
            assert_eq!(found[1].program, "");
        }
        assert_eq!(read_findings("[]").unwrap(), []);
    }

    #[test]
    fn an_answer_that_is_not_a_list_is_kept_as_words() {
        assert_eq!(read_findings(" I could not tell. ").unwrap_err(), "I could not tell.");
        assert!(read_findings("] broken [").is_err());
        assert!(read_findings("[not json]").is_err());
    }

    #[test]
    fn the_report_names_no_person_and_no_one_machine() {
        let raw = "Model Name: Laptop\n  Serial Number (system): C02ABC\nHardware UUID: 1-2-3\nUser Name: Sam Smith (sam)\n\
                   Computer Name: Sams-Laptop\nBoot Volume: Disk\nstartup: C:\\Users\\samsmith\\tool.exe\n";
        let clean = scrub(raw, Some("samsmith"));
        assert_eq!(clean, "Model Name: Laptop\nBoot Volume: Disk\nstartup: C:\\Users\\(user)\\tool.exe\n");
        // A very short name would strike out ordinary words, so it is left.
        assert_eq!(scrub("an apple", Some("a")), "an apple\n");
    }

    #[test]
    fn the_rules_keep_their_promises() {
        // Each of these is something a user is told Optimize will never advise.
        for must in ["Use only the report", "Never advise switching off or weakening security", "registry cleaner",
                     "can stop the machine from starting", "critical", "overclocking", "JSON array"] {
            assert!(RULES.contains(must), "The rules no longer say: {must}");
        }
        assert!(prompt("## System\nx").ends_with("# The report\n\n## System\nx"));
    }

    #[test]
    fn stale_and_busy_programs_are_never_cut_from_the_report() {
        let program = |name: &str, mem: u64, cpu: f32, stale: Option<&str>| Program {
            name: name.to_string(),
            processes: 1,
            cpu,
            mem,
            gpu_mem: 0,
            class: "App",
            stale: stale.map(str::to_string),
            paused: false,
        };
        let mut programs: Vec<Program> = (0..80).map(|i| program(&format!("big{i}"), 1 << 30, 0.0, None)).collect();
        programs.push(program("forgotten", 1024, 0.0, Some("cold 3h")));
        programs.push(program("spinner", 1024, 40.0, None));
        let snap = Snapshot {
            taken_ms: 0,
            interval_ms: 1000,
            system: crate::telemetry::SystemInfo { os_name: Some("Test".into()), os_version: None, uptime_secs: 7200 },
            cpu: crate::telemetry::Cpu { brand: "Chip".into(), total_percent: 5.0, core_percent: vec![5.0; 8] },
            memory: crate::telemetry::Memory { total_bytes: 16 << 30, used_bytes: 8 << 30, swap_total_bytes: 0, swap_used_bytes: 0 },
            disks: Vec::new(),
            networks: Vec::new(),
            processes: Vec::new(),
            connections: Vec::new(),
            gpu: Default::default(),
        };
        let text = measured(&snap, &programs);
        assert!(text.contains("forgotten: 1 processes, 0.0% processor, 0.0 GB memory, app, cold 3h"));
        assert!(text.contains("spinner: 1 processes, 40.0% processor"));
        assert!(text.contains("running for 2 hours"));
        assert!(text.matches("big").count() < 80);
    }
}

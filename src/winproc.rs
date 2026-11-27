//! Every process on the machine from one Windows system call. Read-only.
//!
//! Asking Windows about processes one at a time costs a fraction of a
//! millisecond each, which adds up to a noticeable load with hundreds of them
//! every second. `NtQuerySystemInformation` returns the whole table at once.
//! Only the path of the file behind a process needs a call of its own, and
//! that is looked up once per process and remembered.

use crate::telemetry::Proc;
use std::collections::HashMap;
use std::ffi::c_void;

/// Mirrors UNICODE_STRING.
#[repr(C)]
#[derive(Clone, Copy)]
struct UnicodeString {
    /// Length in bytes, not characters.
    length: u16,
    max_length: u16,
    buffer: *const u16,
}

/// Mirrors the fixed part of SYSTEM_PROCESS_INFORMATION on 64-bit Windows.
/// Times are in 100 ns units.
#[repr(C)]
#[derive(Clone, Copy)]
struct ProcessInfo {
    next_entry_offset: u32,
    thread_count: u32,
    working_set_private: i64,
    hard_fault_count: u32,
    thread_high_watermark: u32,
    cycle_time: u64,
    create_time: i64,
    user_time: i64,
    kernel_time: i64,
    image_name: UnicodeString,
    base_priority: i32,
    pid: usize,
    parent_pid: usize,
    handle_count: u32,
    session_id: u32,
    process_key: usize,
    peak_virtual_size: usize,
    virtual_size: usize,
    page_fault_count: u32,
    peak_working_set: usize,
    working_set: usize,
    quota_peak_paged: usize,
    quota_paged: usize,
    quota_peak_non_paged: usize,
    quota_non_paged: usize,
    pagefile_usage: usize,
    peak_pagefile_usage: usize,
    private_page_count: usize,
    read_operations: i64,
    write_operations: i64,
    other_operations: i64,
    read_bytes: i64,
    write_bytes: i64,
    other_bytes: i64,
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

#[link(name = "kernel32")]
extern "system" {
    fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
    fn QueryFullProcessImageNameW(
        process: *mut c_void,
        flags: u32,
        name: *mut u16,
        size: *mut u32,
    ) -> i32;
    fn CloseHandle(handle: *mut c_void) -> i32;
}

#[link(name = "ntdll")]
extern "system" {
    fn NtQueryInformationProcess(
        process: *mut c_void,
        class: u32,
        info: *mut c_void,
        len: u32,
        returned: *mut u32,
    ) -> i32;
}

const PROCESS_COMMAND_LINE_INFORMATION: u32 = 60;
const SYSTEM_PROCESS_INFORMATION: u32 = 5;
const STATUS_INFO_LENGTH_MISMATCH: i32 = 0xC000_0004_u32 as i32;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
/// Seconds between 1601, where Windows counts from, and 1970.
const WINDOWS_TO_UNIX_SECS: i64 = 11_644_473_600;

/// What was seen of a process last time, to turn running totals into rates.
struct Seen {
    create_time: i64,
    busy_time: i64,
    read_bytes: i64,
    write_bytes: i64,
    exe: Option<String>,
}

pub struct Reader {
    /// Kept between calls. `u64` elements keep the records aligned.
    buf: Vec<u64>,
    seen: HashMap<u32, Seen>,
}

impl Reader {
    pub fn new() -> Self {
        Self {
            buf: vec![0; 128 * 1024],
            seen: HashMap::new(),
        }
    }

    /// All processes, with rates averaged over the `secs` since the last call.
    pub fn read(&mut self, secs: f64, cores: f32) -> Vec<Proc> {
        loop {
            let mut returned = 0u32;
            // SAFETY: the buffer is valid for the byte length passed, and the
            // call writes at most that many bytes into it.
            let status = unsafe {
                NtQuerySystemInformation(
                    SYSTEM_PROCESS_INFORMATION,
                    self.buf.as_mut_ptr().cast(),
                    (self.buf.len() * 8) as u32,
                    &mut returned,
                )
            };
            if status == STATUS_INFO_LENGTH_MISMATCH {
                // The table grew. Leave room for processes that start meanwhile.
                self.buf.resize(returned as usize / 8 + 16 * 1024, 0);
                continue;
            }
            if status < 0 {
                return Vec::new();
            }
            break;
        }

        let base = self.buf.as_ptr() as *const u8;
        let len = self.buf.len() * 8;
        let mut seen = HashMap::with_capacity(self.seen.len() + 16);
        let mut out = Vec::with_capacity(self.seen.len() + 16);
        let mut offset = 0usize;
        while offset + size_of::<ProcessInfo>() <= len {
            // SAFETY: the bounds check above keeps the whole record inside the
            // buffer, and `read_unaligned` makes no assumption about alignment.
            let info: ProcessInfo = unsafe { std::ptr::read_unaligned(base.add(offset).cast()) };
            let pid = info.pid as u32;
            let busy_time = info.user_time + info.kernel_time;

            let old = self
                .seen
                .remove(&pid)
                .filter(|s| s.create_time == info.create_time);
            let (cpu_percent, read_rate, write_rate, exe) = match old {
                // Process 0 is the idle counter: its "busy" time is idle time.
                Some(s) if pid != 0 => (
                    ((busy_time - s.busy_time).max(0) as f64 / 1e7 / secs * 100.0) as f32 / cores,
                    (info.read_bytes - s.read_bytes).max(0) as f64 / secs,
                    (info.write_bytes - s.write_bytes).max(0) as f64 / secs,
                    s.exe,
                ),
                Some(s) => (0.0, 0.0, 0.0, s.exe),
                // First sighting: no rate yet. Look up its file once.
                None => (0.0, 0.0, 0.0, exe_path(pid)),
            };

            out.push(Proc {
                pid,
                parent_pid: (pid != 0).then_some(info.parent_pid as u32),
                name: name(&info, base, len),
                exe: exe.clone(),
                cpu_percent,
                // The private working set, the figure Task Manager calls "Memory".
                memory_bytes: info.working_set_private.max(0) as u64,
                start_secs: (info.create_time / 10_000_000 - WINDOWS_TO_UNIX_SECS).max(0) as u64,
                disk_read_bytes_per_sec: read_rate,
                disk_write_bytes_per_sec: write_rate,
            });
            seen.insert(
                pid,
                Seen {
                    create_time: info.create_time,
                    busy_time,
                    read_bytes: info.read_bytes,
                    write_bytes: info.write_bytes,
                    exe,
                },
            );

            if info.next_entry_offset == 0 {
                break;
            }
            offset += info.next_entry_offset as usize;
        }
        self.seen = seen;
        out
    }
}

/// The process's short name, read from inside the buffer.
fn name(info: &ProcessInfo, base: *const u8, len: usize) -> String {
    if info.pid == 0 {
        return "Idle".to_string();
    }
    let start = info.image_name.buffer as usize;
    let bytes = info.image_name.length as usize;
    let inside = start >= base as usize && start + bytes <= base as usize + len;
    if bytes == 0 || !inside {
        return format!("process {}", info.pid);
    }
    // SAFETY: the range was just checked to lie inside the buffer, and the
    // system writes names as whole UTF-16 units.
    let units = unsafe { std::slice::from_raw_parts(info.image_name.buffer, bytes / 2) };
    String::from_utf16_lossy(units)
}

/// Full path of the file behind a process, where Windows allows it to be read.
fn exe_path(pid: u32) -> Option<String> {
    // The idle counter and the kernel have no file.
    if pid == 0 || pid == 4 {
        return None;
    }
    // SAFETY: plain calls with a local buffer; the handle is closed before returning.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);
        (ok != 0).then(|| String::from_utf16_lossy(&buf[..size as usize]))
    }
}

/// The command line a process was started with, where Windows allows it to
/// be read. Used once per interpreter process to see what it is running.
pub fn command_line(pid: u32) -> Option<String> {
    if pid == 0 || pid == 4 {
        return None;
    }
    // A command line is at most 32,767 characters; this leaves room for the header.
    let mut buf = vec![0u64; 9 * 1024];
    let len = buf.len() * 8;
    // SAFETY: the buffer is valid for the length passed and the handle is
    // closed before returning. The returned text pointer is checked to lie
    // inside the buffer before it is read.
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut returned = 0u32;
        let status = NtQueryInformationProcess(
            handle,
            PROCESS_COMMAND_LINE_INFORMATION,
            buf.as_mut_ptr().cast(),
            len as u32,
            &mut returned,
        );
        CloseHandle(handle);
        if status < 0 {
            return None;
        }
        let text: UnicodeString = std::ptr::read(buf.as_ptr().cast());
        let base = buf.as_ptr() as usize;
        let start = text.buffer as usize;
        let bytes = text.length as usize;
        if bytes == 0 || start < base || start + bytes > base + len {
            return None;
        }
        Some(String::from_utf16_lossy(std::slice::from_raw_parts(
            text.buffer,
            bytes / 2,
        )))
    }
}

#[link(name = "advapi32")]
extern "system" {
    fn RegGetValueW(
        key: isize,
        sub_key: *const u16,
        value: *const u16,
        flags: u32,
        kind: *mut u32,
        data: *mut c_void,
        size: *mut u32,
    ) -> i32;
}

/// One text value from the registry, with `%NAME%` parts filled in. Read-only.
fn registry_text(root: isize, key: &str, value: &str) -> Option<String> {
    // Text values only; Windows fills in environment names unless told not to.
    const RRF_RT_REG_SZ: u32 = 0x2;
    let wide = |text: &str| text.encode_utf16().chain(std::iter::once(0)).collect::<Vec<u16>>();
    let (key, value) = (wide(key), wide(value));
    let mut size: u32 = 0;
    // SAFETY: both names end in a zero, and a null buffer asks only for the size.
    let status = unsafe {
        RegGetValueW(root, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), std::ptr::null_mut(), &mut size)
    };
    if status != 0 || size == 0 || size > 1 << 20 {
        return None;
    }
    let mut buffer = vec![0u16; (size as usize).div_ceil(2) + 1];
    let mut size = (buffer.len() * 2) as u32;
    // SAFETY: the buffer holds `size` bytes, and Windows writes no more than that.
    let status = unsafe {
        RegGetValueW(root, key.as_ptr(), value.as_ptr(), RRF_RT_REG_SZ, std::ptr::null_mut(), buffer.as_mut_ptr().cast(), &mut size)
    };
    if status != 0 {
        return None;
    }
    let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
    Some(String::from_utf16_lossy(&buffer[..end]))
}

/// The path as Windows holds it now: the system's, then the user's. A
/// program keeps the path it started with, so this is how a command an
/// installer has just added is found without a restart.
pub fn fresh_path() -> Option<String> {
    const HKEY_CURRENT_USER: isize = 0x8000_0001u32 as i32 as isize;
    const HKEY_LOCAL_MACHINE: isize = 0x8000_0002u32 as i32 as isize;
    let system = registry_text(HKEY_LOCAL_MACHINE, r"SYSTEM\CurrentControlSet\Control\Session Manager\Environment", "Path");
    let user = registry_text(HKEY_CURRENT_USER, "Environment", "Path");
    let joined: Vec<String> = [system, user].into_iter().flatten().collect();
    (!joined.is_empty()).then(|| joined.join(";"))
}

//! Graphics load and memory. Read-only.
//!
//! On Windows the system keeps counters for every graphics card and for
//! every program that uses one. They are the same numbers Task Manager
//! shows, they work for any maker's card, and reading them needs no extra
//! software. The card's name and how much memory it has come from DXGI,
//! the system's own list of graphics adapters.
//!
//! On Linux the kernel publishes load and memory for some cards as files
//! under /sys. That gives the whole card and nothing per program.
//!
//! macOS is not read yet.

#![cfg_attr(not(windows), allow(dead_code))]

use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Gpu {
    /// False when this system gives no way to read graphics cards yet.
    pub read: bool,
    pub adapters: Vec<Adapter>,
    /// What each process uses, by process id. Empty where the system does
    /// not say.
    pub procs: HashMap<u32, ProcGpu>,
}

#[derive(Serialize, Clone, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Adapter {
    pub name: String,
    /// How busy the busiest part of the card is, 0 to 100.
    pub load_percent: f32,
    /// Memory on the card itself.
    pub dedicated_used_bytes: u64,
    /// Zero when the system does not say.
    pub dedicated_total_bytes: u64,
    /// Ordinary memory the card has borrowed.
    pub shared_used_bytes: u64,
}

#[derive(Serialize, Clone, Copy, Default, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProcGpu {
    /// The busiest part of any card this process uses, 0 to 100.
    pub load_percent: f32,
    pub dedicated_bytes: u64,
    pub shared_bytes: u64,
    /// Windows sometimes reports more memory for one program than the whole
    /// card has in use. Such a figure is dropped, and this says so.
    pub overstated: bool,
}

impl Gpu {
    pub fn of(&self, pid: u32) -> ProcGpu {
        self.procs.get(&pid).copied().unwrap_or_default()
    }

    /// The busiest card, which is the one worth showing first.
    pub fn busiest(&self) -> Option<&Adapter> {
        self.adapters.iter().max_by(|a, b| {
            (a.load_percent, a.dedicated_used_bytes)
                .partial_cmp(&(b.load_percent, b.dedicated_used_bytes))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
    }
}

/// A card as the system identifies it: two numbers that stay the same until
/// the next restart.
type Luid = (u32, u32);

/// What the name of one counter says it is about.
#[derive(Default, Debug, PartialEq)]
struct About {
    pid: Option<u32>,
    luid: Option<Luid>,
    /// Which part of the card: 3D, video decode, copy and so on are numbered.
    engine: Option<u32>,
}

/// Read a counter's name, such as
/// `pid_1234_luid_0x00000000_0x0000D3E5_phys_0_eng_5_engtype_3D`.
fn about(name: &str) -> About {
    let hex = |text: &str| u32::from_str_radix(text.trim_start_matches("0x"), 16).ok();
    let words: Vec<&str> = name.split('_').collect();
    let mut out = About::default();
    let mut i = 0;
    while i < words.len() {
        match words[i] {
            "pid" => out.pid = words.get(i + 1).and_then(|w| w.parse().ok()),
            "luid" => {
                out.luid = words
                    .get(i + 1)
                    .and_then(|w| hex(w))
                    .zip(words.get(i + 2).and_then(|w| hex(w)));
            }
            "eng" => out.engine = words.get(i + 1).and_then(|w| w.parse().ok()),
            _ => {}
        }
        i += 1;
    }
    out
}

/// What is known about a card apart from its counters.
#[derive(Clone, Debug, Default, PartialEq)]
struct Card {
    luid: Luid,
    name: String,
    dedicated_total: u64,
    /// A stand-in the system draws with when there is no real card.
    software: bool,
}

/// One round of counters, as (name, value) pairs.
#[derive(Default)]
struct Counters {
    engine_load: Vec<(String, f64)>,
    proc_dedicated: Vec<(String, f64)>,
    proc_shared: Vec<(String, f64)>,
    card_dedicated: Vec<(String, f64)>,
    card_shared: Vec<(String, f64)>,
}

/// Turn one round of counters into what is shown.
///
/// A card has several engines that work side by side, so its load is that of
/// its busiest engine, not the sum. An engine's load is the sum of what each
/// program puts on it. A program's load is its busiest engine. This is how
/// Task Manager reads the same counters.
fn fold(cards: &[Card], c: &Counters) -> Gpu {
    let mut engine: HashMap<(Luid, u32), f64> = HashMap::new();
    let mut proc_engine: HashMap<(u32, Luid, u32), f64> = HashMap::new();
    for (name, value) in &c.engine_load {
        let a = about(name);
        let (Some(luid), Some(eng)) = (a.luid, a.engine) else { continue };
        *engine.entry((luid, eng)).or_default() += value;
        if let Some(pid) = a.pid {
            *proc_engine.entry((pid, luid, eng)).or_default() += value;
        }
    }

    let mut procs: HashMap<u32, ProcGpu> = HashMap::new();
    for ((pid, _, _), load) in &proc_engine {
        let p = procs.entry(*pid).or_default();
        p.load_percent = p.load_percent.max(load.min(100.0) as f32);
    }
    for (list, shared) in [(&c.proc_dedicated, false), (&c.proc_shared, true)] {
        for (name, value) in list {
            let Some(pid) = about(name).pid else { continue };
            let p = procs.entry(pid).or_default();
            let bytes = value.max(0.0) as u64;
            if shared {
                p.shared_bytes += bytes;
            } else {
                p.dedicated_bytes += bytes;
            }
        }
    }
    // The per-program figure is what a program has reserved, and for some
    // programs Windows reports far more than the card holds. The card's own
    // figure is sound, so anything above it is dropped rather than shown.
    let in_use: u64 = c.card_dedicated.iter().map(|(_, v)| v.max(0.0) as u64).sum();
    if in_use > 0 {
        for p in procs.values_mut() {
            if p.dedicated_bytes > in_use {
                p.dedicated_bytes = 0;
                p.overstated = true;
            }
        }
    }
    // A program that only once touched a card is not worth a line.
    procs.retain(|_, p| p.load_percent >= 0.05 || p.overstated || p.dedicated_bytes + p.shared_bytes > 0);

    let sum_for = |list: &[(String, f64)], luid: Luid| -> u64 {
        list.iter().filter(|(name, _)| about(name).luid == Some(luid)).map(|(_, v)| v.max(0.0) as u64).sum()
    };
    let adapters = cards
        .iter()
        .filter(|card| !card.software)
        .map(|card| Adapter {
            name: card.name.clone(),
            load_percent: engine
                .iter()
                .filter(|((luid, _), _)| *luid == card.luid)
                .map(|(_, load)| load.min(100.0) as f32)
                .fold(0.0, f32::max),
            dedicated_used_bytes: sum_for(&c.card_dedicated, card.luid),
            dedicated_total_bytes: card.dedicated_total,
            shared_used_bytes: sum_for(&c.card_shared, card.luid),
        })
        .collect();

    Gpu { read: true, adapters, procs }
}

pub struct Reader {
    #[cfg(windows)]
    win: Option<win::Query>,
}

impl Reader {
    pub fn new() -> Self {
        Self {
            #[cfg(windows)]
            win: win::Query::open(),
        }
    }

    /// Read the cards now. Load is averaged over the time since the last call,
    /// so the first call reports none.
    pub fn read(&mut self) -> Gpu {
        #[cfg(windows)]
        {
            match &mut self.win {
                Some(query) => {
                    let counters = query.collect();
                    fold(&query.cards, &counters)
                }
                None => Gpu::default(),
            }
        }
        #[cfg(target_os = "linux")]
        {
            linux::read()
        }
        #[cfg(not(any(windows, target_os = "linux")))]
        {
            Gpu::default()
        }
    }
}

/// `--gpu`: print each card and the programs using it.
pub fn print() {
    let mut reader = Reader::new();
    reader.read();
    std::thread::sleep(std::time::Duration::from_millis(1000));
    let gpu = reader.read();
    if !gpu.read {
        println!("Graphics cards are not read on this system yet.");
        return;
    }
    if gpu.adapters.is_empty() {
        println!("No graphics card was found.");
    }
    let mb = |bytes: u64| bytes as f64 / (1024.0 * 1024.0);
    for a in &gpu.adapters {
        println!("{}", a.name);
        println!("  load {:.0}%", a.load_percent);
        match a.dedicated_total_bytes {
            0 => println!("  memory on the card: {:.0} MB in use", mb(a.dedicated_used_bytes)),
            total => println!("  memory on the card: {:.0} of {:.0} MB in use", mb(a.dedicated_used_bytes), mb(total)),
        }
        println!("  shared memory: {:.0} MB in use", mb(a.shared_used_bytes));
    }
    let mut procs: Vec<(&u32, &ProcGpu)> = gpu.procs.iter().collect();
    procs.sort_by(|a, b| {
        (b.1.load_percent, b.1.dedicated_bytes)
            .partial_cmp(&(a.1.load_percent, a.1.dedicated_bytes))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if !procs.is_empty() {
        println!("{} processes use a card. The busiest:", procs.len());
    }
    if procs.iter().any(|(_, p)| p.overstated) {
        println!("  (\"not known\": Windows reported more for that process than the whole card has in use.)");
    }
    for (pid, p) in procs.iter().take(15) {
        let on_card = match p.overstated {
            true => "   not known".to_string(),
            false => format!("{:>9.0} MB", mb(p.dedicated_bytes)),
        };
        println!(
            "  pid {pid:<7} load {:>5.1}%   on the card {on_card}   shared {:>7.0} MB",
            p.load_percent,
            mb(p.shared_bytes)
        );
    }
}

#[cfg(target_os = "linux")]
mod linux {
    use super::{Adapter, Gpu};

    /// Cards the kernel publishes numbers for. AMD and some Intel drivers do;
    /// others list the card with no numbers, and it is shown without them.
    pub fn read() -> Gpu {
        let mut adapters = Vec::new();
        let Ok(cards) = std::fs::read_dir("/sys/class/drm") else {
            return Gpu { read: true, ..Gpu::default() };
        };
        let number = |path: std::path::PathBuf| -> Option<u64> {
            std::fs::read_to_string(path).ok()?.trim().parse().ok()
        };
        let mut names: Vec<_> = cards.flatten().map(|e| e.file_name().to_string_lossy().to_string()).collect();
        names.sort();
        for name in names {
            // "card0" is a card; "card0-HDMI-A-1" is one of its sockets.
            if !name.starts_with("card") || name.contains('-') {
                continue;
            }
            let device = std::path::Path::new("/sys/class/drm").join(&name).join("device");
            let load = number(device.join("gpu_busy_percent"));
            let used = number(device.join("mem_info_vram_used"));
            let total = number(device.join("mem_info_vram_total"));
            if load.is_none() && used.is_none() {
                continue;
            }
            adapters.push(Adapter {
                name,
                load_percent: load.unwrap_or(0) as f32,
                dedicated_used_bytes: used.unwrap_or(0),
                dedicated_total_bytes: total.unwrap_or(0),
                shared_used_bytes: number(device.join("mem_info_gtt_used")).unwrap_or(0),
            });
        }
        Gpu { read: true, adapters, procs: Default::default() }
    }
}

#[cfg(windows)]
mod win {
    use super::{Card, Counters};
    use std::ffi::c_void;

    type Handle = isize;

    /// Mirrors PDH_FMT_COUNTERVALUE_ITEM_W with the value read as a double.
    #[repr(C)]
    struct Item {
        name: *const u16,
        status: u32,
        value: f64,
    }

    #[repr(C)]
    struct Guid(u32, u16, u16, [u8; 8]);

    /// Mirrors DXGI_ADAPTER_DESC1.
    #[repr(C)]
    struct Desc {
        description: [u16; 128],
        vendor: u32,
        device: u32,
        subsystem: u32,
        revision: u32,
        dedicated_video: usize,
        dedicated_system: usize,
        shared_system: usize,
        luid_low: u32,
        luid_high: i32,
        flags: u32,
    }

    const MORE_DATA: u32 = 0x8000_07D2;
    const AS_DOUBLE: u32 = 0x0000_0200;
    /// Without this a counter is cut off at 100, which hides nothing useful
    /// here but would make a sum of parts wrong.
    const NO_CAP: u32 = 0x0000_8000;
    const SOFTWARE: u32 = 2;

    #[link(name = "pdh")]
    extern "system" {
        fn PdhOpenQueryW(source: *const u16, user: usize, query: *mut Handle) -> u32;
        fn PdhAddEnglishCounterW(query: Handle, path: *const u16, user: usize, counter: *mut Handle) -> u32;
        fn PdhCollectQueryData(query: Handle) -> u32;
        fn PdhGetFormattedCounterArrayW(
            counter: Handle,
            format: u32,
            size: *mut u32,
            count: *mut u32,
            items: *mut Item,
        ) -> u32;
        fn PdhCloseQuery(query: Handle) -> u32;
    }

    #[link(name = "dxgi")]
    extern "system" {
        fn CreateDXGIFactory1(iid: *const Guid, factory: *mut *mut c_void) -> i32;
    }

    pub struct Query {
        query: Handle,
        engine_load: Handle,
        proc_dedicated: Handle,
        proc_shared: Handle,
        card_dedicated: Handle,
        card_shared: Handle,
        pub cards: Vec<Card>,
    }

    impl Query {
        pub fn open() -> Option<Self> {
            let mut query: Handle = 0;
            // SAFETY: a null source means the live system; `query` is a valid place for the handle.
            if unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) } != 0 {
                return None;
            }
            let add = |path: &str| -> Handle {
                let wide: Vec<u16> = path.encode_utf16().chain(std::iter::once(0)).collect();
                let mut counter: Handle = 0;
                // SAFETY: `wide` ends in a zero and outlives the call.
                unsafe { PdhAddEnglishCounterW(query, wide.as_ptr(), 0, &mut counter) };
                counter
            };
            let made = Self {
                query,
                engine_load: add(r"\GPU Engine(*)\Utilization Percentage"),
                proc_dedicated: add(r"\GPU Process Memory(*)\Dedicated Usage"),
                proc_shared: add(r"\GPU Process Memory(*)\Shared Usage"),
                card_dedicated: add(r"\GPU Adapter Memory(*)\Dedicated Usage"),
                card_shared: add(r"\GPU Adapter Memory(*)\Shared Usage"),
                cards: cards(),
            };
            // SAFETY: the handle was just opened.
            unsafe { PdhCollectQueryData(made.query) };
            Some(made)
        }

        pub fn collect(&self) -> Counters {
            // SAFETY: the handle is open for as long as `self` lives.
            unsafe { PdhCollectQueryData(self.query) };
            Counters {
                engine_load: values(self.engine_load),
                proc_dedicated: values(self.proc_dedicated),
                proc_shared: values(self.proc_shared),
                card_dedicated: values(self.card_dedicated),
                card_shared: values(self.card_shared),
            }
        }
    }

    impl Drop for Query {
        fn drop(&mut self) {
            // SAFETY: the handle is open and is not used again.
            unsafe { PdhCloseQuery(self.query) };
        }
    }

    /// Every instance of one counter, as its name and value.
    fn values(counter: Handle) -> Vec<(String, f64)> {
        if counter == 0 {
            return Vec::new();
        }
        let (mut size, mut count) = (0u32, 0u32);
        // SAFETY: with no buffer the call only reports the size it needs.
        let first = unsafe {
            PdhGetFormattedCounterArrayW(counter, AS_DOUBLE | NO_CAP, &mut size, &mut count, std::ptr::null_mut())
        };
        if first != MORE_DATA || size == 0 {
            return Vec::new();
        }
        // The buffer holds the items and then the text of their names.
        // u64 cells keep it aligned for the items. A little spare room covers
        // a program starting between the two calls.
        size += 4096;
        let mut buffer = vec![0u64; size as usize / 8 + 1];
        // SAFETY: the buffer is at least `size` bytes and aligned for `Item`.
        let status = unsafe {
            PdhGetFormattedCounterArrayW(counter, AS_DOUBLE | NO_CAP, &mut size, &mut count, buffer.as_mut_ptr().cast())
        };
        if status != 0 {
            return Vec::new();
        }
        let mut out = Vec::with_capacity(count as usize);
        // SAFETY: the call filled `count` items at the start of the buffer.
        let items = unsafe { std::slice::from_raw_parts(buffer.as_ptr().cast::<Item>(), count as usize) };
        for item in items {
            // 0 and 1 mean the value is good.
            if item.status > 1 || item.name.is_null() {
                continue;
            }
            // SAFETY: the name points into the buffer and ends in a zero.
            let name = unsafe {
                let mut len = 0;
                while *item.name.add(len) != 0 {
                    len += 1;
                }
                String::from_utf16_lossy(std::slice::from_raw_parts(item.name, len))
            };
            out.push((name, item.value));
        }
        out
    }

    /// The system's list of graphics adapters: name, memory and identity.
    fn cards() -> Vec<Card> {
        // IDXGIFactory1.
        let iid = Guid(0x770a_ae78, 0xf26f, 0x4dba, [0xa8, 0x29, 0x25, 0x3c, 0x83, 0xd1, 0xb3, 0x87]);
        let mut factory: *mut c_void = std::ptr::null_mut();
        // SAFETY: `factory` is a valid place for the pointer.
        if unsafe { CreateDXGIFactory1(&iid, &mut factory) } < 0 || factory.is_null() {
            return Vec::new();
        }
        // A COM object starts with a pointer to its table of functions.
        // The slots used here are fixed by the published interface:
        // Release is 2 on every object, EnumAdapters1 is 12 on the factory
        // and GetDesc1 is 10 on an adapter.
        type Release = unsafe extern "system" fn(*mut c_void) -> u32;
        type EnumAdapters1 = unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void) -> i32;
        type GetDesc1 = unsafe extern "system" fn(*mut c_void, *mut Desc) -> i32;
        // SAFETY: `object` is a live COM object, whose first field is its table.
        let slot = |object: *mut c_void, index: usize| unsafe { *(*(object as *const *const *const c_void)).add(index) };
        let mut out = Vec::new();
        for index in 0..16 {
            let mut adapter: *mut c_void = std::ptr::null_mut();
            // SAFETY: slot 12 of IDXGIFactory1 has this signature.
            let enumerate: EnumAdapters1 = unsafe { std::mem::transmute(slot(factory, 12)) };
            if unsafe { enumerate(factory, index, &mut adapter) } < 0 || adapter.is_null() {
                break;
            }
            // SAFETY: Desc is plain data, valid when zeroed.
            let mut desc: Desc = unsafe { std::mem::zeroed() };
            // SAFETY: slot 10 of IDXGIAdapter1 has this signature.
            let describe: GetDesc1 = unsafe { std::mem::transmute(slot(adapter, 10)) };
            if unsafe { describe(adapter, &mut desc) } >= 0 {
                let len = desc.description.iter().position(|&c| c == 0).unwrap_or(desc.description.len());
                out.push(Card {
                    luid: (desc.luid_high as u32, desc.luid_low),
                    name: String::from_utf16_lossy(&desc.description[..len]).trim().to_string(),
                    dedicated_total: desc.dedicated_video as u64,
                    software: desc.flags & SOFTWARE != 0,
                });
            }
            // SAFETY: slot 2 is Release on every COM object.
            let release: Release = unsafe { std::mem::transmute(slot(adapter, 2)) };
            unsafe { release(adapter) };
        }
        // SAFETY: as above; the factory is not used again.
        let release: Release = unsafe { std::mem::transmute(slot(factory, 2)) };
        unsafe { release(factory) };
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CARD: &str = "luid_0x00000000_0x0000D3E5";

    #[test]
    fn a_counter_name_says_which_process_card_and_engine() {
        let a = about("pid_1234_luid_0x00000000_0x0000D3E5_phys_0_eng_5_engtype_3D");
        assert_eq!(a, About { pid: Some(1234), luid: Some((0, 0xD3E5)), engine: Some(5) });
        let a = about("luid_0x00000001_0x0000ABCD_phys_0");
        assert_eq!(a, About { pid: None, luid: Some((1, 0xABCD)), engine: None });
        assert_eq!(about("nonsense"), About::default());
        assert_eq!(about("pid_x_luid_0xZZ"), About::default());
    }

    #[test]
    fn a_card_is_as_busy_as_its_busiest_engine_and_so_is_a_program() {
        let cards = [
            Card { luid: (0, 0xD3E5), name: "Real card".into(), dedicated_total: 8 << 30, software: false },
            Card { luid: (0, 0x1), name: "Basic Render Driver".into(), dedicated_total: 0, software: true },
        ];
        let c = Counters {
            engine_load: vec![
                // Two programs on the 3D engine, one of them also decoding video.
                (format!("pid_10_{CARD}_phys_0_eng_0_engtype_3D"), 30.0),
                (format!("pid_20_{CARD}_phys_0_eng_0_engtype_3D"), 25.0),
                (format!("pid_20_{CARD}_phys_0_eng_3_engtype_VideoDecode"), 40.0),
                (format!("pid_30_{CARD}_phys_0_eng_0_engtype_3D"), 0.0),
            ],
            proc_dedicated: vec![(format!("pid_10_{CARD}_phys_0"), 1000.0), (format!("pid_20_{CARD}_phys_0"), 500.0)],
            proc_shared: vec![(format!("pid_10_{CARD}_phys_0"), 70.0)],
            card_dedicated: vec![(format!("{CARD}_phys_0"), 1800.0), ("luid_0x00000000_0x00000001_phys_0".into(), 5.0)],
            card_shared: vec![(format!("{CARD}_phys_0"), 90.0)],
        };
        let gpu = fold(&cards, &c);
        assert!(gpu.read);
        // The stand-in the system draws with is not a card.
        assert_eq!(gpu.adapters.len(), 1);
        let card = &gpu.adapters[0];
        // The 3D engine carries 30 + 25; video decode carries 40. The card is at 55, not 95.
        assert_eq!(card.load_percent, 55.0);
        assert_eq!((card.dedicated_used_bytes, card.dedicated_total_bytes, card.shared_used_bytes), (1800, 8 << 30, 90));
        assert_eq!(gpu.of(10), ProcGpu { load_percent: 30.0, dedicated_bytes: 1000, shared_bytes: 70, overstated: false });
        // Process 20 is at 40, its busier engine, not 65.
        assert_eq!(gpu.of(20).load_percent, 40.0);
        // A program with no load and no memory is left out; asking for it gives zeros.
        assert!(!gpu.procs.contains_key(&30));
        assert_eq!(gpu.of(30), ProcGpu::default());
        assert_eq!(gpu.busiest().unwrap().name, "Real card");

        // A program said to hold more than the whole card has in use is not believed.
        let mut c = c;
        c.proc_dedicated.push((format!("pid_40_{CARD}_phys_0"), 67_000.0));
        let gpu = fold(&cards, &c);
        assert_eq!((gpu.of(40).dedicated_bytes, gpu.of(40).overstated), (0, true));
        assert_eq!((gpu.of(10).dedicated_bytes, gpu.of(10).overstated), (1000, false));
    }

    #[test]
    fn load_never_reads_above_a_hundred() {
        let cards = [Card { luid: (0, 0xD3E5), name: "c".into(), dedicated_total: 0, software: false }];
        let c = Counters {
            engine_load: vec![
                (format!("pid_1_{CARD}_phys_0_eng_0_engtype_3D"), 70.0),
                (format!("pid_2_{CARD}_phys_0_eng_0_engtype_3D"), 60.0),
            ],
            ..Counters::default()
        };
        assert_eq!(fold(&cards, &c).adapters[0].load_percent, 100.0);
    }
}

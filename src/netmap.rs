//! Who this computer is talking to: connections grouped by program, the ports
//! it is listening on, and the names behind the addresses.
//!
//! Everything here observes. The only thing it sends is a name lookup per
//! address to the DNS server the system already uses, and that can be
//! switched off.

use crate::telemetry::Snapshot;
use std::collections::{BTreeMap, HashMap};
use std::net::IpAddr;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// How far away the other end of a connection is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Reach {
    /// This computer talking to itself.
    Own,
    /// The home or office network: private and link-local addresses.
    Local,
    /// Somewhere on the internet.
    Public,
}

impl Reach {
    pub fn label(self) -> &'static str {
        match self {
            Reach::Own => "this computer",
            Reach::Local => "your network",
            Reach::Public => "internet",
        }
    }
}

/// Sort an address by how far away it is. Text that is not an address counts
/// as public, so nothing unknown is ever shown as safe.
pub fn reach(addr: &str) -> Reach {
    match addr.parse::<IpAddr>() {
        Ok(IpAddr::V4(ip)) => {
            let [a, b, ..] = ip.octets();
            if ip.is_loopback() || ip.is_unspecified() {
                Reach::Own
            } else if ip.is_private()
                || ip.is_link_local()
                || ip.is_multicast()
                || ip.is_broadcast()
                // Addresses internet providers share between customers.
                || (a == 100 && (64..128).contains(&b))
            {
                Reach::Local
            } else {
                Reach::Public
            }
        }
        Ok(IpAddr::V6(ip)) => {
            if let Some(v4) = ip.to_ipv4_mapped() {
                return reach(&v4.to_string());
            }
            let first = ip.segments()[0];
            if ip.is_loopback() || ip.is_unspecified() {
                Reach::Own
            } else if first & 0xffc0 == 0xfe80 || first & 0xfe00 == 0xfc00 || first & 0xff00 == 0xff00 {
                Reach::Local
            } else {
                Reach::Public
            }
        }
        Err(_) => Reach::Public,
    }
}

/// What a port number is normally used for, in plain words.
pub fn service(port: u16) -> Option<&'static str> {
    Some(match port {
        21 => "file transfer (ftp)",
        22 => "remote shell (ssh)",
        23 => "telnet, unencrypted",
        25 | 465 | 587 => "sending mail",
        53 => "name lookups (dns)",
        67 | 68 => "address assignment (dhcp)",
        80 | 8080 => "web, unencrypted",
        123 => "clock sync",
        135 => "Windows remote calls",
        137..=139 => "Windows name service",
        143 | 993 => "reading mail (imap)",
        443 | 8443 => "web, encrypted",
        445 => "Windows file sharing",
        500 | 4500 => "VPN (ipsec)",
        853 => "name lookups, encrypted",
        1194 => "VPN (OpenVPN)",
        1883 | 8883 => "device messages (mqtt)",
        1900 => "device discovery (ssdp)",
        3306 => "MySQL database",
        3389 => "remote desktop",
        3478 | 3479 => "calls (stun)",
        5228 => "Google push messages",
        5353 => "device discovery (mdns)",
        5355 => "name lookups on your network",
        5432 => "PostgreSQL database",
        6379 => "Redis database",
        27017 => "MongoDB database",
        51820 => "VPN (WireGuard)",
        _ => return None,
    })
}

/// The organisation a looked-up name belongs to, for the names seen most.
/// It is read from the name alone, so it is a hint and not a check.
pub fn owner(name: &str) -> Option<&'static str> {
    const OWNERS: &[(&str, &str)] = &[
        ("1e100.net", "Google"),
        ("google.com", "Google"),
        ("googleusercontent.com", "Google Cloud"),
        ("amazonaws.com", "Amazon Web Services"),
        ("cloudfront.net", "Amazon CloudFront"),
        ("akamaitechnologies.com", "Akamai"),
        ("akamai.net", "Akamai"),
        ("linode.com", "Akamai (Linode)"),
        ("msedge.net", "Microsoft"),
        ("microsoft.com", "Microsoft"),
        ("azure.com", "Microsoft Azure"),
        ("windows.net", "Microsoft Azure"),
        ("github.com", "GitHub"),
        ("fbcdn.net", "Meta"),
        ("facebook.com", "Meta"),
        ("cloudflare.com", "Cloudflare"),
        ("cloudflare.net", "Cloudflare"),
        ("fastly.net", "Fastly"),
        ("apple.com", "Apple"),
        ("anthropic.com", "Anthropic"),
        ("openai.com", "OpenAI"),
        ("digitalocean.com", "DigitalOcean"),
        ("your-server.de", "Hetzner"),
        ("hetzner.com", "Hetzner"),
        ("ovh.net", "OVH"),
        ("valve.net", "Valve"),
        ("steamcontent.com", "Valve"),
        ("discord.gg", "Discord"),
        ("discord.com", "Discord"),
    ];
    let name = name.trim_end_matches('.').to_ascii_lowercase();
    OWNERS
        .iter()
        .find(|(suffix, _)| {
            name == *suffix
                || name
                    .strip_suffix(suffix)
                    .is_some_and(|head| head.ends_with('.'))
        })
        .map(|(_, owner)| *owner)
}

/// One place a program is connected to.
#[derive(Debug, PartialEq)]
pub struct Dest {
    pub addr: String,
    pub port: u16,
    pub reach: Reach,
    /// How many connections go there.
    pub count: usize,
}

/// A program and everywhere it is connected to.
#[derive(Debug, PartialEq)]
pub struct Talker {
    pub program: String,
    pub dests: Vec<Dest>,
    /// Destinations on the internet.
    pub public: usize,
}

/// A port a program is waiting on for others to connect to.
#[derive(Debug, PartialEq)]
pub struct Listener {
    pub program: String,
    pub port: u16,
    pub protocol: &'static str,
    /// True when other computers can reach it, as far as the socket says.
    /// A firewall may still stop them.
    pub open: bool,
}

#[derive(Debug, Default, PartialEq)]
pub struct Map {
    pub talkers: Vec<Talker>,
    pub listeners: Vec<Listener>,
}

impl Map {
    pub fn public(&self) -> usize {
        self.talkers.iter().map(|t| t.public).sum()
    }

    pub fn open(&self) -> usize {
        self.listeners.iter().filter(|l| l.open).count()
    }
}

/// Sort every socket in a sample into who is talking to whom and who is listening.
pub fn map(snap: &Snapshot) -> Map {
    let names: HashMap<u32, &str> = snap
        .processes
        .iter()
        .map(|p| (p.pid, p.name.as_str()))
        .collect();
    let program = |pid: Option<u32>| match pid {
        // The system files connections under process 0 once their owner has
        // let go of them and they are winding down.
        Some(0) => "closing, no owner any more".to_string(),
        Some(pid) => match names.get(&pid) {
            Some(name) => name.to_string(),
            None => format!("process {pid}, not listed"),
        },
        None => "unknown".to_string(),
    };

    let mut talking: BTreeMap<String, BTreeMap<(String, u16), usize>> = BTreeMap::new();
    let mut listening: BTreeMap<(u16, &'static str, String), bool> = BTreeMap::new();
    for c in &snap.connections {
        let remote = c
            .remote_addr
            .as_deref()
            .zip(c.remote_port)
            .filter(|(addr, port)| *port != 0 && !unspecified(addr));
        match remote {
            Some((addr, port)) => {
                *talking
                    .entry(program(c.pid))
                    .or_default()
                    .entry((addr.to_string(), port))
                    .or_default() += 1;
            }
            // No far end: a TCP socket waiting for callers, or a UDP socket.
            None => {
                let waiting = c.state.as_deref().is_none_or(|s| s.eq_ignore_ascii_case("listen"));
                if waiting {
                    let open = reach(&c.local_addr) != Reach::Own || unspecified(&c.local_addr);
                    *listening
                        .entry((c.local_port, c.protocol, program(c.pid)))
                        .or_default() |= open;
                }
            }
        }
    }

    let mut talkers: Vec<Talker> = talking
        .into_iter()
        .map(|(program, dests)| {
            let mut dests: Vec<Dest> = dests
                .into_iter()
                .map(|((addr, port), count)| Dest {
                    reach: reach(&addr),
                    addr,
                    port,
                    count,
                })
                .collect();
            // Furthest away first; the map already put them in address order.
            dests.sort_by(|a, b| b.reach.cmp(&a.reach));
            let public = dests.iter().filter(|d| d.reach == Reach::Public).count();
            Talker {
                program,
                dests,
                public,
            }
        })
        .collect();
    talkers.sort_by(|a, b| b.public.cmp(&a.public).then_with(|| a.program.cmp(&b.program)));

    let mut listeners: Vec<Listener> = listening
        .into_iter()
        .map(|((port, protocol, program), open)| Listener {
            program,
            port,
            protocol,
            open,
        })
        .collect();
    listeners.sort_by(|a, b| b.open.cmp(&a.open));
    Map { talkers, listeners }
}

fn unspecified(addr: &str) -> bool {
    addr.parse::<IpAddr>().is_ok_and(|ip| ip.is_unspecified())
}

// ---- names behind addresses ---------------------------------------------

enum Lookup {
    Pending,
    Missing,
    Found(String),
}

/// Looks up the name registered for an address, off the window's thread, and
/// remembers the answers.
pub struct Names {
    cache: Arc<Mutex<HashMap<String, Lookup>>>,
    tx: Sender<String>,
    /// When false, nothing new is looked up.
    pub on: bool,
}

const WORKERS: usize = 8;
const CACHE_LIMIT: usize = 5000;

impl Names {
    /// `wake` is called whenever an answer arrives.
    pub fn start(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let cache: Arc<Mutex<HashMap<String, Lookup>>> = Arc::default();
        let (tx, rx) = channel::<String>();
        let rx: Arc<Mutex<Receiver<String>>> = Arc::new(Mutex::new(rx));
        let wake = Arc::new(wake);
        // A lookup with no answer can take seconds, so several run side by side.
        for _ in 0..WORKERS {
            let (rx, cache, wake) = (rx.clone(), cache.clone(), wake.clone());
            std::thread::spawn(move || {
                // Opening any socket makes the standard library start Windows'
                // networking, which the lookup below needs.
                let _ = std::net::UdpSocket::bind("127.0.0.1:0");
                loop {
                    let next = match rx.lock() {
                        Ok(rx) => rx.recv(),
                        Err(_) => return,
                    };
                    let Ok(addr) = next else { return };
                    let found = addr.parse::<IpAddr>().ok().and_then(reverse);
                    if let Ok(mut cache) = cache.lock() {
                        cache.insert(addr, found.map_or(Lookup::Missing, Lookup::Found));
                    }
                    wake();
                }
            });
        }
        Self {
            cache,
            tx,
            on: true,
        }
    }

    /// The name for an address, if one is known yet. The first request for an
    /// address starts its lookup. Addresses inside this computer are not asked about.
    pub fn get(&self, addr: &str) -> Option<String> {
        let mut cache = self.cache.lock().ok()?;
        match cache.get(addr) {
            Some(Lookup::Found(name)) => Some(name.clone()),
            Some(_) => None,
            None => {
                if self.on && reach(addr) != Reach::Own {
                    if cache.len() >= CACHE_LIMIT {
                        cache.clear();
                    }
                    cache.insert(addr.to_string(), Lookup::Pending);
                    let _ = self.tx.send(addr.to_string());
                }
                None
            }
        }
    }

    /// How many lookups have not come back yet.
    pub fn pending(&self) -> usize {
        self.cache
            .lock()
            .map(|c| c.values().filter(|l| matches!(l, Lookup::Pending)).count())
            .unwrap_or(0)
    }
}

/// Lay an address out the way the system's lookup call wants it.
fn sockaddr(ip: IpAddr) -> Vec<u8> {
    #[cfg(windows)]
    const AF_INET6: u16 = 23;
    #[cfg(target_os = "macos")]
    const AF_INET6: u16 = 30;
    #[cfg(not(any(windows, target_os = "macos")))]
    const AF_INET6: u16 = 10;
    const AF_INET: u16 = 2;

    let (family, len) = match ip {
        IpAddr::V4(_) => (AF_INET, 16usize),
        IpAddr::V6(_) => (AF_INET6, 28usize),
    };
    let mut out = vec![0u8; len];
    // macOS and the BSDs start with a length byte and a one-byte family.
    if cfg!(any(target_os = "macos", target_os = "ios", target_os = "freebsd")) {
        out[0] = len as u8;
        out[1] = family as u8;
    } else {
        out[..2].copy_from_slice(&family.to_ne_bytes());
    }
    match ip {
        // Family, port, then the four address bytes.
        IpAddr::V4(v4) => out[4..8].copy_from_slice(&v4.octets()),
        // Family, port, flow label, then the sixteen address bytes.
        IpAddr::V6(v6) => out[8..24].copy_from_slice(&v6.octets()),
    }
    out
}

/// Ask the system for the name registered for an address.
fn reverse(ip: IpAddr) -> Option<String> {
    #[cfg(windows)]
    #[link(name = "ws2_32")]
    extern "system" {
        fn getnameinfo(
            addr: *const u8,
            addr_len: i32,
            host: *mut u8,
            host_len: u32,
            service: *mut u8,
            service_len: u32,
            flags: i32,
        ) -> i32;
    }
    #[cfg(not(windows))]
    extern "C" {
        fn getnameinfo(
            addr: *const u8,
            addr_len: u32,
            host: *mut u8,
            host_len: u32,
            service: *mut u8,
            service_len: u32,
            flags: i32,
        ) -> i32;
    }
    /// Fail when there is no name, instead of handing the address back as text.
    #[cfg(target_os = "linux")]
    const NAME_REQUIRED: i32 = 8;
    #[cfg(not(target_os = "linux"))]
    const NAME_REQUIRED: i32 = 4;

    /// The call reads numbers out of the address, so it must sit on an even boundary.
    #[repr(C, align(8))]
    struct Aligned([u8; 32]);

    let bytes = sockaddr(ip);
    let mut addr = Aligned([0; 32]);
    addr.0[..bytes.len()].copy_from_slice(&bytes);
    let mut host = [0u8; 1025];
    // SAFETY: both buffers are valid for the lengths passed, the address is
    // laid out as the call expects, and no service name is asked for.
    let status = unsafe {
        getnameinfo(
            addr.0.as_ptr(),
            bytes.len() as _,
            host.as_mut_ptr(),
            host.len() as u32,
            std::ptr::null_mut(),
            0,
            NAME_REQUIRED,
        )
    };
    if status != 0 {
        return None;
    }
    let end = host.iter().position(|b| *b == 0)?;
    let name = String::from_utf8_lossy(&host[..end]).trim().to_string();
    (!name.is_empty() && name.parse::<IpAddr>().is_err()).then_some(name)
}

/// `--connections`: print who is talking to whom and what is listening.
pub fn print(snap: &Snapshot, lookups: bool) {
    let map = map(snap);
    let mut names = Names::start(|| {});
    names.on = lookups;
    if lookups {
        for dest in map.talkers.iter().flat_map(|t| &t.dests) {
            names.get(&dest.addr);
        }
        // Give the lookups up to twenty seconds; whatever has no answer by then goes without.
        for _ in 0..200 {
            if names.pending() == 0 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }
    println!(
        "{} programs connected, {} destinations on the internet, {} ports open to other computers",
        map.talkers.len(),
        map.public(),
        map.open()
    );
    for talker in &map.talkers {
        println!("\n{} ({} on the internet)", talker.program, talker.public);
        for d in &talker.dests {
            let name = names.get(&d.addr);
            let owner = name.as_deref().and_then(owner).map(|o| format!(" [{o}]")).unwrap_or_default();
            println!(
                "  {:<13} {}:{}  {}{}{}{}",
                d.reach.label(),
                d.addr,
                d.port,
                name.as_deref().unwrap_or("-"),
                owner,
                service(d.port).map(|s| format!("  ({s})")).unwrap_or_default(),
                if d.count > 1 { format!("  x{}", d.count) } else { String::new() },
            );
        }
    }
    println!("\nlistening");
    for l in &map.listeners {
        println!(
            "  {} {:<5} {}  {}{}",
            l.protocol,
            l.port,
            if l.open { "other computers can reach it" } else { "this computer only" },
            l.program,
            service(l.port).map(|s| format!("  ({s})")).unwrap_or_default(),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telemetry::{Conn, Cpu, Memory, Proc, Snapshot, SystemInfo};

    fn conn(pid: u32, protocol: &'static str, local: &str, port: u16, remote: Option<(&str, u16)>, state: Option<&str>) -> Conn {
        Conn {
            pid: Some(pid),
            protocol,
            local_addr: local.to_string(),
            local_port: port,
            remote_addr: remote.map(|r| r.0.to_string()),
            remote_port: remote.map(|r| r.1),
            state: state.map(str::to_string),
        }
    }

    fn snapshot(connections: Vec<Conn>) -> Snapshot {
        let proc = |pid, name: &str| Proc {
            pid,
            parent_pid: None,
            name: name.to_string(),
            exe: None,
            cpu_percent: 0.0,
            memory_bytes: 0,
            start_secs: 0,
            disk_read_bytes_per_sec: 0.0,
            disk_write_bytes_per_sec: 0.0,
        };
        Snapshot {
            taken_ms: 0,
            interval_ms: 0,
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
            processes: vec![proc(10, "browser"), proc(11, "browser"), proc(20, "server")],
            connections,
        }
    }

    #[test]
    fn addresses_are_sorted_by_distance() {
        for own in ["127.0.0.1", "::1", "0.0.0.0", "::", "::ffff:127.0.0.1"] {
            assert_eq!(reach(own), Reach::Own, "{own}");
        }
        for local in ["192.168.1.20", "10.0.0.1", "172.16.5.5", "169.254.1.1", "100.64.0.1", "fe80::1", "fd00::1", "224.0.0.251", "ff02::fb"] {
            assert_eq!(reach(local), Reach::Local, "{local}");
        }
        for public in ["8.8.8.8", "172.32.0.1", "100.128.0.1", "2606:4700::1111", "not an address"] {
            assert_eq!(reach(public), Reach::Public, "{public}");
        }
    }

    #[test]
    fn connections_are_grouped_by_program_and_counted() {
        let map = map(&snapshot(vec![
            conn(10, "tcp", "192.168.1.5", 50000, Some(("8.8.8.8", 443)), Some("ESTABLISHED")),
            conn(11, "tcp", "192.168.1.5", 50001, Some(("8.8.8.8", 443)), Some("ESTABLISHED")),
            conn(10, "tcp", "192.168.1.5", 50002, Some(("192.168.1.1", 80)), Some("ESTABLISHED")),
            conn(20, "tcp", "127.0.0.1", 50003, Some(("127.0.0.1", 9000)), Some("ESTABLISHED")),
            conn(99, "tcp", "192.168.1.5", 50004, Some(("1.1.1.1", 53)), Some("ESTABLISHED")),
        ]));
        let programs: Vec<&str> = map.talkers.iter().map(|t| t.program.as_str()).collect();
        // Programs reaching the internet come first.
        assert_eq!(programs, ["browser", "process 99, not listed", "server"]);
        let browser = &map.talkers[0];
        assert_eq!(browser.public, 1);
        assert_eq!(browser.dests[0], Dest { addr: "8.8.8.8".into(), port: 443, reach: Reach::Public, count: 2 });
        assert_eq!(browser.dests[1].reach, Reach::Local);
        assert_eq!(map.talkers[2].public, 0);
        assert_eq!(map.public(), 2);
    }

    #[test]
    fn listeners_say_whether_other_computers_can_reach_them() {
        let map = map(&snapshot(vec![
            conn(20, "tcp", "0.0.0.0", 8080, Some(("0.0.0.0", 0)), Some("LISTEN")),
            conn(20, "tcp", "127.0.0.1", 9000, Some(("0.0.0.0", 0)), Some("LISTEN")),
            conn(20, "tcp", "::1", 9000, Some(("::", 0)), Some("LISTEN")),
            conn(10, "udp", "192.168.1.5", 5353, None, None),
            // A closing connection with no far end is neither talking nor listening.
            conn(10, "tcp", "192.168.1.5", 50000, Some(("0.0.0.0", 0)), Some("CLOSED")),
        ]));
        assert!(map.talkers.is_empty());
        let seen: Vec<(u16, &str, bool)> = map.listeners.iter().map(|l| (l.port, l.protocol, l.open)).collect();
        assert_eq!(seen, [(5353, "udp", true), (8080, "tcp", true), (9000, "tcp", false)]);
        assert_eq!(map.open(), 2);
    }

    #[test]
    fn owners_match_whole_name_parts_only() {
        assert_eq!(owner("lhr25s33-in-f14.1e100.net"), Some("Google"));
        assert_eq!(owner("ec2-3-9-1-1.eu-west-2.compute.amazonaws.com."), Some("Amazon Web Services"));
        assert_eq!(owner("GitHub.com"), Some("GitHub"));
        assert_eq!(owner("notgithub.com"), None);
        assert_eq!(owner("router.lan"), None);
    }

    #[test]
    fn addresses_are_laid_out_for_the_lookup_call() {
        let v4 = sockaddr("1.2.3.4".parse().unwrap());
        assert_eq!(v4.len(), 16);
        assert_eq!(&v4[4..8], &[1, 2, 3, 4]);
        let v6 = sockaddr("2001:db8::1".parse().unwrap());
        assert_eq!(v6.len(), 28);
        assert_eq!(&v6[8..12], &[0x20, 0x01, 0x0d, 0xb8]);
        assert_eq!(v6[23], 1);
    }

    /// Needs the internet, so it only runs when asked for: `cargo test -- --ignored`.
    #[test]
    #[ignore]
    fn a_well_known_address_has_a_name() {
        let _ = std::net::UdpSocket::bind("127.0.0.1:0");
        assert_eq!(reverse("8.8.8.8".parse().unwrap()).as_deref(), Some("dns.google"));
        assert_eq!(reverse("2001:4860:4860::8888".parse().unwrap()).as_deref(), Some("dns.google"));
    }

    #[test]
    fn nothing_is_looked_up_when_switched_off_or_for_this_computer() {
        let mut names = Names::start(|| {});
        assert_eq!(names.get("127.0.0.1"), None);
        assert_eq!(names.pending(), 0);
        names.on = false;
        assert_eq!(names.get("8.8.8.8"), None);
        assert_eq!(names.pending(), 0);
    }
}

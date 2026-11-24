# Research: the four limits on full control

Checked 2026-11-24. This covers the first four items under "Limits on full control" in [plan.md](../plan.md). Nothing here has been tested on a machine; it is desk research only.

How to read the sources:

- **[P]** the primary page was fetched and read on 2026-11-24.
- **[S]** seen only as a search-result summary on 2026-11-24; the page itself was not read. Treat as likely but unconfirmed.
- **Not verified** means no source was found or read. These are collected again at the end.

Licence notes are a plain reading of the published terms, not legal advice.

Plan phases referred to below: 1 Monitor (read-only), 2 Controls, 3 Agents, 4 Depth, 5 Router.

---

## 1. Encrypted traffic contents

### Summary

Payloads of TLS traffic cannot be read in general. What can be improved is the *metadata* (which name a process asked for, which host it connected to) and, for apps that cooperate or that the user explicitly chooses to inspect, the contents. Both name sources (DNS and SNI) are being eroded by encrypted DNS and Encrypted Client Hello, so the app must be able to say "name hidden" instead of guessing.

### Achievable now without special privileges

- **Connection to process mapping.** `GetExtendedTcpTable` with `TCP_TABLE_OWNER_PID_*` returns the owning PID for each TCP endpoint, IPv4 and IPv6. The reference page states no privilege requirement. [P] <https://learn.microsoft.com/en-us/windows/win32/api/iphlpapi/nf-iphlpapi-getextendedtcptable>
- **Per-app key logging the user sets up themselves.** Setting the `SSLKEYLOGFILE` environment variable before launching an app makes supporting TLS libraries write session secrets to a file. It is an ordinary environment variable, so no elevation is needed for the user's own processes.
  - Honoured by: Firefox, Chrome, Chromium-based Edge (79+), curl, and apps using OpenSSL 3.4 or later. [P] <https://wiki.wireshark.org/TLS>
  - curl supports it only when built with OpenSSL, AWS-LC, BoringSSL, GnuTLS or wolfSSL; Schannel is not on the list. [P] <https://everything.curl.dev/usingcurl/tls/sslkeylogfile.html> The curl.exe that ships in Windows uses Schannel as far as I know, so it is probably excluded (not verified).
  - **Not honoured** by anything using Windows Schannel (so: most .NET, WinHTTP and WinINet apps, PowerShell, many Windows services). [P] <https://wiki.wireshark.org/TLS> No Microsoft document was found describing a native Schannel key log. The known workaround hooks LSASS and needs LSA protection turned off, which Maestro should not do. [S] <https://crushonme.github.io/2025/05/07/Decrypt-TLS-Traffic-on-Windows>
  - The format is now RFC 9850 (Informational, July 2026). It states that access to the file breaks confidentiality and integrity of every logged connection and that it is intended for systems where TLS protects test data. [S] <https://www.rfc-editor.org/rfc/rfc9850.html>
  - The keys are useless without a packet capture to decrypt, which needs admin (below).

### Achievable with admin or an opt-in component

- **DNS names per process (ETW).** The `Microsoft-Windows-DNS-Client` provider (GUID `{1c95126e-7eea-49a9-a3fe-a378b03ddb4d}`) emits event 3008 for completed queries with the query name and the calling process. [S] <https://docs.velociraptor.app/artifact_references/pages/windows.etw.dns/> [S] <https://www.github.com/asgarciap/etw-dns>
  - Starting an ETW session needs an administrator, membership of Performance Log Users, or a LocalSystem/LocalService/NetworkService service. The NT Kernel Logger session needs an administrator or LocalSystem. [S] <https://learn.microsoft.com/windows/win32/api/evntrace/nf-evntrace-starttracea>
  - Blind spot: this only sees the Windows DNS client. Browsers doing their own DNS-over-HTTPS bypass it. Windows' own DoH (Windows 11 / Server 2022 onwards) still goes through the DNS client, so it should remain visible, but that was not tested. [P] <https://learn.microsoft.com/en-us/windows-server/networking/dns/doh-client-support>
- **Connection and byte counts per process (ETW).** The kernel TCP/IP event classes carry PID and size for send, receive, connect and disconnect. [S] <https://learn.microsoft.com/en-us/windows/win32/etw/tcpip-sendipv4> One third-party tool reports it can only attach to `Microsoft-Windows-Kernel-Network` as SYSTEM. [S] <https://docs.velociraptor.app/artifact_references/pages/windows.etw.kernelnetwork> The exact privilege needed is not verified.
- **Packet capture with nothing to install: pktmon.** In-box since Windows 10 build 19041 (the syntax page says 1809). Captures full packets (`--pkt-size 0`), has a real-time mode, writes ETL and converts to pcapng. Filters are by MAC, IP, port, EtherType, protocol and VLAN only: there is **no process filter**, so packets must be joined to PIDs by 5-tuple against the TCP table. [P] <https://learn.microsoft.com/en-us/windows-server/networking/technologies/pktmon/pktmon> [P] <https://learn.microsoft.com/en-us/windows-server/networking/technologies/pktmon/pktmon-syntax> The pages do not state the privilege needed; in practice it requires elevation (not verified from the docs).
  - This is the route to **SNI**: the server name in a TLS ClientHello is plaintext unless ECH is used.
- **Packet capture with Npcap.** Better API, but the licence is a problem for a public MIT project:
  - The free licence allows five installs per user and says Npcap "may not be redistributed or used in other software without special permission". Copies used solely with Nmap, Wireshark or Microsoft Defender for Identity do not count toward the limit. For open-source authors the Nmap Project recommends asking users to download and install Npcap themselves. [P] <https://raw.githubusercontent.com/nmap/npcap/master/LICENSE>
  - Bundling needs the OEM redistribution licence: USD 39,980 to 79,980 perpetual. The free build has no silent installer. [P] <https://npcap.com/oem/redist>
  - Conclusion: Maestro must not bundle Npcap. It may detect a user-installed copy, but the five-copy limit and "used in other software" wording make even that worth asking the Nmap Project about before relying on it.
- **WinDivert.** User-mode capture-and-divert for Windows 10/11, LGPL, signed driver, needs Administrator. [P] <https://reqrypt.org/windivert.html> Can be shipped as a separate, replaceable DLL and driver alongside MIT code under LGPL terms. It is frequently flagged by antivirus and anti-cheat products (from memory, not verified).
- **HTTP detail before encryption (ETW), WinINet/WinHTTP apps only.**
  - `Microsoft-Windows-WinINet-Capture` has four events: request and response headers and payloads, in plaintext. Seen on Windows 10 1511 to Windows 11 21H2. [S] <https://winevt-kb.readthedocs.io/en/latest/sources/eventlog-providers/Provider-Microsoft-Windows-WinINet-Capture.html> [S] <https://cloud.google.com/blog/topics/threat-intelligence/introducing-pywintrace-python-wrapper-etw>
  - `Microsoft-Windows-WinHttp` is mostly proxy events; `Microsoft-Windows-WebIO` gives more, with headers in an intermediate form. [S] <https://gary-nebbett.blogspot.com/2018/06/tracing-https-traffic-on-microsoft.html>
  - These sources are from 2017 to 2021. Whether the providers still behave this way on Windows 11 24H2/25H2 is **not verified**. They cover nothing that uses its own TLS stack (browsers, Electron, Go, Rust, Java, Python).
  - No documented Schannel ETW provider exposing plaintext was found.
- **Opt-in local TLS-intercepting proxy.** mitmproxy's local capture mode on Windows starts a privileged redirector that uses WinDivert and can target specific PIDs or process names (`--mode local:curl`). [P] <https://mitmproxy.org/posts/local-capture/windows/> What breaks or costs:
  - **Certificate pinning:** pinned apps reject the proxy's certificates "without modifying them". [P] <https://docs.mitmproxy.org/stable/concepts/certificates/>
  - **Root certificate risk:** the user must trust a locally generated CA whose private key sits in a file in the user profile (`~/.mitmproxy/mitmproxy-ca.pem`). Anything that can read that file can impersonate any site to this machine. The CA is unique per install. [P] same page.
  - **HTTP/3 / QUIC:** mitmproxy supports HTTP/3 in local, reverse and WireGuard modes, QUIC v1 only, tested mainly against curl. [P] <https://docs.mitmproxy.org/stable/concepts/protocols/> A simpler proxy would have to block UDP 443 to force a fall back to TCP.
  - mitmproxy's own licence was not re-read this session (it is MIT as far as I know).
- **Windows Filtering Platform from user mode.** WFP's user-mode API covers connection monitoring and filtering by application; looking inside packets or streams needs a kernel callout driver. [S] <https://learn.microsoft.com/windows-hardware/drivers/network/porting-packet-processing-drivers-and-apps-to-wfp> The scope of `FwpmNetEventSubscribe`/`FwpmConnectionSubscribe` was not read.

### Needs a kernel driver

- A WFP callout driver, for per-process stream inspection or redirection without third-party components. Signing requirements are in section 3.

### What erodes name visibility

- **DNS-over-HTTPS** in the app itself (browsers) hides lookups from the DNS-Client ETW provider and from the wire.
- **Encrypted Client Hello** is RFC 9849 (Proposed Standard, March 2026) and encrypts the real server name; an observer sees only the provider's public name. Chrome and Firefox support it by default. [S] <https://rfc-editor.org/rfc/rfc9849> [S] <https://dl.ifip.org/db/conf/networking/networking2026/1571261276.pdf>
  - Firefox uses ECH only when DoH is configured (default on since Firefox 119). [S] <https://support.mozilla.org/kb/understand-encrypted-client-hello> Whether current Chrome needs Secure DNS for ECH is not verified.
  - How much of the web has ECH turned on (for example whether Cloudflare enables it by default) is not verified.

### Remains impossible

- Reading the contents of an app that pins certificates and uses its own TLS stack, without modifying or injecting into that app. Maestro should not inject.
- Reading traffic of protected processes or anything Schannel-based without weakening LSA protection.
- Recovering the true hostname when both DNS and the ClientHello are encrypted. Only the IP and its owner remain.

### Recommended approach for Maestro

| Step | What | Privilege | Earliest phase |
|---|---|---|---|
| a | Connections tied to PIDs via IP Helper | none | 1 |
| b | Hostname per connection from DNS-Client ETW, through a small elevated helper service | admin once, at install | 1 (optional), otherwise 2 when the elevated helper exists for controls |
| c | SNI from pktmon capture limited to TCP 443 ClientHello packets, joined to PIDs by 5-tuple | admin | 2 |
| d | Label every connection with how its name was learned: DNS, SNI, reverse lookup, or "hidden (encrypted DNS / ECH)" | none | 1 |
| e | "Inspect this app" mode: relaunch a chosen app with `SSLKEYLOGFILE`, capture with pktmon, decrypt. Works for Chromium, Firefox, curl/OpenSSL apps only. Delete keys when the session ends. | admin | 4 |
| f | Opt-in per-process intercepting proxy with a per-machine CA, off by default, time-limited, with the CA removed on stop | admin + user-installed root | 4, and only if (e) proves insufficient |

Do not bundle Npcap. Keep the plan's current wording that content inspection is not on by default.

---

## 2. Location is approximate

### Summary

Free data gives a reliable owner (ASN and organisation), a reliable country in most cases, and a city that is right roughly two times in three. Anycast, CDNs and VPNs make the "where" meaningless for a large share of traffic, so the app should show the owner first and state the kind of destination before showing a pin on a map.

### Data sources and licences

| Source | Data | Licence | Bundle in an MIT repo/installer? |
|---|---|---|---|
| DB-IP Lite | country, city, ASN; CSV and MMDB; monthly | CC BY 4.0, attribution to DB-IP.com (link back where results are shown) | Yes, with attribution. "Reduced coverage and accuracy" versus paid. [P] <https://db-ip.com/db/lite.php> |
| IPinfo Lite | country, continent, ASN, AS name and domain; MMDB/CSV/JSON/Parquet; daily; API advertised as unlimited | CC BY-SA 4.0, attribution to IPinfo | Data file yes, as a separately licensed file with attribution. Share-alike applies to the data and adaptations of it, not to Maestro's code. No city level. [P] <https://ipinfo.io/lite> |
| MaxMind GeoLite2 | country, city, ASN | GeoLite2 EULA (plus CC BY-SA 4.0 for copyrightable parts) | **No.** The EULA restricts disclosing the databases to third parties, requires attribution, and requires old versions to be destroyed within 30 days of a new release. Use only as "bring your own account and key". [P] <https://www.maxmind.com/en/geolite/eula> |
| sapics/ip-location-db | repackaged sets; some under PDDL 1.0 (iptoasn, RIR-derived country and ASN) | per dataset | PDDL sets yes, but the page notes many upstream sources lack explicit licences. [P] <https://github.com/sapics/ip-location-db> |
| Team Cymru IP-to-ASN | live DNS/whois lookups | free community service; no stated redistribution or commercial terms | Query live, do not bundle. Keep to DNS for volume; heavy whois use is null-routed. Not for geolocation. [P] <https://www.team-cymru.com/ip-asn-mapping> |
| RDAP / WHOIS (ARIN) | registered owner, abuse contact | ARIN Whois terms | Live lookup for operational purposes is allowed; **no republishing**, and no use "as part of a commercial service or product". Fine for a free tool querying on the user's behalf with a local cache; do not ship a copy. [P] <https://www.arin.net/resources/registry/whois/tou/> RIPE, APNIC, LACNIC and AFRINIC terms and RDAP rate limits were not checked. |

### Accuracy limits

- MaxMind's own figure for its databases: about 66% of US addresses resolve to the right city within 50 km; lower for mobile and VPN addresses; "never precise enough to identify or locate a specific household, individual, or street address". [P] <https://support.maxmind.com/hc/en-us/articles/4407630607131-Geolocation-Accuracy> Free tiers are stated to be less accurate than paid. No independent accuracy figure for DB-IP Lite or IPinfo Lite was found.
- A geolocation database places an address where the vendor believes its users or servers are. For a VPN or proxy it is the exit server, not the far end.

### Labelling anycast, CDN, VPN and proxy honestly

- **Anycast:** one address is served from many sites, so a single map pin is wrong by design.
  - bgp.tools publishes detected anycast prefixes for IPv4 and IPv6, updated after each scan. **No licence is stated** in the repository. [P] <https://github.com/bgptools/anycast-prefixes>
  - University of Twente's daily anycast census publishes /24 anycast prefixes; tooling is stated as MPL 2.0. The licence of the data itself was not confirmed. [S] <https://ut-dacs.github.io/anycast-census>
  - Before bundling either, ask the maintainers or fetch at run time with the source shown.
- **CDN / cloud:** the ASN and AS name from the table above identify most of them. Several providers publish their own address ranges (Cloudflare, AWS, Google, Azure, Fastly); those lists and their terms were not fetched this session.
- **VPN / proxy / Tor:** no free, redistributable VPN/proxy dataset was confirmed. IPinfo Lite does not include privacy flags. The Tor Project publishes exit lists (not fetched this session). Treat VPN detection as best effort from AS name and type.
- **Latency sanity check (no data needed):** light in fibre covers roughly 100 km per millisecond of round-trip time. If the measured round trip to the destination is 4 ms, it is within about 400 km whatever the database says. Use this to mark a database location as "implausible" and to spot anycast.
- **Suggested wording on the map:** owner first ("Cloudflare, AS13335"), then kind ("anycast content network: served from the nearest of many sites"), then location with its uncertainty ("registered in US; nearest site likely within ~300 km"). Never draw a precise pin for anycast, VPN or low-confidence results.

### Traceroute from user mode

- `IcmpSendEcho2` (IPv4) and `Icmp6SendEcho2` (IPv6) in `iphlpapi.dll` send echo requests without raw sockets, synchronously or asynchronously, and accept an `IP_OPTION_INFORMATION` structure for the request. Setting its TTL to 1, 2, 3... and reading the address of each "TTL expired" reply is a traceroute. [P] <https://learn.microsoft.com/en-us/windows/win32/api/icmpapi/nf-icmpapi-icmpsendecho2>
- The page states no privilege requirement. This is the same API family the in-box `ping`/`tracert` use and it works unelevated in practice (not stated in the doc; needs a quick test).
- Limits: many routers do not answer or rate-limit; ICMP may take a different path from the app's TCP/UDP flow; return paths are invisible. A TCP-SYN traceroute along the real flow would need raw packets (admin plus a capture/inject driver such as WinDivert).

### Privilege summary

- **No privileges:** everything above except TCP traceroute.
- **Admin / opt-in:** TCP traceroute; online lookups (IPinfo API, RDAP, Team Cymru) are opt-in because they disclose the looked-up addresses to a third party.
- **Kernel driver / hardware:** nothing useful.
- **Impossible:** knowing where data ends up behind a CDN, VPN, proxy or anycast address; knowing a server's physical location with certainty from its address alone.

### Recommended approach for Maestro

| Step | What | Earliest phase |
|---|---|---|
| a | Bundle DB-IP Lite city + ASN (MMDB) with attribution in the UI and in a `THIRD-PARTY` notice; refresh monthly | 1 |
| b | Classify each destination from its AS: ISP, hosting, CDN, anycast, VPN/unknown; show owner before location | 1 |
| c | ICMP traceroute via `IcmpSendEcho2` on demand, with unanswered hops shown as gaps; latency sanity check | 1 |
| d | Opt-in online enrichment: RDAP for the registered owner (cached, never redistributed), IPinfo Lite API | 1 or 2 |
| e | Optional "use my own GeoLite2 key" setting; never ship the file | 4 |
| f | Anycast prefix list once a licence is confirmed | 2 |

---

## 3. Not an antivirus; kernel-level malware can hide

### Summary

Maestro cannot become a kernel-level security product: that needs a signed driver and, for the most valuable telemetry, an anti-malware vendor relationship with Microsoft. What it can do is lean on what Windows already provides (Defender, signature checks, optionally Sysmon), add independent cross-checks that catch careless or user-mode hiding, and be explicit that a kernel rootkit defeats all of it.

### Achievable now without special privileges

- **Authenticode and catalog signature verification** with `WinVerifyTrust` and the `CryptCATAdmin*` catalog functions. These are standard user-mode APIs; the reference pages were not re-read this session. <https://learn.microsoft.com/en-us/windows/win32/api/wintrust/nf-wintrust-winverifytrust>
- **Hash reputation, opt-in.**
  - VirusTotal public API: 500 requests per day and 4 per minute; "must not be used in commercial products or services" nor in "business workflows that do not contribute new files". [P] <https://docs.virustotal.com/reference/public-vs-premium-api> Workable only as "paste your own API key", rate-limited and cached, hash lookups only (never upload files without a separate explicit action). Maestro cannot ship a shared key.
  - CIRCL hashlookup: free public API, best effort, data under CC BY (credit the service), commercial use allowed. It reports whether a hash is a *known* file (NSRL and other sets) with a trust score; "a match does not, by itself, indicate whether a file is malicious". An offline Bloom filter of SHA-1 values (about 700 MB, monthly) allows lookups that never leave the machine. [P] <https://www.circl.lu/services/hashlookup/>
  - MalwareBazaar (abuse.ch): hash lookup needs a personal Auth-Key from a free account. [S] <https://bazaar.abuse.ch/api/> Rate limits and data licence were not verified.
- **Microsoft Defender status and history.** The Defender WMI provider (`root\Microsoft\Windows\Defender`, classes `MSFT_MpComputerStatus`, `MSFT_MpThreatDetection`) and the matching cmdlets (`Get-MpComputerStatus`, `Get-MpThreatDetection`) expose protection state and past detections. [S] <https://learn.microsoft.com/en-us/previous-versions/windows/desktop/defender/windows-defender-wmiv2-apis-portal> [S] <https://learn.microsoft.com/en-us/powershell/module/defender/get-mpthreatdetection> Whether a standard user can read these is **not verified**; forum reports describe the classes returning nothing on some machines. The Windows Security Center API (`IWSCProductList`) for third-party antivirus status was not researched.
- **Ask the installed antivirus to scan content: AMSI.** `AmsiScanBuffer` lets any application submit a buffer to whichever antimalware product is registered. [S] <https://learn.microsoft.com/en-us/windows/win32/api/amsi/nf-amsi-amsiscanbuffer>
- **Cross-view checks in user mode** (see below).

### Achievable with admin or an opt-in component

- **ETW kernel telemetry.** Process, image-load, file and network providers through an elevated helper. Privilege rules as in section 1.
- **On-demand Defender scan of a file or folder:** `MpCmdRun.exe -Scan -ScanType 3 -File <path>` (type 3 is custom scan). [S] <https://learn.microsoft.com/en-us/defender-endpoint/command-line-arguments-microsoft-defender-antivirus>
- **Sysmon as an optional dependency.**
  - Sysmon is now an optional built-in feature of Windows 11 and Windows Server 2025, serviced through Windows Update. [S] <https://learn.microsoft.com/windows/security/operating-system-security/sysmon/overview> Secondary reports say it is enabled with `Dism /Online /Enable-Feature /FeatureName:Sysmon` followed by `sysmon -i`, and that the standalone version must be uninstalled first; the general-availability date (reported as February or March 2026) was not confirmed from a Microsoft source.
  - The standalone Sysinternals licence forbids redistribution: you may not "publish the software for others to copy" or "transfer the software ... to any third party". [P] <https://learn.microsoft.com/en-us/sysinternals/license-terms>
  - So: never bundle Sysmon. Detect it, offer to enable the built-in feature (admin), supply a configuration, and read its event log channel.

### Needs a signed kernel driver, or more

- **Driver signing today.**
  - Submitting a driver for attestation signing or HLK certification requires a Hardware Dev Center account with at least one EV code-signing certificate. Attestation-signed drivers are not "Windows Certified". [S] <https://learn.microsoft.com/en-us/windows-hardware/drivers/dashboard/code-signing-reqs>
  - A Microsoft Q&A answer from April 2026 says Azure Trusted Signing cannot replace the EV certificate for registration (community answer, not official). [S] <https://learn.microsoft.com/en-us/answers/questions/5866910/hardware-program-verification-using-azures-trusted>
  - From the April 2026 update, Windows 11 24H2/25H2/26H1 and Server 2025 stop trusting drivers signed under the old cross-signed root programme: first in evaluation (audit) mode, then enforced once a machine has run 250 hours and several restarts with no violations. Default-trusted drivers are those that "the Windows Hardware Compatibility Program (WHCP) have passed and been signed"; the post says drivers for the Windows ecosystem "must be WHCP certified and signed through the Microsoft HDC portal". [P] <https://techcommunity.microsoft.com/blog/windows-itpro-blog/advancing-windows-driver-security-removing-trust-for-the-cross-signed-driver-pro/4504818>
  - That post does not mention attestation signing, so whether attestation-signed drivers stay default-trusted under the new policy is **not verified**. EV certificate prices and whether an individual (not a company) can register were not researched.
  - Practical reading: a hobby open-source project cannot realistically ship its own kernel driver. Users cannot build and load it themselves without turning on test signing, which weakens the machine.
- **The Threat-Intelligence ETW provider** (`Microsoft-Windows-Threat-Intelligence`: memory injection, remote thread and similar events) only delivers to processes running as Protected Process Light at anti-malware level. [S] <https://www.elastic.co/security-labs/doubling-down-etw-callstacks> [S] <https://specterops.io/blog/2023/03/15/uncovering-windows-events/>
  - Microsoft's requirement for running a service as anti-malware PPL: the vendor "must have an ELAM driver installed", with the service's signing certificates embedded in that driver. Protected processes cannot have a UI. [P] <https://learn.microsoft.com/en-us/windows/win32/services/protecting-anti-malware-services->
  - Whether ELAM signing formally requires Microsoft Virus Initiative membership was not confirmed in a Microsoft page read this session. Either way it is out of reach for this project.
  - Published research describes getting these events without PPL; those are bypasses and not something to ship.

### Cross-view techniques (and their ceiling)

The idea: list the same thing two independent ways and flag disagreements. Legitimate software rarely hides, so false positives are low. [S] <https://microsoft.com/en-us/research/wp-content/uploads/2016/02/tr-2005-25.pdf>

User-mode views Maestro can compare:

- Process list from the normal snapshot APIs versus brute-force opening every possible PID (the classic Blacklight method). [S] <https://www.blackhat.com/presentations/bh-usa-06/BH-US-06-Silberman.pdf>
- Process list versus PIDs that own TCP/UDP endpoints, windows, or services.
- Process start/stop events from ETW or Sysmon versus the current list (a process that started, never exited, and is not listed).
- Connections in the IP Helper table versus flows actually seen on the wire by pktmon.
- Files listed through the directory API versus a raw read of the NTFS master file table (admin).

Ceiling: these catch user-mode hooking and sloppy hiding. A kernel rootkit can alter the structures every one of these views reads, or subvert the detector itself; surveys rate high-level versus low-level comparison as "vulnerable against modern kernel-level rootkit". [S] <https://arxiv.org/pdf/2304.00473> A trustworthy view needs to come from outside the running system (offline scan, hypervisor, another machine on the network).

### Remains impossible

- Guaranteeing that nothing is hidden. Any tool running above a compromised kernel sees what the kernel lets it see.
- Receiving Threat-Intelligence ETW events, or self-protecting against an administrator-level attacker, without being a Microsoft-recognised anti-malware vendor.

### Recommended approach for Maestro

| Step | What | Privilege | Earliest phase |
|---|---|---|---|
| a | Signature and catalog verification for every running image; flag unsigned, invalid, or mismatched | none | 1 |
| b | Show Defender state (on/off, definitions age, recent detections) and whether another antivirus is registered; say plainly "Maestro is not your antivirus" | none or admin (to verify) | 1 |
| c | User-mode cross-view checks, reported as "inconsistency found", not "malware found" | none | 1 |
| d | Opt-in hash lookups: CIRCL (online or offline filter) by default, VirusTotal with the user's own key | none | 1 for read-only risk display |
| e | "Scan this file with Defender" action via MpCmdRun or AMSI | admin for MpCmdRun | 2 |
| f | Optional Sysmon integration: detect, offer to enable the built-in feature, consume events | admin | 2 |
| g | Show platform protections Maestro depends on (Secure Boot, memory integrity, vulnerable-driver blocklist) and recommend an offline scan when cross-view checks disagree. How to read each setting was not researched. | none | 4 |
| h | Own kernel driver | n/a | Not planned |

---

## 4. Hardware without a controller

### Summary

The owner's assumption that a mappable power route exists is wrong for desktop PCs: there is no interface through which a PC reports which rail or connector feeds which component, and the power supply's rails cannot be switched individually from software. A device can be controlled only if it sits behind something that accepts commands: a USB or SMBus lighting controller, an embedded controller or Super I/O chip for fans, a USB hub that implements port power switching, or the operating system's own device enable/disable. An LED wired to a fan's power line or a Molex/SATA plug has no such thing in front of it.

### Achievable now without special privileges

- **Windows Dynamic Lighting (HID LampArray).** Devices that implement the open HID LampArray standard, in firmware or through a virtual HID driver, can be controlled through `Windows.Devices.Lights.LampArray` with no vendor software and no kernel driver. Foreground control since Windows 10, background ("ambient") control on Windows 11; ambient apps declare the `com.microsoft.windows.lighting` app extension, and the user ranks apps in Settings. [S] <https://learn.microsoft.com/windows/uwp/devices-sensors/lighting-dynamic-lamparray> [S] <https://learn.microsoft.com/windows-hardware/design/component-guidelines/dynamic-lighting-devices>
  - Coverage is mostly keyboards, mice and "other illuminated accessories". [P] <https://support.microsoft.com/en-us/windows/control-dynamic-lighting-devices-in-windows-8e8f22e3-e820-476c-8f9d-9ffc7b6ffcd2> How many case, fan or strip controllers and motherboards implement LampArray was not verified; expect few.
  - The ambient app extension implies a packaged app identity; whether an unpackaged desktop app can hold background control was not checked.
- **OpenRGB through its SDK server**, for USB HID devices that need no special access. See next section for the rest.
- **GPU power draw** from the vendor libraries (for example NVIDIA's NVML reports board power). Not verified this session.
- **Smart plug telemetry over the local network.** Shelly Gen2+ plugs expose a local JSON-RPC HTTP API with power and energy readings and no cloud dependency (for example `/rpc/Switch.GetStatus` or `/rpc/PM1.GetStatus`). [S] <https://shelly-api-docs.shelly.cloud/gen2/Devices/Gen3/ShellyPlugPMG3> Tasmota-flashed plugs offer the same kind of local API (not verified this session).

### Achievable with admin or an opt-in component

- **OpenRGB** for motherboard, RAM, GPU, cooler, strip and peripheral lighting.
  - Licence: GPL version 2. [P] <https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/LICENSE> ("or later" not checked.) Maestro must not link or copy its code. Talking to a separately installed OpenRGB over its network SDK keeps the two programs separate, which is the usual way MIT projects use it; Maestro should not bundle the installer without a licence review.
  - SDK: a "network interface for controlling supported RGB devices from other software"; TCP, default port 6742; third-party clients exist for Python, Node.js and Dart. [P] <https://gitlab.com/CalcProgrammer1/OpenRGB/-/raw/master/README.md> [S] <https://openrgb-python.readthedocs.io/en/stable/pages/api.html> Client library licences and whether the server listens beyond localhost by default were not checked.
  - Risk: protocols are reverse-engineered, and the README warns hardware has been "damaged/bricked" in the past.
  - Drivers: the README lists **PawnIO** as a Windows dependency. [P] same README. The previous driver, WinRing0, has been flagged by Microsoft Defender since March 2025 as a vulnerable driver (CVE-2020-14979). [S] <https://support.microsoft.com/windows/security/threat-malware-protection/microsoft-defender-antivirus-alert-vulnerabledriver-winnt-winring0> Which OpenRGB release completed the switch was not confirmed.
  - The supported-device list and its size were not read.
- **LibreHardwareMonitor** for temperatures, fan speeds, voltages, load and CPU package power, plus fan control on supported Super I/O and embedded-controller chips.
  - Licence: MPL 2.0, published as the `LibreHardwareMonitorLib` NuGet package. MPL is file-level copyleft, so an MIT app can use the unmodified library and keep its own licence. "Some sensors require administrator privileges." [P] <https://github.com/LibreHardwareMonitor/LibreHardwareMonitor>
  - Fan control exists per chip: release notes list control being added or restored for specific IT87xx and NCT66xx controllers. Latest release seen: v0.9.6 (14 February, year not shown, presumably 2026), which updates PawnIO modules to 2.2. [P] <https://github.com/LibreHardwareMonitor/LibreHardwareMonitor/releases>
  - It moved from WinRing0 to PawnIO; downstream projects report PawnIO must be installed separately or CPU and motherboard sensors disappear. [S] <https://newreleases.io/project/github/nickbabcock/OhmGraphite/release/v0.36.0>
- **PawnIO** is "a scriptable universal kernel driver"; the official build is digitally signed and loads only modules from its official repository. [P] <https://pawnio.eu/> Its open-source licence name, who signed it, and redistribution terms were **not verified**; there are secondhand reports of game anti-cheat objecting to it. Maestro should ask the user to install it and never ship it silently.
- **Device enable/disable.** `CM_Disable_DevNode` (with `CM_DISABLE_PERSIST` on Windows 10+ to survive a reboot) or the SetupAPI route (`SetupDiCallClassInstaller` with `DIF_PROPERTYCHANGE` / `DICS_DISABLE`), which is what Device Manager does. [S] <https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_disable_devnode> [S] <https://learn.microsoft.com/en-us/windows-hardware/drivers/install/dif-propertychange> The pages seen do not state the privilege; in practice it needs elevation (not verified from the docs). Disabling a device stops its driver and usually drops it to a low-power state, but does not promise that power is cut: a disabled USB light may stay lit.
- **USB port power.**
  - The USB standard lets a hub switch power per port, but "not many USB hubs correctly support per-port power switching", and some that accept the command still do not cut VBUS. The uhubctl compatibility table has roughly 120 entries. [P] <https://github.com/mvp/uhubctl>
  - On Windows this is worse: uhubctl (GPL-2.0) compiles but "USB power switching does not work on Windows", because the WinUSB driver does not pass the needed hub requests. [P] same page.
  - Windows does offer `IOCTL_USB_HUB_CYCLE_PORT`, a user-mode request to the hub that power-cycles a port and re-enumerates the device. It is a restart, not a sustained off. [S] <https://learn.microsoft.com/en-ca/windows-hardware/drivers/ddi/usbioctl/ni-usbioctl-ioctl_usb_hub_cycle_port>
  - No documented way to hold a port off from user mode on Windows was found. A custom hub filter driver could do it on capable hubs (see driver signing in section 3).
- **PSU telemetry.** liquidctl supports Corsair HXi (HX750i to HX1500i, ATX 3.1 HX1200i) and RMi (RM650i to RM1000i) power supplies and partially NZXT E-series, on Windows, over USB HID. It is GPL-3.0 and needs Python 3.10+, so it can only be an optional external tool that Maestro runs as a separate process. [P] <https://github.com/liquidctl/liquidctl> Whether it conflicts with Corsair iCUE running at the same time, and whether the iCUE SDK exposes PSU power, were not verified.
- **CPU energy (RAPL).** Reading the RAPL registers directly needs a kernel driver (this is what LibreHardwareMonitor does through PawnIO). Windows also has an Energy Meter Interface: drivers can expose energy counters that user mode reads with `IOCTL_EMI_GET_MEASUREMENT` in picowatt-hours. [S] <https://learn.microsoft.com/windows-hardware/drivers/powermeter/energy-meter-interface> Whether ordinary desktop CPUs expose RAPL through EMI out of the box, and whether a standard user can open the device, is **not verified**; test this, since it would give CPU power with no third-party driver.

### Needs a kernel driver or hardware

- Sustained per-port USB power off on Windows (driver, and a hub that supports it).
- Any sensor or fan chip reached through port I/O, MSRs or SMBus (PawnIO or equivalent).
- Whole-system power: a PSU with telemetry or a metering smart plug. Software sensors only cover CPU package and GPU; the rest is an estimate.

### Physically impossible from software

- Turning off an LED that is wired to a power rail with nothing switchable in between: LEDs fed from a fan's 12 V line, strips on Molex or SATA power, lighting hubs whose only control is a button on the case.
- Mapping internal power routes. Nothing in a desktop reports which connector feeds what, and ATX rails are common to everything on them.
- Cutting power to a single internal component. Software can ask a device to sleep or be disabled; only a controller, a switched port, or the PSU can remove power.

### Best hardware-side alternatives (for the owner's case LEDs)

1. **Check for an existing controller first.** If the LEDs plug into a motherboard RGB/ARGB header, or into a case hub that has a cable to a motherboard header or USB header, they are probably controllable through OpenRGB or the board's own utility. Many boards also have a firmware (BIOS/UEFI) setting to turn onboard lighting off, including when the PC is asleep or shut down.
2. **Move them onto an addressable controller.** If the case hub is button-only, replug the strips or fans into the motherboard's ARGB header (matching 5 V 3-pin or 12 V 4-pin: they are not interchangeable) or into a USB lighting controller that OpenRGB supports.
3. **Use a switched or PWM header.** Single-colour LEDs that only need power can be fed from a spare fan header and turned off by setting that header to 0% through fan control, if the board allows a full stop and the current is within the header's rating (commonly about 1 A; check the board manual).
4. **Inline physical switch** on the LED power lead: the only option when nothing is addressable.
5. **Smart plug:** only for lighting with its own external power supply. A smart plug on the PC itself switches the whole machine, though it is the best source of true wall-power readings.

### Recommended approach for Maestro

| Step | What | Privilege | Earliest phase |
|---|---|---|---|
| a | Device inventory with driver, status and a "controllable through: LampArray / OpenRGB / LHM / none found" label | none | 1 |
| b | Sensors read-only through LibreHardwareMonitorLib; show which readings need the PawnIO driver and offer the link, never a silent install | none, then admin + driver | 1 |
| c | GPU and CPU power readings; label system total as an estimate unless a PSU or plug supplies it | varies | 1 |
| d | Lighting through Dynamic Lighting first (no driver), then OpenRGB via SDK if the user has it installed | none / user-installed | 2 |
| e | Fan control through LibreHardwareMonitorLib with a safe minimum and automatic restore on exit | admin + driver | 2 |
| f | Device enable/disable with undo; USB "restart port"; state plainly that this does not guarantee power is cut | admin | 2 |
| g | Optional adapters: Shelly/Tasmota plug for wall power, liquidctl for Corsair PSU telemetry | none (network) / external tool | 4 |
| h | For anything with no control path, show the "hardware-side alternatives" list above instead of a disabled button | none | 1 |
| i | Per-port USB power off on Windows | driver | Not planned; revisit on Linux in phase 4, where uhubctl works |

Change to consider in plan.md (not made here): the "Power: per-port USB power" bullet promises something Windows does not expose. Reword to "restart a USB port, and disable devices" for Windows.

---

## Could not verify

Network contents:
- Current behaviour of the WinINet-Capture and WebIO ETW providers on Windows 11 24H2/25H2 (sources are 2017 to 2021).
- Exact privilege needed for the DNS-Client and Kernel-Network ETW providers and for pktmon (docs read did not state it).
- Whether Windows-native DoH lookups still appear in DNS-Client ETW events.
- Whether current Chrome requires Secure DNS for ECH; how widely servers enable ECH.
- Whether the in-box curl.exe honours `SSLKEYLOGFILE`; whether OpenSSL 3.4+ needs a build-time option for it.
- mitmproxy's licence and WinDivert's antivirus/anti-cheat reputation (stated from memory).
- Scope of the WFP user-mode subscribe functions.

Location:
- Licences of the bgp.tools anycast list (none stated) and the Twente census data.
- Terms and rate limits for RIPE, APNIC, LACNIC, AFRINIC and for ARIN RDAP specifically.
- Provider-published IP range lists and the Tor exit list (not fetched).
- Any independent accuracy figure for DB-IP Lite or IPinfo Lite.
- That `IcmpSendEcho2` needs no elevation (not stated on the page).

Security:
- Whether a standard user can read the Defender WMI classes; the Windows Security Center API.
- Whether attestation-signed drivers remain default-trusted under the April 2026 kernel trust policy; EV certificate cost; whether individuals can register for the Hardware Dev Center.
- Whether ELAM/anti-malware PPL formally requires Microsoft Virus Initiative membership.
- General-availability date of built-in Sysmon, and its exact enable steps (secondary sources only).
- MalwareBazaar rate limits and data licence.
- How to read Secure Boot, memory integrity and driver blocklist state.

Hardware:
- OpenRGB: "GPL-2.0-only" versus "or later", current release, supported-device count, default bind address of the SDK server, client library licences, which release replaced WinRing0 with PawnIO.
- PawnIO: licence name, signer, redistribution terms, anti-cheat reports.
- How many non-peripheral devices implement HID LampArray; whether an unpackaged app can be an ambient lighting controller.
- Privilege needed for `CM_Disable_DevNode` / `DIF_PROPERTYCHANGE`.
- Whether desktop CPUs expose RAPL through the Windows Energy Meter Interface without a third-party driver.
- NVML power readings, Tasmota's API, Corsair iCUE SDK coverage and liquidctl/iCUE coexistence.
- The fan-header current figure in alternative 3 is a typical value from memory; check each board's manual.

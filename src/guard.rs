//! Guard rails for changes to Maestro itself.
//!
//! Maestro runs with the user's rights, sits beside their sign-ins and can
//! pause, end and block programs. A change that quietly starts a new program,
//! opens a new connection, reads a credential file or pulls in new code is
//! how a user's machine or account would be taken over. So each of those
//! things is written down here as a list, and these tests fail when the code
//! no longer matches its list.
//!
//! A failing test is not a ban. It means: the change touches something that
//! matters, so say why in the pull request, update the list in the same
//! change, and a maintainer who owns this file must approve it. See
//! SECURITY.md.
//!
//! This file has no code that ships. It is only tests.

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::path::{Path, PathBuf};

    /// Every place a program is started: the file, and what is passed to
    /// `Command::new`. A name in quotes is a fixed program. A bare word is a
    /// variable, and the note says what it can hold.
    const STARTS: &[(&str, &str, &str)] = &[
        ("control.rs", "\"powershell\"", "firewall and adapter scripts, behind the system's own permission prompt"),
        ("control.rs", "\"/usr/bin/osascript\"", "the macOS permission prompt for adapter actions"),
        ("control.rs", "\"pkexec\"", "the Linux permission prompt for adapter actions"),
        ("environment.rs", "name", "only ever \"git\" or \"wsl.exe\", from the two callers in that file"),
        ("live.rs", "\"claude\"", "the unmodified Claude Code command, for a session the user starts"),
        ("safe.rs", "\"git\"", "test code only"),
        ("services.rs", "program", "a tool found on the path, asked only for its version or sign-in state"),
        ("services.rs", "\"cmd\"", "opens a terminal for the tool's own sign-in, on the user's click"),
        ("services.rs", "\"osascript\"", "the same on macOS"),
        ("services.rs", "\"x-terminal-emulator\"", "the same on Linux"),
    ];

    /// Files that may hold `unsafe` code: the hand-written calls into the
    /// operating system.
    const UNSAFE: &[&str] = &["control.rs", "gpu.rs", "main.rs", "netmap.rs", "stale.rs", "telemetry.rs", "winproc.rs"];

    /// Every use of the network, by file and the call that makes it.
    const NETWORK: &[(&str, &str, &str)] = &[
        ("netmap.rs", "UdpSocket::bind", "bound to this computer only, to start Windows networking; nothing is sent"),
        ("netmap.rs", "getnameinfo(", "asks the system's own DNS server for the name behind an address"),
        ("receiver.rs", "tiny_http::Server::http", "listens on 127.0.0.1 only, for usage reports from agents"),
        ("services.rs", "TcpStream::connect_timeout", "opens Ollama's port on this computer to see if it is running"),
    ];

    /// Calls that open a connection or a listening port. Anything matching
    /// one of these must be on the list above.
    const NETWORK_CALLS: &[&str] = &[
        "TcpStream::connect",
        "TcpListener::bind",
        "UdpSocket::bind",
        "tiny_http::Server::http",
        "getnameinfo(",
        "getaddrinfo(",
        "ToSocketAddrs",
    ];

    /// The crates Maestro is built from. Each one runs with the user's rights.
    const CRATES: &[&str] = &["eframe", "image", "netstat2", "serde", "serde_json", "sysinfo", "tiny_http"];

    /// Words that name files holding sign-ins. Maestro points tools at their
    /// folders and never opens what is inside, so none of these belongs in
    /// the code.
    const SECRET_FILES: &[&str] = &[
        ".credentials",
        "credentials.json",
        "id_rsa",
        "id_ed25519",
        "Login Data",
        "Cookies",
        "keychain",
        "auth.json",
        ".netrc",
        ".npmrc",
        "wallet",
    ];

    fn root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
    }

    /// Every source file as (name relative to `src`, text). This file is
    /// left out, because it has to name the things it forbids.
    fn sources() -> Vec<(String, String)> {
        fn walk(dir: &Path, base: &Path, out: &mut Vec<(String, String)>) {
            for entry in std::fs::read_dir(dir).expect("src can be listed").flatten() {
                let path = entry.path();
                if path.is_dir() {
                    walk(&path, base, out);
                } else if path.extension().is_some_and(|e| e == "rs") {
                    let name = path.strip_prefix(base).unwrap().to_string_lossy().replace('\\', "/");
                    if name != "guard.rs" {
                        out.push((name, std::fs::read_to_string(&path).expect("source is text")));
                    }
                }
            }
        }
        let src = root().join("src");
        let mut out = Vec::new();
        walk(&src, &src, &mut out);
        out.sort();
        out
    }

    /// The text of a file without its `//` comments, so a word in a comment
    /// neither trips a check nor hides from one.
    fn code(text: &str) -> String {
        text.lines()
            .map(|line| match line.find("//") {
                Some(at) if !line[..at].contains('"') => &line[..at],
                _ => line,
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    const HOW: &str = "\nThis is a guard rail, not a ban. Say why the change needs it in the pull request, \
                       update the list in src/guard.rs in the same change, and ask an owner of that file to review. \
                       See SECURITY.md.";

    #[test]
    fn every_place_a_program_is_started_is_on_the_list() {
        let mut found = BTreeSet::new();
        for (name, text) in sources() {
            let text = code(&text);
            let mut rest = text.as_str();
            while let Some(at) = rest.find("Command::new(") {
                let after = &rest[at + "Command::new(".len()..];
                let end = after.find(')').expect("the call is closed");
                found.insert((name.clone(), after[..end].trim().to_string()));
                rest = &after[end..];
            }
        }
        let listed: BTreeSet<(String, String)> =
            STARTS.iter().map(|(file, what, _)| (file.to_string(), what.to_string())).collect();
        let added: Vec<_> = found.difference(&listed).collect();
        let gone: Vec<_> = listed.difference(&found).collect();
        assert!(added.is_empty(), "New places start a program: {added:?}.{HOW}");
        assert!(gone.is_empty(), "These are listed but no longer in the code; take them off the list: {gone:?}");
    }

    /// Words that hand a line of text to a shell or script runner.
    const SHELL_WORDS: &[&str] = &[
        "\"sh\"", "\"/bin/sh\"", "\"bash\"", "\"zsh\"", "\"-lc\"", "\"-Command\"", "\"-EncodedCommand\"",
        "'-EncodedCommand'", "\"/c\"", "\"/k\"", "\"-e\"", "\"eval\"",
    ];

    /// Every place a line of text is run as a command, and what the text can be.
    const SHELL_LINES: &[(&str, &str, &str)] = &[
        ("control.rs", "\"-Command\"", "a fixed wrapper that raises the Windows permission prompt"),
        ("control.rs", "'-EncodedCommand'", "the firewall or adapter script, encoded so no quote in it can break out"),
        ("control.rs", "\"-e\"", "the macOS permission prompt; the script inside is escaped by one function"),
        ("control.rs", "\"/bin/sh\"", "the same script behind the Linux permission prompt"),
        ("environment.rs", "\"bash\"", "asks a WSL distro which of a fixed list of tools it has"),
        ("environment.rs", "\"-lc\"", "the same call"),
        ("services.rs", "\"/c\"", "opens a terminal for a sign-in command that is fixed in the code"),
        ("services.rs", "\"/k\"", "the same call"),
        ("services.rs", "\"-e\"", "the same on Linux; on macOS the line also carries the sign-in folder, quoted by mac_sign_in_script"),
    ];

    #[test]
    fn every_line_of_text_run_as_a_command_is_on_the_list() {
        // A command line built from text is how one stray quote becomes
        // someone else's command. Text that came from outside the program,
        // such as a folder name, a process name or anything an agent wrote,
        // must never be put into one.
        let mut found = BTreeSet::new();
        for (name, text) in sources() {
            let text = code(&text);
            for word in SHELL_WORDS {
                if text.contains(word) {
                    found.insert((name.clone(), word.to_string()));
                }
            }
        }
        let listed: BTreeSet<(String, String)> =
            SHELL_LINES.iter().map(|(file, word, _)| (file.to_string(), word.to_string())).collect();
        let added: Vec<_> = found.difference(&listed).collect();
        let gone: Vec<_> = listed.difference(&found).collect();
        assert!(added.is_empty(), "New places run a line of text as a command: {added:?}.{HOW}");
        assert!(gone.is_empty(), "These are listed but no longer in the code; take them off the list: {gone:?}");
    }

    #[test]
    fn unsafe_code_stays_in_the_files_that_talk_to_the_system() {
        for (name, text) in sources() {
            let has = code(&text).contains("unsafe");
            assert!(
                !has || UNSAFE.contains(&name.as_str()),
                "{name} now has unsafe code and is not on the list.{HOW}"
            );
        }
    }

    #[test]
    fn every_use_of_the_network_is_on_the_list() {
        let mut found = BTreeSet::new();
        for (name, text) in sources() {
            let text = code(&text);
            for call in NETWORK_CALLS {
                if text.contains(call) {
                    found.insert((name.clone(), call.to_string()));
                }
            }
        }
        for (file, call) in &found {
            let listed = NETWORK.iter().any(|(f, c, _)| f == file && c.starts_with(call.as_str()));
            assert!(listed, "{file} now uses the network through {call}.{HOW}");
        }
        for (file, call, _) in NETWORK {
            let still = found.iter().any(|(f, c)| f == file && call.starts_with(c.as_str()));
            assert!(still, "{file} is listed for {call} but no longer uses it; take it off the list.");
        }
    }

    #[test]
    fn nothing_listens_beyond_this_computer() {
        for (name, text) in sources() {
            let text = code(&text);
            for open in ["0.0.0.0", "[::]", "Ipv4Addr::UNSPECIFIED", "Ipv6Addr::UNSPECIFIED"] {
                // netmap.rs reads these as other programs' addresses; it binds nothing to them.
                let reads_only = name == "netmap.rs" && open == "0.0.0.0";
                assert!(
                    reads_only || !text.contains(open),
                    "{name} mentions {open}. Maestro must never listen where other computers can reach it.{HOW}"
                );
            }
        }
    }

    #[test]
    fn no_code_names_a_file_that_holds_a_sign_in() {
        for (name, text) in sources() {
            let text = code(&text).to_lowercase();
            for secret in SECRET_FILES {
                assert!(
                    !text.contains(&secret.to_lowercase()),
                    "{name} mentions \"{secret}\". Maestro never opens the files tools keep their sign-ins in.{HOW}"
                );
            }
        }
    }

    #[test]
    fn no_code_opens_a_link() {
        // The crate that opens links has a known flaw that the build cannot
        // yet take the fix for. It is passed in .cargo/audit.toml on the
        // ground that Maestro never opens a link. This holds that ground.
        for (name, text) in sources() {
            let text = code(&text);
            for call in ["hyperlink", "open_url", "OpenUrl"] {
                assert!(!text.contains(call), "{name} opens a link through {call}.{HOW}");
            }
        }
    }

    #[test]
    fn the_crates_are_the_listed_ones_and_come_from_the_public_registry() {
        let manifest = std::fs::read_to_string(root().join("Cargo.toml")).unwrap();
        let mut section = String::new();
        let mut found = BTreeSet::new();
        for line in manifest.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                section = line.to_string();
                assert!(
                    !matches!(line, "[build-dependencies]" | "[patch.crates-io]" | "[replace]"),
                    "Cargo.toml gained {line}, which changes what code is built in.{HOW}"
                );
            } else if section == "[dependencies]" && line.contains('=') {
                found.insert(line.split('=').next().unwrap().trim().to_string());
                assert!(
                    !line.contains("git =") && !line.contains("path ="),
                    "A crate is taken from somewhere other than the public registry: {line}.{HOW}"
                );
            } else if section == "[package]" {
                assert!(!line.starts_with("build"), "Cargo.toml names a build script: {line}.{HOW}");
            }
        }
        let listed: BTreeSet<String> = CRATES.iter().map(|c| c.to_string()).collect();
        assert_eq!(found, listed, "The crates in Cargo.toml are not the listed ones.{HOW}");
        assert!(!root().join("build.rs").exists(), "A build script runs on every contributor's machine.{HOW}");

        let lock = std::fs::read_to_string(root().join("Cargo.lock")).expect("Cargo.lock is committed");
        for line in lock.lines().filter(|l| l.starts_with("source = ")) {
            assert_eq!(
                line, "source = \"registry+https://github.com/rust-lang/crates.io-index\"",
                "Cargo.lock takes a crate from somewhere else.{HOW}"
            );
        }
    }

    #[test]
    fn workflows_are_pinned_and_hold_no_more_rights_than_they_need() {
        let dir = root().join(".github").join("workflows");
        let Ok(files) = std::fs::read_dir(&dir) else { return };
        for file in files.flatten() {
            let name = file.file_name().to_string_lossy().to_string();
            let text = std::fs::read_to_string(file.path()).unwrap();
            // Comment lines are dropped, so a workflow can warn against the trigger by name.
            let text: String =
                text.lines().filter(|l| !l.trim_start().starts_with('#')).collect::<Vec<_>>().join("
");
            // This trigger runs a stranger's pull request with the repository's secrets.
            assert!(!text.contains("pull_request_target"), "{name} uses pull_request_target.{HOW}");
            assert!(
                text.lines().any(|l| l.starts_with("permissions:")),
                "{name} does not say what rights it needs, so it gets the default ones.{HOW}"
            );
            assert!(!text.contains("write-all"), "{name} asks for every right.{HOW}");
            for line in text.lines().filter(|l| l.trim_start().trim_start_matches("- ").starts_with("uses:")) {
                let used = line.split("uses:").nth(1).unwrap().split('#').next().unwrap().trim();
                let pin = used.rsplit('@').next().unwrap_or("");
                let pinned = pin.len() == 40 && pin.chars().all(|c| c.is_ascii_hexdigit());
                assert!(
                    pinned,
                    "{name} uses {used}. An action must be pinned to a full commit, because a tag can be \
                     moved to different code later.{HOW}"
                );
            }
        }
    }
}

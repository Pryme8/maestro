# Install and first run

Maestro is one native program. It has no installer, no web view and no background service. Windows 11 is the system it is built and tested on first. It also builds and runs on Linux. The macOS code is written but has not been run on a Mac yet: see [Status](status.md) before relying on it there.

## Download

Downloads are on the [releases page](https://github.com/Pryme8/maestro/releases). The macOS app is experimental: it builds and passes its tests on GitHub's Mac, and nobody has opened it on a Mac yet.

| System | File | To run it |
|---|---|---|
| Windows 11, 64-bit | `maestro-<version>-windows-x64.zip` | Unpack anywhere and run `maestro.exe` |
| macOS, Apple Silicon and Intel | `maestro-<version>-macos-universal.zip` | Unpack and move `Maestro.app` to Applications |
| Linux, 64-bit | `maestro-<version>-linux-x64.tar.gz` | Unpack anywhere and run `./maestro` |

Each release lists SHA-256 checksums so you can check the file you downloaded.

### The downloads are not signed yet

- **Windows:** SmartScreen warns about an unrecognised program the first time. Choose "More info", then "Run anyway".
- **macOS:** the app is not signed with an Apple developer identity or notarised, so Gatekeeper refuses to open it on a double-click. Right-click `Maestro.app` and choose Open, then Open again. On recent macOS versions you instead allow it under System Settings, Privacy & Security, "Open Anyway". If macOS says the app is damaged, clear the download mark from a terminal: `xattr -dr com.apple.quarantine /Applications/Maestro.app`.

Signing takes a code-signing certificate on Windows and a paid Apple developer account on macOS. Neither is set up yet.

## Build from source

You need a [Rust toolchain](https://rustup.rs), version 1.84 or newer, and git.

```
git clone https://github.com/Pryme8/maestro.git
cd maestro
cargo run --release
```

The finished program is `target/release/maestro.exe` on Windows and `target/release/maestro` on macOS and Linux. It can be copied anywhere; it needs no other files.

- **macOS** also needs Apple's command line tools: `xcode-select --install`. Building this way gives a plain program you start from a terminal, not an app bundle. The steps that make `Maestro.app` are in [Releasing](releasing.md).
- **Linux** also needs a few development packages. On Ubuntu or Debian: `sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev`. To open the window it needs the EGL and xkbcommon-x11 libraries at run time (`libegl1` and `libxkbcommon-x11-0` on Ubuntu).

`cargo test` runs the unit tests.

## First run

Start Maestro and it opens on the [Base](base.md) page. It starts reading the machine straight away and changes nothing until you press a button.

- **No administrator rights are needed** to look at the machine or to pause, resume and end your own programs.
- **Some actions ask the system for permission each time.** On Windows: blocking a program on the network and switching a network adapter off. On macOS, switching an adapter shows the system password prompt. On Linux it asks through polkit.
- **Some programs cannot be read.** For programs that belong to the system or to another user, the operating system may not let Maestro read the file path or command line. Maestro says so where that happens. This is more common on macOS, which shows less about other users' and Apple's own processes to a program without administrator rights.
- **macOS may ask about Terminal.** The Sign in button on the [Live](live.md) page asks Terminal to run a command, and macOS asks you to allow that the first time.
- **Port 4318.** Maestro listens on port 4318 on this computer only, for usage reports from agents. If another telemetry collector already uses that port, Maestro tells you and carries on without the receiver.

## Using the command line

Every flag in [Command line](command-line.md) is given to the same program. From a download:

- Windows: `maestro.exe --agents`
- macOS: `/Applications/Maestro.app/Contents/MacOS/maestro --agents`
- Linux: `./maestro --agents`

From source, `cargo run --release -- --agents` does the same.

## Where it keeps its files

| System | Folder |
|---|---|
| Windows | `%LOCALAPPDATA%\Maestro` |
| macOS | `~/Library/Application Support/Maestro` |
| Linux | `~/.local/state/maestro` (or `$XDG_STATE_HOME/maestro`) |

Set the `MAESTRO_DATA_DIR` environment variable to another folder to try Maestro without touching your real files. What each file holds is listed in [Data and privacy](data-and-privacy.md).

## Remove it

Close Maestro, delete the program (or `Maestro.app`), and delete the data folder above. Closing Maestro resumes everything it paused.

Before deleting it, undo any network block or adapter switch from the action log, because those are changes to the operating system that stay in place without Maestro. If you already deleted it: on Windows, Maestro's firewall rules are in the group named "Maestro" in Windows Firewall, and an adapter can be switched back on in the system's network settings on any system.

The data folder also holds any [extra Claude Code sign-ins](live.md#more-than-one-account) you added, with their session history. Deleting it signs those accounts out.

Works made with the [managed environment](environment.md) leave a branch named `maestro/<name>` in your repository. Remove works with `--work-remove` before deleting the data folder, then delete those branches yourself if you do not want them.

# Security

Maestro runs with your rights. It can pause, end and block programs, it sits beside the sign-ins of your AI tools, and it starts agents that can change files. A flaw here, or a change made in bad faith, could cost someone their machine or their accounts. This page says how to report a problem, what Maestro promises, and how changes to the project are kept from breaking those promises.

## Reporting a problem

Do not open a public issue for a security problem. Use **Report a vulnerability** on the repository's [Security tab](https://github.com/Pryme8/maestro/security/advisories/new), which sends a private report to the maintainers.

Say what you found, how to make it happen, and which version or commit you used. We aim to reply within a week. Please give us time to fix it before you publish.

Maestro is early and has no released version yet. Fixes go to `main`.

## What Maestro promises

These are the rules the code keeps today. A change that breaks one is a security change, whatever else it does.

1. **It does not read sign-ins.** Maestro points a tool at the folder its sign-in lives in. It never opens the files a tool keeps there, and it never reads browser data, key stores or SSH keys.
2. **It does not keep keys.** A sign-in can name an environment variable that holds an API key. Maestro stores the name. The key is read when a session starts, handed to the tool, and never written down, logged or shown.
3. **It sends nothing off your computer.** Maestro makes no connection to any server of its own and has no telemetry. Its only uses of the network are: a listener on `127.0.0.1` for usage reports from agents on the same computer, a check of Ollama's port on the same computer, and asking your system's DNS for the name behind an address. The name lookups can be turned off.
4. **Nothing listens where other computers can reach it.**
5. **It starts only programs on a short, written list**, and does not build command lines out of text that came from outside the program. Folder names, program names and anything an agent wrote are passed as separate arguments or not at all. There is one exception: on macOS, the terminal that opens for a sign-in inherits nothing from Maestro, so the path of the folder Maestro made for that sign-in is written into the line it runs. The path is quoted for the shell and escaped for AppleScript by one function, `mac_sign_in_script`, and a key is never written into that line.
6. **Anything that needs administrator rights goes through your system's own permission prompt**, every time. Maestro never keeps raised rights.
7. **An agent is refused unless you allow it.** When a session asks to do something, no answer and a closed window both mean no.
8. **It builds only from the public crate registry**, from versions fixed in `Cargo.lock`, with no build script.

## Optimize

The Optimize page hands a report on the machine to an agent you are signed in to, which is the one time Maestro's own data leaves the computer, and it goes only when you click **Run the analysis**. The report is shown first. It holds hardware, firmware and driver versions, settings and program names, and lines naming a user, a computer or a serial number are taken out. The agent runs in an empty folder with every step that needs permission refused, so it can advise and nothing else. The rules it is given are in `RULES` in [src/optimize.rs](src/optimize.rs); a change to them changes what every user is advised to do, and should be reviewed as a security change.

## Setup steps

A setup guide can run a step for the user. It runs only lines from the fixed list `RUN_LINES` in [src/setup.rs](src/setup.rs), only after showing the line and getting a yes, and in a terminal the user can see. Two of those lines download and run a tool maker's own installer over https, which is the only place Maestro causes anything to be downloaded. A test fails if a guide offers a line that is not on the list, or if a line fetches from anywhere but the maker's site. Adding or changing a line is a security change.

## How changes are checked

Rules 1, 3, 4, 5 and 8 are checked by tests in [src/guard.rs](src/guard.rs), which run with `cargo test` and on every pull request. Each sensitive thing the code does is on a list in that file: every program it starts, every command line built from text, every use of the network, every file with `unsafe` code, every crate. The tests fail when the code and its list differ.

A failing guard test is not a ban. It means the change touches something that matters. To get it through:

1. Say in the pull request what is added and why it is needed.
2. Update the list in `src/guard.rs` in the same pull request, with a note saying what the new entry can and cannot do.
3. A maintainer listed in [.github/CODEOWNERS](.github/CODEOWNERS) has to approve it. That file makes their review required for `src/guard.rs`, the workflows, `Cargo.toml` and `Cargo.lock`.

The tests read the source as text, so they can be fooled by someone who sets out to fool them, for example by building a program's name out of pieces. They are there to make a risky change visible, and to catch honest mistakes. They do not replace reading the change. Reviewers should treat any of the following as a reason to look hard, even when every test passes:

- a change to `src/guard.rs`, the workflows or the lock file that the description does not mention
- text assembled at run time and then passed to `Command`, a socket or a file path
- anything encoded, compressed or downloaded
- a new crate, or a crate swapped for one with a similar name
- code that reads a folder under the user's home that Maestro did not make

## The build

- Pull requests run with a read-only token and no secrets. No workflow uses `pull_request_target`.
- Every action is pinned to a full commit, so a tag cannot be moved to different code later.
- Each pull request is checked for crates with known vulnerabilities, and the lock file is audited on a schedule.
- A release is built by the workflow from a tag, with checksums, and stays a draft until a maintainer publishes it by hand.

## What is not covered

- The agents themselves. Maestro starts the tools you have installed, unmodified. What an agent does inside a session is limited by what you allow it and by the safe copy it works in, not by this project's code. See [docs/live.md](docs/live.md).
- A program already running with your rights. It can do what you can do, including to Maestro's own files.
- The downloads are not signed yet. Until they are, check the checksum of a download against the one on the release page.

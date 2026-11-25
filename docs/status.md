# Status

Maestro is early. This page says plainly what works on each system and what has only been written. "Written" means the code exists; it does not mean anyone has seen it work.

## Windows, macOS and Linux

| | Windows 11 | macOS | Linux |
|---|---|---|---|
| Builds and passes its tests | yes | not tried | yes, on Ubuntu 22.04 under WSL |
| Reads the machine | yes | written, never run | yes, run from the command line |
| Pause, resume, end, and the locks | yes | written, never run | yes, run against a throwaway process |
| Finds agents | yes | written, never run | yes, one agent tried |
| The window | yes | not tried | not seen: the test machine has no EGL or xkbcommon-x11 library, so it could not open |
| Not-responding check | written, not seen working | not built | not built |
| Switch an adapter off | written, not run | written, never run (system password prompt) | written, not run (asks through polkit) |
| Block one program on the network | written, not run | not built | not built |
| Follow and direct Claude Code | yes, see below | written, never run | not tried |
| Managed environment | yes, with or without WSL | not tried | yes, on the machine itself |

### macOS

The macOS code was written from documentation. Nothing in it has been compiled or run, because the project has had no Mac to try it on. The release build compiles it on a Mac for the first time, and is set up so that a failure there does not hold back the Windows and Linux downloads. Until someone has run it on a Mac, treat a macOS download as untested.

Known ahead of time:

- The download is not signed or notarised, so macOS will not open it on a double-click. See [Install](install.md#the-downloads-are-not-signed-yet).
- macOS shows less about processes that are not yours to a program without administrator rights, so some file paths and sockets may be missing.
- Blocking one program on the network takes a Network Extension content filter, which has to be signed with an Apple developer identity. It is not built.
- A program started from Finder or the Dock does not get your terminal's `PATH`, which may hide installed agent tools from the Services list.
- **Sign in… for a named sign-in on macOS is untested.** Terminal starts a shell that inherits nothing from Maestro, so the sign-in's folder is written into the command it runs. That has a unit test and has never been run on a Mac. Whether macOS keeps two sign-ins apart at all has not been checked either, because Claude Code stores its sign-in in the system keychain there. To be sure on a Mac, check that the terminal's command starts with `CLAUDE_CONFIG_DIR=` and the folder `--logins` prints.

If you have a Mac, a report of what happens when you build and run it is one of the most useful things you can send. See [Contributing](../CONTRIBUTING.md).

### Linux

Blocking one program on Linux takes a control group for the program and an nftables rule that matches it. It is not built. The window has not been seen on Linux.

## What has been tested

**Controls**

- Pause, resume, end, the locks and resume-on-close were run against a throwaway process.
- The planning and script-building logic has unit tests.
- The network block and the adapter switch need the system's permission prompt and have not been run.
- The buttons have been seen on screen but not clicked through by a person. The Base page, with the profile row, was seen on screen with real data.
- Profiles and family actions have unit tests for what they would act on and have not been clicked through.

**Stale programs**

- The cold count has unit tests.
- The not-responding check and the flags on the tiles have not been seen working.

**Network**

- The page was seen on screen once with real data: the connection tables drew with names and owners as intended. Its sorting has unit tests.
- The same data was printed with `--connections` on Windows, where 18 of 40 internet addresses came back with a name within twenty seconds.

**Agents**

- The recogniser was run on a machine with Claude Code, the Claude app, Copilot and Ollama. The other agents' rules are untested.
- The usage receiver was tested with a sample report, not yet with a real agent.

**Live**

- The page was seen on screen once with real data: the services list and the session chips drew as intended. The services list was then made more compact and Cursor's editor is now found as well as its agent; those changes and the map were then seen once in saved pictures, drawn as intended. What the map is built from has unit tests.
- `--services` was run on Windows, where it reported Claude Code as needing sign-in and Ollama as running, both correctly. The Sign in button has not been pressed.
- Reading session files was run from the command line against two real sessions on Windows.
- Three signed-in runs were driven from the command line on Windows against the real `claude` command. One ran a shell command and its call and result showed as they happened. One asked to write a file and was refused: no file was made, the reason reached the agent and it said it had not been allowed. One asked the same and was allowed: the file was written. Each cost between about $0.15 and $0.38 by the tool's own estimate.
- A run on `--model haiku` was driven for real and reported that model; it cost about $0.07 by the tool's own estimate.
- Named sign-ins were run from the command line on Windows in a scratch data folder: adding, refusing a name that would share a folder, listing with states, and starting runs. A run under a sign-in that had never been signed in came back as not logged in while Claude Code's own sign-in stayed signed in, which shows the two are kept apart. A run under a key sign-in whose variable was not set was refused before anything started. Signing a second account in, and a run under it, have not been done: that takes a second account. The sign-in list and picker on the Live page have not been seen on screen.
- A person drove one session from the window on Windows, on Haiku, in a scratch folder: start, allow a file edit, send a second message, refuse a shell command, further messages, pause and resume, and stop. The amber question box, the "needs you" chip, the map with its waiting ring, and the ended notice all showed as intended, and the session appeared once in the list. The file was edited when allowed and left alone when refused.
- Interrupt has not had a fair test with a signed-in session: the reply it was tried on finished within two seconds. The agent took a refusal with no reason for a permission setting; the wording was changed afterwards and that change has not been seen in a session.
- Safe copies have unit tests for preparing one, listing its changes and rolling it back. One real Haiku session was run from the command line on Windows with `--safe`: it edited a file and made a new one in the copy, the user's own checkout was untouched, both changes were listed, and the rollback restored the copy. The buttons for this on the page have not been clicked.

**Timeline**

- The timeline, events and log book were seen on screen with a few minutes of real data.
- The ledger has only been seen empty. Its sums are covered by a unit test.

**Router**

- The advisor and ledger are covered by unit tests and were run from the command line on Windows and Linux against a scratch folder.
- The Router page compiles but has not been seen on screen.
- The commands in the relay plan come from reading each vendor's documents and have not been run.

**Managed environment**

- Works, snapshots and rollback have unit tests and were run from the command line three ways against a scratch repository: with git on Windows, with git on Linux, and from Windows with git inside a WSL distro.
- The steps `--env-plan` prints have not been run, so whether a distro with drive mounting turned off still mounts single folders from its mount table is unconfirmed.

## Known problems

- When every processor core is busy, one sample can take two or three seconds instead of about 16 ms. Giving the sampling thread a higher priority is the likely fix and has not been tried.

## Not built yet

Running a relay, the isolated image, snapshots of the whole image, a page for the environment in the window, graphics load, how much each connection carries, locations for connections, signature checks, hardware controls, and stopping an agent automatically when its budget runs out. The [plan](plan.md) gives the order.

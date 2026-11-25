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
- Starting a run, sending it a second message and interrupting it were run against the real `claude` command. Its login had expired, so each request came back as a sign-in error, and no run that calls tools has been driven from Maestro yet.

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

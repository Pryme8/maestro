# Maestro

Maestro is a planned replacement for the resource manager and the everyday operating-system interfaces, built for a machine whose main job is running AI agents. It shows everything running on your PC, shuts down whatever the current task does not need, and gives you one visual, manageable place to direct the agents doing the work.

**Status: phases 1 and 2 are running in a first form.** The app is a native Windows program that reads the machine, draws it, and can now act on it: pause, resume and end programs, clear the machine for a task, block a program on the network and switch an adapter off, each with a way back. This repo is public from day one so the design, the doubts and the decisions are all in the open.

## The vision

Today's desktops are built around a person clicking through windows. Task Manager, Settings and the file explorer assume you are the one doing the work. When agents do most of it, the machine should be organised around them instead: its resources dedicated to the task in hand, its state visible at a glance, and its controls built for orchestrating agents, not for launching apps.

Maestro is that agent-centric environment. It runs on top of your existing operating system and takes over the interfaces you use to see and manage it.

## What it does

- **Monitors the whole system.** Processes, memory, CPU, GPU, disk and network, shown as a base you can read at a glance instead of a table of numbers.
- **Clears the machine for the task.** For each task it works out what needs to run, then suspends or stops everything else, with undo. Processes the operating system needs are never touched.
- **Manages what is running.** Every process, connection and adapter can be inspected and controlled from one interface, replacing the resource manager.
- **Follows every connection.** If a program is sending data somewhere, you see which file is responsible, where the data goes on a map, how risky it looks, and how to stop it, block it or remove it.
- **Controls the hardware it can reach.** Devices, case lighting, fans and port power, wherever the hardware exposes a control.
- **Treats agents as first-class.** It recognises AI agents, shows what each one is doing and what it costs, and presents them as crew you assign to work.
- **Routes and relays work between agents.** A ledger of every AI account, an advisor that suggests the cheapest route for a task, and a relay that hands work from one agent to the next.
- **Gives agents a managed environment.** A WSL2, VM or container image with the toolchain preinstalled, recorded paths and snapshots, so unattended runs cannot touch the rest of your machine.

The full plan, including what each hard part takes, is in [docs/plan.md](docs/plan.md).

## What works today

- **The base:** processor load per core, memory, disks and network, updated every second.
- **Everything running:** every process, grouped by program, with its processor share, memory, disk writes, the file on disk behind it and what started it.
- **Every socket:** each open network socket is tied to the process that owns it, with the remote address and port.
- **Four views of the same data:** tiles, a heat map sized by memory and coloured by load, a tree of what started what, and a map that joins programs to the programs they started.
- **A first sorting of processes:** critical to Windows, part of Windows, background service, or app. A process using a critical Windows name from outside the Windows folder is flagged. This is judged from names, file locations and parentage; signatures are not checked yet.

- **Controls, with a way back:**
  - **Pause and resume** a program. Pausing freezes it so it uses no processor, and nothing is lost.
  - **End** a program, after a second click to confirm.
  - **Clear the machine for a task.** Pick the program that matters and Maestro lists what could be paused, leaving the task, what started it, what it started and anything on your keep list running. You untick what you want left alone, then pause the rest in one go.
  - **Block a program on the network** with Windows Firewall rules, and **switch a network adapter off**. Windows asks for permission before either.
  - **An action log** along the bottom with an Undo button on everything that can be undone, and Resume all.
- **Guard rails:** processes critical to Windows and parts of Windows are locked and cannot be acted on. Everything Maestro pauses is written to disk, resumed when Maestro closes, and offered for resuming at the next start if Maestro crashed.

Not there yet: graphics load, hostnames and locations for connections, signature checks, saved task profiles, and hardware controls. See the plan for the order.

What has been tested: pause, resume, end, the locks and resume-on-close were run against a throwaway process, and the planning and script-building logic has unit tests. The network block and adapter switch need Windows' permission prompt and have not been run yet. The buttons have been seen on screen but not clicked through by a person.

## Build and run

Maestro is written in Rust with [egui](https://github.com/emilk/egui). It is a single native program with no web view. You need a [Rust toolchain](https://rustup.rs), version 1.84 or newer.

```
cargo run --release
```

Other ways to start it:

- `cargo run -- --view heat` opens on a chosen view: `tiles`, `heat`, `tree` or `mold`.
- `cargo run -- --dump` prints one sample of the machine as JSON and exits, without opening a window.
- `cargo run -- --screenshot shot.png` opens the window, saves one picture of it and exits.
- `cargo run --release -- --profile` times each part of a sample, to show what the monitor itself costs.
- `cargo run -- --pause 1234`, `--resume 1234` and `--end 1234` act on one process by number, under the same locks as the buttons.
- `cargo run -- --focus-plan code.exe` lists what clearing the machine for that program would pause. It changes nothing.
- `cargo test` runs the unit tests.

It is built and tested on Windows 11. Linux and macOS are planned for phase 4 and are untested.

## What it will not claim

- Shutting down unneeded processes frees memory, CPU and GPU for local work such as builds and local models. It does not make a cloud-hosted model answer faster.
- It shows who a program talks to, how much and when, but not the contents of encrypted traffic.
- It is not an antivirus, and it can only switch hardware that exposes a control.
- It cannot measure answer quality. "Best route" means cheapest within your rules.
- Subscription quota figures are estimates, because vendors do not expose them exactly.
- A relay for a vendor only gets built if that vendor's terms allow it.

## Get involved

Help is welcome at every layer: the monitor, the controls, the agent view and the router.

- Humans: see [CONTRIBUTING.md](CONTRIBUTING.md).
- AI agents: see [AGENTS.md](AGENTS.md). This project is meant to be a better place for you to work, and your view of what that takes is wanted.

## License

[MIT](LICENSE)

# Maestro

Maestro is a planned replacement for the resource manager and the everyday operating-system interfaces, built for a machine whose main job is running AI agents. It shows everything running on your PC, shuts down whatever the current task does not need, and gives you one visual, manageable place to direct the agents doing the work.

**Status: phases 1 to 4 are running in a first form, and phase 5 has its first part: the ledger and the advisor. The relay only prints its plan, and the managed environment is not built.** The app is a native program, built on Windows first and now also on Linux, that reads the machine, draws it, and can act on it: pause, resume and end programs, clear the machine for a task, block a program on the network and switch an adapter off, each with a way back. This repo is public from day one so the design, the doubts and the decisions are all in the open.

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
- **A first sorting of processes:** critical to the system, part of the system, background service, or app. A process using a critical system name from outside the system folders is flagged. This is judged from names, file locations and parentage; signatures are not checked yet.

- **Controls, with a way back:**
  - **Pause and resume** a program. Pausing freezes it so it uses no processor, and nothing is lost.
  - **End** a program, after a second click to confirm.
  - **Clear the machine for a task.** Pick the program that matters and Maestro lists what could be paused, leaving the task, what started it, what it started and anything on your keep list running. You untick what you want left alone, then pause the rest in one go.
  - **Block a program on the network** with Windows Firewall rules, and **switch a network adapter off**. Windows asks for permission before either.
  - **An action log** along the bottom with an Undo button on everything that can be undone, and Resume all.
- **Guard rails:** processes critical to Windows and parts of Windows are locked and cannot be acted on. Everything Maestro pauses is written to disk, resumed when Maestro closes, and offered for resuming at the next start if Maestro crashed.

- **Agents as crew:**
  - **Recognised by themselves.** Maestro finds Claude Code, the Claude app, Cursor, Codex, ChatGPT, Copilot, Gemini CLI, Aider, Windsurf, Ollama and LM Studio among the running processes, including tools that run inside Node or Python.
  - **Helpers counted in.** The shells, tools and scripts an agent starts are counted with it, so you see what the agent is really using.
  - **Usage reports.** Agents that can export OpenTelemetry metrics, such as Claude Code, can report tokens and cost to Maestro on port 4318. The app shows how to set that up. Maestro listens on this computer only and keeps only the token and cost figures.
  - **Budgets and energy.** Give an agent a daily budget in dollars and its energy bar shows what is left. Passing the budget puts a warning in the action log.
  - **Quests.** Name a piece of work, assign it to an agent, and see what the agent reported spending on it.

- **Depth over time**, on the Timeline page:
  - **Timeline.** The last hour, 6 hours or 24 hours, one line a minute: processor load, memory, download and upload, with a mark wherever something happened. Hover for that minute's figures. It keeps recording while the window is minimised.
  - **Events.** Every action, warning, agent starting or stopping, and achievement, newest first.
  - **Ledger.** What each agent reported spending today, over 7 days, over 30 days and in total, with a row for all agents together.
  - **Log book.** Thirteen achievements, such as keeping the processor calm for an hour or finishing ten quests, each with its progress.
  - All of this is kept in a file on this computer and sent nowhere.

- **A ledger and an advisor**, on the Router page and from the command line:
  - **Ledger.** You list your AI accounts: which tool, which model, how you rate the model (light, standard or heavy), and either its price per million tokens or your estimate of its subscription allowance over a rolling window.
  - **Advisor.** Give it a task's size in tokens and the rating it needs, and it proposes up to three routes: the cheapest single tool that fits, a single tool that leaves every allowance untouched, and a draft on a cheaper model followed by a review on a stronger one. Each shows what it adds to your bill and what is left of each allowance.
  - **Handoffs are costed.** A split counts the tokens for the handoff brief and for the second tool reading the work again. It is only proposed when it still saves money or you asked for a review, and small tasks are told to stay in one tool.
  - **Relay plan, as a dry run.** For any route it prints the command each step would run and the handoff brief the tools would pass. It starts nothing. A command is shown only where the vendor's own documents support it: see [docs/research/relay.md](docs/research/relay.md).
  - **Limits.** Maestro knows no vendor's prices or limits; every figure is yours. Usage is counted per tool, not per account or model, so two accounts on the same tool share one figure, and use from other devices is not seen. The advisor cannot judge answer quality.

Not there yet: running a relay, the managed environment (the WSL image, recorded paths and snapshots), graphics load, hostnames and locations for connections, signature checks, saved task profiles, hardware controls, and stopping an agent automatically when its budget runs out. See the plan for the order.

What has been tested: pause, resume, end, the locks and resume-on-close were run against a throwaway process, and the planning and script-building logic has unit tests. The network block and adapter switch need Windows' permission prompt and have not been run yet. The buttons have been seen on screen but not clicked through by a person. The agent recogniser was run on a machine with Claude Code, the Claude app, Copilot and Ollama; the other agents' rules are untested. The usage receiver was tested with a sample report, not yet with a real agent. The timeline, events and log book were seen on screen with a few minutes of real data; the ledger has only been seen empty, and its sums are covered by a unit test. The advisor and ledger are covered by unit tests and were run from the command line on Windows and Linux against a scratch folder; the Router page compiles but has not been seen on screen. The commands in the relay plan come from reading each vendor's documents and have not been run.

One known problem, there before phase 4 and not yet fixed: when every processor core is busy, one sample can take two or three seconds instead of about 16 ms. Giving the sampling thread a higher priority is the likely fix and has not been tried.

## Build and run

Maestro is written in Rust with [egui](https://github.com/emilk/egui). It is a single native program with no web view. You need a [Rust toolchain](https://rustup.rs), version 1.84 or newer.

```
cargo run --release
```

Other ways to start it:

- `cargo run -- --view heat` opens on a chosen view: `tiles`, `heat`, `tree` or `mold`.
- `cargo run -- --page timeline` opens on the Timeline page, and `--page router` on the Router page.
- `cargo run -- --ledger` lists your AI accounts with what is left of each allowance.
- `cargo run -- --account-add work claude-code opus heavy allowance 2000000 168` adds a subscription account with an estimated two million tokens a week. `... payperuse 3 15` adds one billed at $3 in and $15 out per million tokens. `--account-remove work` removes it. These example figures are made up.
- `cargo run -- --advise 300000 --tier heavy --review` proposes routes for a task of 300,000 tokens. Add `--relay-plan` to print what a relay would run. Nothing is started.
- Set `MAESTRO_DATA_DIR` to a folder to try any of this without touching your real files.
- `cargo run -- --dump` prints one sample of the machine as JSON and exits, without opening a window.
- `cargo run -- --screenshot shot.png` opens the window, saves one picture of it and exits.
- `cargo run --release -- --profile` times each part of a sample, to show what the monitor itself costs.
- `cargo run -- --pause 1234`, `--resume 1234` and `--end 1234` act on one process by number, under the same locks as the buttons.
- `cargo run -- --agents` lists the AI agents found running.
- `cargo run -- --receive` runs only the usage receiver and prints totals as reports arrive.
- `cargo run -- --focus-plan code.exe` lists what clearing the machine for that program would pause. It changes nothing.
- `cargo test` runs the unit tests.

### Windows, Linux and macOS

| | Windows 11 | Linux | macOS |
|---|---|---|---|
| Builds and passes its tests | yes | yes, on Ubuntu 22.04 under WSL | not tried |
| Reads the machine | yes | yes, run from the command line | written, never run |
| Pause, resume, end, and the locks | yes | yes, run against a throwaway process | written, never run |
| Finds agents | yes | yes, one agent tried | written, never run |
| The window | yes | not seen: the test machine has no EGL or xkbcommon-x11 library, so it could not open | not tried |
| Switch an adapter off | written, not run | written, not run (asks through polkit) | written, never run |
| Block one program on the network | written, not run | not built | not built |

Blocking one program takes a different mechanism on each system. On Linux it is a control group for the program and an nftables rule that matches it. On macOS it is a Network Extension content filter, which has to be signed with an Apple developer identity. The macOS column needs someone with a Mac: the code was written from documentation, and nothing in it has been compiled or run.

Maestro keeps its files in `%LOCALAPPDATA%\Maestro` on Windows, `~/.local/state/maestro` on Linux and `~/Library/Application Support/Maestro` on macOS.

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

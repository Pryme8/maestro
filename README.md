# Maestro

Maestro is a planned replacement for the resource manager and the everyday operating-system interfaces, built for a machine whose main job is running AI agents. It shows everything running on your PC, shuts down whatever the current task does not need, and gives you one visual, manageable place to direct the agents doing the work.

**Status: plan only. No code exists yet.** This repo is public from day one so the design, the doubts and the decisions are all in the open.

## The vision

Today's desktops are built around a person clicking through windows. Task Manager, Settings and the file explorer assume you are the one doing the work. When agents do most of it, the machine should be organised around them instead: its resources dedicated to the task in hand, its state visible at a glance, and its controls built for orchestrating agents, not for launching apps.

Maestro is that agent-centric environment. It runs on top of your existing operating system and takes over the interfaces you use to see and manage it.

## What it does

- **Monitors the whole system.** Processes, memory, CPU, GPU, disk and network, shown as a base you can read at a glance instead of a table of numbers.
- **Clears the machine for the task.** For each task it works out what needs to run, then suspends or stops everything else, with undo. Processes the operating system needs are never touched.
- **Manages what is running.** Every process, connection and adapter can be inspected and controlled from one interface, replacing the resource manager.
- **Treats agents as first-class.** It recognises AI agents, shows what each one is doing and what it costs, and presents them as crew you assign to work.
- **Routes and relays work between agents.** A ledger of every AI account, an advisor that suggests the cheapest route for a task, and a relay that hands work from one agent to the next.
- **Gives agents a managed environment.** A WSL2, VM or container image with the toolchain preinstalled, recorded paths and snapshots, so unattended runs cannot touch the rest of your machine.

The full plan, including what Maestro cannot do, is in [docs/plan.md](docs/plan.md).

## What it will not claim

- Shutting down unneeded processes frees memory, CPU and GPU for local work such as builds and local models. It does not make a cloud-hosted model answer faster.
- It cannot measure answer quality. "Best route" means cheapest within your rules.
- Subscription quota figures are estimates, because vendors do not expose them exactly.
- A relay for a vendor only gets built if that vendor's terms allow it.

## Get involved

Help is welcome at every layer: the monitor, the controls, the agent view and the router.

- Humans: see [CONTRIBUTING.md](CONTRIBUTING.md).
- AI agents: see [AGENTS.md](AGENTS.md). This project is meant to be a better place for you to work, and your view of what that takes is wanted.

## License

[MIT](LICENSE)

# Maestro: plan

As of 2026-11-24. Phases 1 to 4 are running in a first form. Of phase 5, the ledger and advisor are built, the relay prints its plan without running it, and the managed environment is not built.

## Vision

Maestro replaces the resource manager and the everyday operating-system interfaces with an agent-centric environment. It is first a system resource monitor: it shows everything running on the PC, shuts down every process the current task does not need, and gives you a visual, manageable interface to what is left. Agents and the routing of work between them are built on that base.

It runs on top of the existing operating system and takes over the interfaces you use to see and manage it. It has three layers: the system monitor and its controls, the agent monitor, and the agent router.

It is a native program written in Rust with egui, as one executable with no web view. A tool whose job is to free the machine should not bring a browser engine with it.

## The system monitor

This is the foundation, built in phases 1 and 2.

- **Monitor:** processes, memory, CPU, GPU, disk and network, read-only at first, shown in the game style as a base.
- **Task profiles:** for each task, the app works out what needs to run. Everything else is suspended or stopped so the machine's resources go to the task.
- **Undo:** every stop is reversible. Ending a task restores what was running before it.
- **Protected processes:** processes the operating system needs are never stopped, and you can pin anything else you want kept.
- **Controls:** process, network and adapter actions from one interface, replacing the resource manager. Built so far: pause, resume and end a program; block a program at the firewall; switch an adapter off; and an action log with undo.
- **Clearing the machine:** built as a preview. You pick the task, the app lists what could be paused, and nothing is paused until you confirm. The task's family is always left running: what started it and what it started. Saved, named profiles per task come next.
- **Acting on a process does not reach what it started.** Pausing a launcher leaves the program it launched running. Actions today apply to every process with the same program name; acting on a whole family tree at once is the next control to add.
- **Never left frozen:** everything paused is written to disk first, resumed when the app closes, and offered for resuming at the next start after a crash.
- **Many views of the same data:** no single picture suits every question, so the running programs can be drawn several ways. Built so far: tiles, a heat map sized by memory and coloured by load, a tree of what started what, and a map that joins programs to the programs they started. More views are welcome wherever they help a person understand and control what is going on.
- **Critical or extra:** every process is sorted in plain words: critical to the system, part of the system, background service, or app. A process using a critical system name from outside the system folders is flagged. Each operating system has its own rules for this. Today this is judged from names, file locations and parentage. Signature checks come next, and this sorting is what task profiles will use to decide what is safe to stop.

Clearing the machine frees memory, CPU and GPU for local work such as builds and local models. It does not make a cloud-hosted model answer faster, and the app does not claim it does.

## Full control

The goal is that nothing on the machine is hidden from you and nothing is out of your reach. For anything running, connected or powered, the app answers the same questions: what is it, what is responsible for it, what is it doing, how risky is it, and how do I stop, block or remove it.

### Network: follow every connection

If something on the PC is sending data to a server, you see the whole path in plain language.

- **Who is talking:** every connection is tied to the process that opened it, the file on disk behind that process, who signed it, what started it and what makes it start at boot.
- **Where it goes:** the remote address, its domain, the organisation that owns it and its approximate location on a map, with the network hops in between where they answer.
- **What it is doing:** how much it sends and receives, how often, to which domains, and over which protocol.
- **How risky it is:** unsigned or tampered files, unusual locations on disk, unknown destinations and odd timing raise the risk shown. A file's fingerprint can be checked against public reputation services if you opt in.
- **What you can do:** end the process, block the destination or the whole program at the firewall, disable what restarts it, and quarantine or remove the file. Each action is explained before it runs and can be undone.

### Hardware: control what the machine exposes

- **Devices:** every device, what it draws where that is reported, and the driver behind it, with enable and disable.
- **Lighting and fans:** case and component LEDs, and fan speeds, through the controllers that expose them.
- **Power:** device enable and disable, and device sleep states. Holding a single USB port switched off takes a hub that supports per-port switching plus a driver-level route, because Windows offers programs no call for it.

### What full control takes

Nothing here is off the table. Each hard part has a route, and the route has a cost. The plan takes the cheap routes first and names the price of the rest.

- **Reading encrypted traffic:** the first step shows who is talking to whom, how much and when, plus hostnames from the system's own name lookups. Reading the contents takes an opt-in inspection mode per app: either the app's own key log, or a local intercepting proxy with a certificate you install. Apps that pin their certificates need a deeper hook into the app itself.
- **Pinpointing where data goes:** the first step maps an address to its owner and a rough location. Getting closer takes route tracing, latency checks and marking content networks, VPNs and proxies for what they are. Seeing past a VPN or proxy takes information from the far side.
- **Catching malware that hides:** the first step uses Windows' own signature checks and Defender, plus cross-checks that flag inconsistencies. Seeing beneath a compromised system core takes a signed kernel driver, which means a code-signing certificate and Microsoft's hardware program, or a scan from outside the running system.
- **Switching hardware with no controller:** the first step controls everything that exposes a control: lighting, fans, devices. An LED wired straight to a power rail takes a hardware change: move it to an addressable header or a supported controller, or fit a switched line. PCs do not report their internal power routes, so mapping them takes a per-board description that the community can build up.
- **Removing files safely:** removing a file another program depends on can damage it, so removal is quarantine first, delete later.

Each route has been researched, with what can be done now, what takes elevated rights or an opt-in component, and what takes a driver or hardware. See [research/blockers.md](research/blockers.md). That file is desk research: nothing in it has been tested on a machine yet, and it marks which sources were read in full.

## The agent layer

Built in phase 3, in a first form.

- **Recogniser:** agents are found by program name and file location, and tools that run inside Node or Python by their command line. The command line is read once per process and not kept.
- **Helpers:** every process an agent starts is counted with it, stopping at any other agent it launched.
- **Telemetry receiver:** agents that export OpenTelemetry metrics report tokens and cost to a receiver on this computer only. It reads the JSON form, keeps the token and cost figures and discards the rest, including account and email fields. Any local program can send it figures, so they are treated as the agent's own claim.
- **Budgets:** a daily dollar limit per agent. The energy bar shows what is left and passing it logs a warning. Pausing an agent automatically when its budget runs out is the next step and will be opt-in.
- **Quests:** a named piece of work assigned to an agent, with what the agent reported spending since it started.
- **What reporting needs:** an agent only shows spending if it reports it. Subscription plans report tokens but not a real bill, so cost there is the vendor's own estimate. This is the same limit the router's ledger works within.

## Depth

Built in phase 4, in a first form.

- **History:** the sampling thread writes one line per minute with average and peak processor load, memory, download, upload, agents running and processes paused. A day of minutes is kept. It is recorded off the window's thread, so it carries on while the window is minimised.
- **Timeline:** the history drawn over 1, 6 or 24 hours, with a mark for each event and that minute's figures under the pointer. Gaps show when Maestro was not running.
- **Events:** actions, warnings, agents starting and stopping, and achievements. The last 400 are kept.
- **Account totals:** the receiver now keeps each finished day, so the ledger can show what every agent reported today, over 7 days, over 30 days and in total. Up to 400 days are kept. These are the figures the router's ledger will start from.
- **Achievements:** judged from running counts that only go up, such as processes paused, quests finished and the longest calm stretch. They are there to make good habits visible, and none of them rewards spending more.
- **Linux and macOS:** the rules that differ per system sit behind one interface each: how processes are sorted, how a process is paused and ended, how an adapter is switched, and where files are kept. Linux builds, passes its tests and runs from the command line. macOS is written from documentation and has never been compiled.
- **Still open in this phase:** a longer history than a day, per-program lines on the timeline, the Linux and macOS ways of blocking one program, and a first run on a Mac.

## The router layer

The router layer picks which AI account and model handles each step of a task, then hands the work between them. It sits on top of the agent layer and is phase 5.

Built so far, in a first form:

- **Ledger:** accounts you enter by hand, each with a tool, a model, your rating of the model, and either a price per million tokens or your estimate of an allowance over a rolling window of hours. The receiver now files usage by the hour, for eight days, so a rolling window can be counted.
- **Advisor:** for a task's size and the rating it needs, up to three routes with their cost and their effect on each allowance. It prefers something that fits, then the cheapest, then the allowance with the most left, and keeps a reserve of each allowance out of its plans. A two-tool split counts the cost of the handoff and is dropped when it saves nothing.
- **Relay plan:** a dry run. It prints what each step would run and the handoff brief. Commands are shown per tool and per kind of sign-in, from [research/relay.md](research/relay.md): Claude Code on either, and Codex, Cursor and Gemini CLI on an API key only, because their terms do not say whether another program may start them on a subscription login. Cursor's acceptable use policy also bars automated access outright, which conflicts with its own documents for scripted runs and is unresolved.

Not built: running a relay, the managed environment, and context balancing. What they take is below.

It can be exact about cost on API-billed accounts, can only estimate subscription quota, and cannot measure answer quality. "Optimal" therefore means the cheapest route that follows rules you set.

## What the router adds

The router adds three parts: a ledger, an advisor and a relay. A fourth, context balancing, is deferred.

- **Ledger:** for each account, it holds the plan, what is left, when it resets and the price per model. Where a subscription does not expose its remaining allowance, you enter the limit once and the app counts down from what it observes. That count drifts, because allowances use rolling windows and are also spent from other devices and the web app, so subscription figures are shown as estimates.
- **Advisor:** for a task, it shows two or three routes with their cost and quota impact. Example: draft on Cursor with model A because the allowance resets in six days, then review on Claude with model B.
- **Relay:** it runs the chosen route. It starts the first tool without its chat window, captures the result, compacts it into a handoff brief and passes that to the next tool. This works because Claude Code, Codex and Cursor each offer a scripted, no-chat way to run ([source](https://flaviocopes.com/ai-glossary/headless-mode/)).
- **Context balancing (deferred):** it would watch how full each session is and propose a handoff brief seeded into a fresh session on the same or a cheaper service. It is not built until ledger data shows a saving: the brief costs tokens, the fresh session re-reads the files, prompt caching keeps long sessions cheaper than they look, and the tools already compact themselves.

## Limits

The router works within five limits.

- **Quality:** the app cannot measure how good an answer is. Routing follows your rules plus your ratings of past results.
- **Lossy handoffs:** a brief drops detail and costs tokens to write. The advisor says when a task should stay in one tool.
- **Scripted access only:** Cursor's editor window cannot be driven from outside, only its command-line agent. Scripted runs also fail on team accounts whose admin has disabled them ([source](https://cursor.com/docs/cli/cookbook/code-review)).
- **Vendor terms:** the relay only launches each vendor's own tool with your existing login and never handles credentials. Each vendor's terms are checked before its relay is designed, not just before it is wired: if a vendor disallows driving a subscription login from a third-party orchestrator, there is no relay for that vendor.
- **Safety:** a relayed Cursor run can still write files and execute shell commands ([source](https://cursor.com/docs/cli/reference/parameters)), and the other tools are similar. Unattended runs only get far with file and shell permissions granted up front, so approving a route means approving unattended shell access. Relays therefore run in the managed environment, in a git worktree, not just a folder you choose, and need your approval per route by default.

## Managed environment

Relays run inside an environment image the app manages, not on your desktop. Agents get a known, isolated place to work while you stay on your own machine.

- **Image:** a WSL2 distro first, then a VM or dev container, with the coding toolchain and each vendor's command-line agent preinstalled. You sign in to each tool inside it once.
- **Recorded paths:** only the folders you mount are visible inside, and the app records which ones each route uses.
- **Snapshots:** the app takes a snapshot before each relay, so a bad run can be rolled back.
- **Not a separate operating system:** Maestro replaces the interfaces of the operating system you already run, not the system underneath. A boot-into system was considered and dropped for now, because it would mean maintaining a Linux distribution. A bootable USB version of the same image is a possible later step.

## In the game

In the app's game view, where the PC is a base and agents are crew, routes appear as missions.

- **Missions:** a route is a mission with stages. Crew pass a baton, the handoff brief, from one to the next.
- **Energy:** each crew member's bar is that account's remaining allowance. It is an estimate for subscriptions and refills as the allowance window rolls over.
- **Quartermaster:** the advisor appears as a character who proposes the crew for each job.

## Build order

The router is phase 5, but a bare ledger and advisor are trialled first as a standalone tool. Its payoff is quota arbitrage for people with two or more subscriptions, and a few days of trial can confirm that before the other phases are committed.

1. **Monitor and base:** read-only system telemetry, in the game style.
2. **Controls:** task profiles that stop what a task does not need, plus process, network and adapter actions, all with undo.
3. **Agents:** recogniser, telemetry receiver, crew, quests and budgets.
4. **Depth:** timeline, achievements, account totals, then Linux and macOS. Built, except that macOS has never been compiled and the Linux window has not been seen.
5. **Router:** the advisor first, since it only needs the ledger, then the managed environment and the relay. The ledger and advisor are built and the relay is a dry run; the managed environment comes next, because a relay must not run outside it. It depends on phase 3 only, so it can move ahead of phases 1, 2 and 4 if the trial pays off.

Cursor and Claude adapters are built first. Each phase works on its own, so the build can stop after any of them.

The full build is large: a system monitor, an agent monitor and an agent router in one app.

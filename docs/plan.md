# Maestro: plan

As of 2026-11-24. Nothing is built yet.

Maestro is a desktop app in three layers: a PC scanner, an agent monitor and an agent router. The router layer picks which AI account and model handles each step of a task, then hands the work between them. It sits on top of the agent layer and is planned as phase 5, after a standalone trial of the ledger and advisor.

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
- **Not an operating system:** a boot-into system was considered and dropped. The work is network-bound, so it would gain little speed and would mean maintaining a Linux distribution. A bootable USB version of the same image is a possible later step.

## In the game

In the app's game view, where the PC is a base and agents are crew, routes appear as missions.

- **Missions:** a route is a mission with stages. Crew pass a baton, the handoff brief, from one to the next.
- **Energy:** each crew member's bar is that account's remaining allowance. It is an estimate for subscriptions and refills as the allowance window rolls over.
- **Quartermaster:** the advisor appears as a character who proposes the crew for each job.

## Build order

The router is phase 5, but a bare ledger and advisor are trialled first as a standalone tool. Its payoff is quota arbitrage for people with two or more subscriptions, and a few days of trial can confirm that before the other phases are committed.

1. **Scanner and base:** read-only system telemetry, in the game style.
2. **Controls:** process, network and adapter actions, with undo.
3. **Agents:** recogniser, telemetry receiver, crew, quests and budgets.
4. **Depth:** timeline, achievements, account totals, then Linux and macOS.
5. **Router:** the advisor first, since it only needs the ledger, then the managed environment and the relay. It depends on phase 3 only, so it can move ahead of phases 1, 2 and 4 if the trial pays off.

Cursor and Claude adapters are built first. Each phase works on its own, so the build can stop after any of them.

The full build is large: a system monitor, an agent monitor and an agent router in one app.

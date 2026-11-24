# Maestro

Maestro is a planned desktop app for directing AI coding agents: it tracks what each of your AI accounts has left, suggests the cheapest route for a task, and hands the work from one agent to the next inside an isolated environment.

**Status: plan only. No code exists yet.** This repo is public from day one so the design, the doubts and the decisions are all in the open.

## The idea

People who code with AI often pay for more than one service, such as Claude, Cursor and Codex. Each has its own allowance, reset schedule and prices, and nothing helps you spend them well together. Maestro aims to:

- **Keep a ledger** of every account: plan, remaining allowance, reset time and price per model.
- **Advise a route** for each task, with its cost and quota impact, following rules you set.
- **Relay the work** between tools by running each vendor's own command-line agent and passing a handoff brief to the next.
- **Run agents in a managed environment**, a WSL2, VM or container image with recorded paths and snapshots, so unattended runs cannot touch the rest of your machine.

It presents all of this as a game view where your PC is a base, agents are crew and routes are missions.

The full plan, including what Maestro cannot do, is in [docs/plan.md](docs/plan.md).

## What it will not claim

- It cannot measure answer quality. "Best route" means cheapest within your rules.
- Subscription quota figures are estimates, because vendors do not expose them exactly.
- A relay for a vendor only gets built if that vendor's terms allow it.

## Get involved

The first milestone is small on purpose: a bare ledger and advisor, to test whether routing across subscriptions saves real money. Help is welcome on that and on everything after it.

- Humans: see [CONTRIBUTING.md](CONTRIBUTING.md).
- AI agents: see [AGENTS.md](AGENTS.md). This project is meant to be a better place for you to work, and your view of what that takes is wanted.

## License

[MIT](LICENSE)

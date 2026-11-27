# Changelog

What changed in each release. No release has been made yet.

## Unreleased

What the first release will hold. Each item is in a first form; [docs/status.md](docs/status.md) says what has been tested.

- **Base:** processor, memory, disks, network and graphics every second, every process grouped by program, and four views: tiles, heat map, tree and map.
- **Sorting:** each process marked critical, part of the system, background service or app. Stale programs flagged as not responding or cold.
- **Controls:** pause, resume and end a program or its whole family; clear the machine for a task; profiles; block a program on the network; switch an adapter off; an action log with undo.
- **Network page:** connections by program, names and owners, and listening ports.
- **Agents:** eleven agent tools recognised, with their helpers counted in, usage reports on port 4318, budgets and quests.
- **Live page:** follow Claude Code sessions step by step, start one and direct it, in a safe copy that can be rolled back, approve or refuse each step it asks to take or allow a kind of step for the session, several Claude Code sign-ins kept apart and chosen per session, a model per session, a map of each session, a preview of the pictures a session writes that can be switched on, and the state of each agent tool.
- **Optimize:** a button beside the pages that gathers a report on the machine (firmware, drivers, settings, stale and heavy programs) and has an agent you are signed in to read it by a written set of rules and list what is worth changing. It advises only. Windows first; the Linux and macOS readers are written and have not been run.
- **Timeline page:** a day of history, events, what agents spent, and thirteen achievements.
- **Router page:** a ledger of AI accounts, an advisor, and a relay plan as a dry run.
- **Managed environment,** from the command line: recorded folders, works, snapshots and rollback.
- **Systems:** Windows 11 first. Linux builds and runs from the command line. macOS is written and has not been run.
- **Security guard rails:** a written set of rules in [SECURITY.md](SECURITY.md), tests that fail when a change starts a new program, uses the network, adds `unsafe` code or adds a crate, and checks on every pull request.
- **Documentation** in [docs/](docs/README.md), and a build that makes the downloads for Windows, macOS and Linux.

# Data and privacy

Maestro has no account, no telemetry of its own and no update check. Everything it records stays in one folder on your computer.

## The files it keeps

The folder is `%LOCALAPPDATA%\Maestro` on Windows, `~/Library/Application Support/Maestro` on macOS and `~/.local/state/maestro` on Linux, unless `MAESTRO_DATA_DIR` says otherwise.

| File | Holds |
|---|---|
| `state.json` | What Maestro has changed and not yet undone: processes it paused, programs it blocked, adapters it switched off. Also your keep list, profiles, budgets and quests |
| `history.json` | One line a minute of machine load for the last day, the last 400 events, and the counts achievements are judged from. No process names, except inside the text of an event that describes an action you took |
| `usage.json` | The token and cost figures agents reported, by hour for eight days and by day for up to 400 days |
| `ledger.json` | The AI accounts you entered on the Router page |
| `environment.json` | Recorded folders, works and their snapshots |
| `logins.json` | The names of the Claude Code sign-ins you added and, for a key sign-in, the name of the variable that holds its key. Never a key |
| `logins/` | One folder per named sign-in. Claude Code keeps that account's sign-in, settings and session history in it. Maestro makes the folder and does not open what the tool puts there |
| `work/` | The worktrees of your works |

The files are plain JSON you can read. Delete the folder to forget everything.

**The `logins/` folders hold whatever Claude Code stores to stay signed in to each account.** Treat them like a password: do not copy them to a shared place or into a backup other people can read. Deleting the data folder signs those accounts out and deletes their session history.

## What it reads

- **Running processes:** names, file paths, what started each one, and processor, memory and disk figures. Command lines are read once per process to recognise agents, and are not kept.
- **Network sockets:** which process owns each one, and the remote address and port. Not the contents of any traffic.
- **Claude Code's session files,** on the [Live](live.md) page and with `--watch`. These hold your conversations with the agent. Maestro shows their steps and holds them in memory only: they are not copied into Maestro's own files.
- **Pictures in a session's folder,** only while **Show what it is making** is ticked on the [Live](live.md) page. The picture is held in memory to be shown and is not copied anywhere.
- **Whether a key is set** for each agent tool. Maestro checks that a key exists and never shows or stores it. For a sign-in that uses a key, Maestro reads the key from the variable you named at the moment a session starts and hands it to Claude Code. It is not written down or shown.

## What leaves your computer

Maestro sends nothing to its authors or to any service of its own. These are the only things that reach a network:

- **Name lookups.** The [Network](network.md) page asks the DNS server your system already uses for the name behind each address a program is connected to. That server therefore sees those addresses. Untick **Look up names**, or use `--no-names`, to switch this off.
- **Agents you start.** A session started from the Live page, or with `--run`, is Claude Code running on your own login. It talks to its vendor exactly as it would if you had started it yourself.
- **Tool checks.** To see whether Claude Code is signed in, Maestro runs `claude auth status`. What that command does on the network is up to the tool.

## What listens

Maestro opens one port: 4318, on this computer only, for [usage reports](agents.md#usage-reports). Other computers cannot reach it. Any program on this computer can send it figures. It keeps token and cost figures and discards the rest of each report, including account and email fields.

## Changes it makes outside its folder

Only when you ask:

- **Paused programs.** Resumed when Maestro closes.
- **Firewall rules** on Windows, in a rule group named "Maestro".
- **A switched-off network adapter.**
- **Branches and refs in your repositories:** `maestro/<name>` branches and `refs/maestro/` snapshots, made by the [managed environment](environment.md).

## Screenshots

A picture of Maestro's window shows the names of the programs you run, the hosts they are connected to and, on the Live page, the contents of agent sessions. Check a screenshot before you share it.

# Command line

Every flag is given to the one `maestro` program. With no flag it opens the window. Most flags print something and exit without opening a window.

How to reach the program:

| | |
|---|---|
| Windows download | `maestro.exe <flags>` |
| macOS download | `/Applications/Maestro.app/Contents/MacOS/maestro <flags>` |
| Linux download | `./maestro <flags>` |
| From source | `cargo run --release -- <flags>` |

On Windows the release program has no console of its own. It prints into the terminal it was started from.

## The window

| Flag | Does |
|---|---|
| `--view <tiles\|heat\|tree\|mold>` | Opens on that view of the [Base](base.md) page |
| `--page <base\|timeline\|network\|live\|router>` | Opens on that page |
| `--select <program>` | Opens with that program's details showing |
| `--preview-focus` | With `--select`, also opens its clear-the-machine preview. Changes nothing |
| `--screenshot <file.png>` | Opens the window, saves one picture of it and exits |
| `--tour <folder>` | Opens the window once, saves a picture of each page into the folder and closes. The pictures show your running programs, so check them before sharing |

## The machine

| Flag | Does |
|---|---|
| `--dump` | Prints one sample of the machine as JSON |
| `--profile` | Times each part of a sample, three rounds, to show what the monitor itself costs |
| `--connections` | Lists who each program is connected to and which ports are listening. Waits up to twenty seconds for names |
| `--no-names` | With `--connections`, skips looking names up |

## Controls

| Flag | Does |
|---|---|
| `--pause <number>` | Pauses one process by its number |
| `--resume <number>` | Resumes it |
| `--end <number>` | Ends it |
| `--focus-plan <program>` | Lists what clearing the machine for that program would pause. Changes nothing |

These follow the same locks as the buttons: see [Controls](controls.md#guard-rails). They exit with 0 when it worked, 1 when the action failed, 2 when no process has that number, and 3 when the process is locked.

## Agents

| Flag | Does |
|---|---|
| `--agents` | Lists the AI agents found running |
| `--receive` | Runs only the usage receiver and prints totals as reports arrive |
| `--services` | Prints each agent tool's state, how it was checked and what to do about it |
| `--watch` | Prints what your Claude Code sessions are doing as it happens |
| `--watch-once` | Prints their latest steps and exits |
| `--run <folder> "<request>"` | Starts Claude Code in that folder, read-only, and prints each step until it answers |

See [Agents](agents.md) and [Live](live.md).

## Ledger and advisor

| Flag | Does |
|---|---|
| `--ledger` | Lists your AI accounts with what is left of each allowance |
| `--account-add <name> <tool> <model> <light\|standard\|heavy> payperuse <$ in> <$ out>` | Adds an account billed per million tokens |
| `--account-add <name> <tool> <model> <light\|standard\|heavy> allowance <tokens> <hours>` | Adds a subscription account with an estimated allowance over a rolling window |
| `--account-remove <name>` | Removes an account |
| `--advise <tokens>` | Proposes routes for a task of that size |
| `--tier <light\|standard\|heavy>` | With `--advise`, the rating the task needs. Standard if left out |
| `--review` | With `--advise`, asks for a second tool to review the work |
| `--relay-plan` | With `--advise`, prints what a relay would run. Starts nothing |

See [Router](router.md).

## Managed environment

| Flag | Does |
|---|---|
| `--env` | Shows the environment |
| `--env-use <distro>` | Windows: runs git inside that WSL distro. `none` goes back to this machine |
| `--env-plan` | Prints how a dedicated image would be made. Makes nothing |
| `--env-mount <folder>` | Records a folder |
| `--env-unmount <folder>` | Stops recording it |
| `--work` | Lists works |
| `--work-new <repository> <name>` | Cuts a work from a repository |
| `--work-snapshot <name> [note]` | Records the work as it is |
| `--work-rollback <name> [number]` | Puts it back to a snapshot, the latest if no number is given |
| `--work-remove <name>` | Deletes the worktree and keeps its branch |

See [Managed environment](environment.md).

## Environment variables

| Variable | Does |
|---|---|
| `MAESTRO_DATA_DIR` | Keeps Maestro's files in this folder instead of the usual one. Use it to try anything here without touching your real files |
| `MAESTRO_CLAUDE_DIR` | Where to look for Claude Code's session files, instead of `.claude/projects` in your home folder |

There is no `--help` or `--version` flag yet.

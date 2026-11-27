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
| `--setup` | Prints each agent tool that is on this computer but not set up, and the steps left |
| `--optimize-report` | Prints the report the Optimize page would send to an agent. Sends nothing |
| `--optimize-run <model>` | Opens the Optimize page and runs the analysis on that model once the report is ready, as if the button had been clicked. The report is sent to the agent, and the answer is also printed |
| `--setup-guide <tool>` | Opens with that tool's setup guide showing: `claude-code`, `cursor` or `ollama` |
| `--preview-focus` | With `--select`, also opens its clear-the-machine preview. Changes nothing |
| `--screenshot <file.png>` | Opens the window, saves one picture of it and exits |
| `--tour <folder>` | Opens the window once, saves a picture of each page into the folder and closes. The pictures show your running programs, so check them before sharing |

## The machine

| Flag | Does |
|---|---|
| `--dump` | Prints one sample of the machine as JSON |
| `--gpu` | Prints the graphics cards and the 15 processes using them most |
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
| `--run … --safe` | Runs in a safe copy of the repository in that folder, with a snapshot taken first. Prints what changed and the `--work-rollback <name> <snapshot>` line that undoes it |
| `--run … --model <name>` | Picks the model for that run: a short name such as `haiku`, or a full model name |
| `--run … --as <name>` | Runs under a named sign-in instead of Claude Code's own |
| `--logins` | Lists the named sign-ins, whether each is ready, and its folder |
| `--login-add <name> [--key-var <VARIABLE>]` | Adds a sign-in with its own folder, or one that takes an API key from that variable |
| `--login-forget <name>` | Takes a sign-in off the list and leaves its folder in place |
| `--run <folder> "<request>" --answer allow\|refuse\|session` | The same, but it asks before each step that needs permission and every question gets that answer. `session` gives a standing yes for each kind of step the first time it comes up |

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

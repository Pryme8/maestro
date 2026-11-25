# Live

The Live page shows what an agent is doing as it does it, and lets you start a session and direct it. Today it works with Claude Code only.

## Watch any Claude Code session

Every Claude Code session active in the last hour is listed, wherever it was started: a terminal, an editor or the desktop app. Pick one to see its steps as they happen:

- what was asked,
- what the agent said,
- each tool call, with the command, file or search in it,
- what came back, with its size.

Maestro reads the session files Claude Code already keeps on this computer, in `.claude/projects` in your home folder on every system. Steps are held in memory only, and nothing is sent anywhere.

A session started somewhere else can be watched but not directed from this page. To pause or end it, find its program on the [Base](base.md) page.

## Start a session and direct it

Give a folder and a request, and Maestro starts Claude Code there with no window, on your own Claude Code login. While it works you can:

- send it a message,
- interrupt the current step,
- pause and resume it,
- close it to new messages, so it finishes what it has and exits,
- stop it.

**By default it may only read and search.** One tick lets it edit files in its folder. A session started here cannot ask you anything part-way through: it is either read-only or allowed to edit. Approving each step from Maestro is planned and not built.

## The map

Each session can be shown as a map or as a list.

![The map of one session](images/live-map.png)

- The agent sits in the middle.
- Each tool it has used sits around it.
- What each tool was pointed at (files, commands, sites) sits on the outside.
- Whatever was used most recently is brightest.
- A ring pulses around a tool whose call has not come back yet.
- A failed call turns its tool red.

Hover anything for the detail. The step list stays underneath the map, and the **List** switch shows the list alone.

## Approving each step

A session started from Maestro asks before every step that needs permission, such as changing a file or running a command Claude Code does not already treat as harmless. The question appears above the controls in an amber box and the session's chip shows "needs you".

- The box shows the tool, the exact command or file, what the tool says it is for and why it has to ask. Hover the command for the whole call.
- **Allow this once** lets that one step go ahead exactly as shown. The next step is asked again.
- **Refuse** stops the step. Anything typed in the reason box goes back to the agent, so it can try another way.

When you start a session you choose one of three settings: ask each time, let file edits through and ask about everything else, or refuse everything so it can only read and search. Allowing a kind of step for the rest of a session, and changing a step before allowing it, are not built.

## More than one account

You can keep several sign-ins for Claude Code, such as a work account and a personal one, and choose which one a session runs under when you start it.

- **How it is kept apart.** Claude Code keeps its sign-in, settings and history in one folder, and a variable it documents, `CLAUDE_CONFIG_DIR`, says which. Each named sign-in gets a folder of its own under Maestro's data folder. A session started under a name is pointed at that folder and stays there until it ends, so two accounts never share a sign-in, a setting or a history file.
- **Adding one.** Open Claude Code under Services, type a name and press **Add sign-in**. Then press its **Sign in…**, which opens a terminal running `claude auth login` for that sign-in only, and sign in there with the account you want it to be.
- **Using a key instead.** Give the name of an environment variable that holds an API key for that sign-in. Maestro keeps the variable's name and hands the key to the tool when a session starts. The key is never written down or shown.
- **Choosing.** The start form has a **Sign-in** list: Claude Code's own, or any you added. The session's name shows which it runs as.
- **What a new sign-in does not have.** It starts clean: your settings, connectors and memory from Claude Code's own folder are not copied into it.
- **Forgetting one** takes it off the list and leaves its folder in place, and says where the folder is.

Maestro makes the folders and passes the variable. It does not open the files the tool keeps in them. Sessions started outside Maestro under a named sign-in are not followed yet; only those under Claude Code's own folder are.

Each session can also be given a **Model**: Claude Code's own setting, one of its short names (haiku, sonnet, opus, fable), or a full model name. A small job on a cheaper model uses less of your plan.

## Services

Each agent tool is one line that opens for detail, and the ones that need something from you open by themselves. For each tool:

- whether it is installed,
- whether it is signed in or has a key set,
- what Maestro can do with it,
- how each of those was checked,
- the exact step that fixes what is missing.

**Sign in…** opens a terminal running the tool's own sign-in command: a console window on Windows, Terminal on macOS, and the default terminal on Linux. Maestro checks whether a key is set and never reads, shows or stores the key itself.

| Tool | Maestro can |
|---|---|
| Claude Code | Follow its sessions, start one and direct it |
| Codex, Cursor's command-line agent, Gemini CLI | Report whether it is installed and has a key. It cannot be started or followed yet |
| Cursor's editor | Report whether it is installed. It is listed apart from Cursor's command-line agent, and cannot be driven from outside |
| Ollama | Report whether it is installed and running. Work cannot be sent to it yet |

A session cannot be started while Claude Code is not ready, and the page says why.

A program started from the macOS Finder or Dock does not get the `PATH` your terminal has. If Maestro reports a tool as not installed on a Mac when it is, start Maestro from a terminal. This has not been checked on a Mac.

## From the command line

- `--services` prints each tool's state, how it was checked and what to do about it.
- `--watch` prints what your Claude Code sessions are doing as it happens. `--watch-once` prints their latest steps and exits.
- `--logins` lists the named sign-ins and whether each is ready. `--login-add <name>` adds one, with `--key-var <VARIABLE>` for one that uses an API key, and `--login-forget <name>` takes one off the list.
- `--run` takes `--as <name>` to run under a named sign-in and `--model <name>` to pick the model.
- `--run <folder> "<request>"` starts Claude Code in that folder, read-only, and prints each step until it answers. Add `--answer refuse` or `--answer allow` to have it ask before each step that needs permission and give every question that same answer. `--answer allow` lets it change files and run commands unasked, so use it only in a folder you can throw away.

If your Claude Code session files are somewhere else, set `MAESTRO_CLAUDE_DIR` to the folder that holds them.

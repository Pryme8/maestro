# Optimize

The **Optimize** button sits beside the pages at the top of the window. It gathers a report on your machine and has an agent read it and list what is worth changing. It advises only: nothing on the machine is changed.

## What you need

An agent Maestro can start. Today that is Claude Code, signed in or with a key set. Until one is ready the button is grey, and hovering over it says what to do: activate one under **Services** on the [Live](live.md) page. The analysis runs under the sign-in chosen there.

## What happens

1. Click **Optimize**. Maestro gathers the report, which takes about fifteen seconds on Windows, and shows it under **The report that will be sent**. Nothing has been sent yet.
2. Pick a model if you want one other than the tool's own setting, and click **Run the analysis**. The report and the rules go to the agent. This uses your plan like any other session, and the session shows on the Live page.
3. The findings come back as a list, largest gain first. Each has an area (firmware, drivers, hardware, settings or programs), the gain, the risk, why, and how. A finding about one program has a button that opens that program on the [Base](base.md) page, where you can pause or end it.

## What is in the report

- The system, processor, memory, volumes and graphics card, as Maestro measures them.
- The programs running now: the stale ones (not responding, or cold for fifteen minutes or more), the largest by memory and the busiest, each with its kind.
- Firmware maker, version and date; the computer and board; memory modules and the speed they run at; drives and their health; displays; drivers not supplied by the system's maker, with dates; devices reporting a problem; the power plan; and programs set to start with the system.

It holds no user name, computer name, serial number or file path. `--optimize-report` prints it without sending anything.

## The rules

The agent is told, among other things:

- Use only the report, and use no tools.
- Never advise weakening security, and never advise a registry cleaner, a driver-updater, a tweak script or a download from anyone but the maker.
- Advise a firmware update only from the maker's own page, say that a failed one can stop the machine from starting, and say "check whether a newer one exists", because it cannot see which version is newest.
- Never advise closing a program that is critical or part of the system.
- No overclocking, and no padding: an empty list is a fair answer.

The full text is `RULES` in `src/optimize.rs`.

## Limits

- The findings are an AI model's reading of a report. Check a firmware or driver step against the maker's own page before you take it.
- Maestro cannot read settings inside the firmware, such as fan curves or whether a memory profile is on. It infers what it can from what the system reports.
- Windows is the system this has been run on. On Linux the report is read from files the kernel keeps and has not been run. On macOS it asks the system profiler and has not been compiled or run.
- Only Claude Code can do the reading today. Other agents need a way for Maestro to start them first.

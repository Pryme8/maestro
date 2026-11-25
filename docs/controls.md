# Controls

Maestro can act on what it shows. Every action is either undoable from the action log or asks you to confirm first. The buttons are in a program's details on the [Base](base.md) page; click a program to open them.

## Pause and resume

**Pause** freezes a program so it uses no processor. Nothing is lost: its memory stays as it was, and **Resume** lets it carry on from the same point.

A paused program does not answer anything, including its own window and anything waiting on it over the network. Programs that talk to a paused one may time out.

## End

**End…** stops a program. It asks for a second click to confirm, because unsaved work in that program is lost and this cannot be undone.

## A program and everything it started

Pause, Resume and End act on every process with the same program name. One tick makes them also act on every process the program started, and the ones those started. Locked processes inside that family are left alone.

## Clear the machine for a task

Pick the program that matters and press **Clear the machine for this**. Maestro lists what could be paused and pauses nothing until you confirm.

- The task itself, what started it and what it started are always left running.
- Anything on your keep list is left running. Tick **Always keep running when clearing the machine** in a program's details to put it on the list.
- Apps are ticked by default. Background services are offered but not ticked.
- Untick anything you want left alone, then pause the rest in one go.

This frees memory and processor for local work such as builds and local models. It does not make a cloud-hosted model answer faster.

### Profiles

Save the set you ticked under a name and pause it again later with one click. A profile only names programs. What is safe to pause is worked out again each time, so a profile never pauses the task's family, your keep list or anything locked.

## Block a program on the network

**Block network** stops one program from making or receiving connections, and **Unblock network** lifts it.

| System | How | State |
|---|---|---|
| Windows | Two Windows Firewall rules for the program's file, outgoing and incoming, in a rule group named "Maestro". Windows asks for permission first | Written, not yet run |
| macOS | Takes a Network Extension content filter, which has to be signed with an Apple developer identity | Not built. The button says so |
| Linux | Takes a control group for the program and an nftables rule that matches it | Not built. The button says so |

## Switch a network adapter off

Click an adapter in the Comms panel, then **Switch off…**. The operating system asks for permission first: Windows with its own prompt, macOS with the system password prompt, Linux through polkit. **Switch back on** reverses it. Switching off the adapter you are connected through takes the computer offline until you switch it back on.

## The action log

The action log runs along the bottom of the window. Everything Maestro did is listed there, with an **Undo** button on everything that can be undone and **Resume all** for everything paused.

## Guard rails

- **Locks.** Processes critical to the system, and parts of the system, are locked and cannot be paused, ended or blocked, from the window or the command line.
- **Never left frozen.** Everything Maestro pauses is written to disk first, resumed when Maestro closes, and offered for resuming at the next start if Maestro crashed.
- **Network changes outlive Maestro.** A firewall block and a switched-off adapter are changes to the operating system and stay in place when Maestro closes. Maestro remembers them and offers the undo next time.

## From the command line

`--pause <number>`, `--resume <number>` and `--end <number>` act on one process by its number, under the same locks. `--focus-plan <program>` prints what clearing the machine for that program would pause, and changes nothing. See [Command line](command-line.md).

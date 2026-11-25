# Network

The Network page shows who each program is talking to and which ports on this computer are open.

## By program

Every connection is grouped under the program that opened it, and each one is marked as:

- **inside this computer**, hidden unless you tick **Show connections inside this computer**,
- **on your network**, or
- **on the internet**.

A box at the top filters by program, name or address. **Details and actions** opens the program's details, where you can pause it, end it or [block it on the network](controls.md#block-a-program-on-the-network).

## Names and owners

For each address Maestro looks up the name registered for it, in the background, and shows the organisation for the most common hosts.

- The lookups go to the DNS server your system already uses, and nowhere else. Untick **Look up names** to switch them off.
- The name is the one registered for the address, which is often the hosting company and not the site the program asked for.
- Lookups are slow on some networks, so names can take a while to appear.

## Listening ports

Every port a program is waiting on, what that port is normally used for, and whether other computers can reach it. Ports only this computer can reach are hidden unless you tick **Show ports only this computer can reach**.

Firewall rules are not read, so a port shown as reachable may still be blocked by your firewall.

## What it does not show yet

- How much each connection carries.
- The names the program itself asked for.
- Where an address is on a map.
- Who signed the program.
- The contents of encrypted traffic. See the [plan](plan.md#what-full-control-takes) for what that would take.

Blocking a single destination, as opposed to a whole program, is not built either.

## On each system

The page has been seen with real data from Windows only, through `--connections`. On macOS and Linux the system may show a program's sockets only for your own programs unless Maestro is run with administrator rights; how much appears there has not been checked.

## From the command line

`--connections` prints the same data without a window. It waits up to twenty seconds for names; add `--no-names` to skip them.

# Maestro sizzle reel: script, shot list and edit plan

A draft for review. Nothing has been captured or rendered yet. 75 seconds, 1920x1080, 30 frames a second, for YouTube.

Every shot is the real program. Title cards are plain text on the site's dark background. There is no voice: the words below are on-screen captions, so the reel works with the sound off. They can also be read as a voice-over if you want to record one.

## Script

| # | Time | On screen | Caption |
|---|---|---|---|
| 1 | 0:00–0:04 | Title card, black to dark navy. The map's teal node pulses once. | Your desktop was built for a person clicking through windows. |
| 2 | 0:04–0:08 | Title card. | Now agents do the work. |
| 3 | 0:08–0:12 | Logo and name. | **MAESTRO.** The resource manager, rebuilt for agents. |
| 4 | 0:12–0:20 | Base page, live: the Reactor, Stores, Docks and Comms panels ticking each second. Slow push in on the crew row. | Everything running on your machine. Your agents, found by themselves. |
| 5 | 0:20–0:27 | Base page: switch tiles → heat map → tree → map. One second each, hard cuts on the beat. | Four views of the same machine. |
| 6 | 0:27–0:35 | Click a program, press **Clear the machine for this**. The preview list appears with its ticks. | Clear the machine for the task. Nothing pauses until you say so. |
| 7 | 0:35–0:41 | Network page: connections grouped by program, names and owners filling in. | See who every program is talking to. |
| 8 | 0:41–0:50 | Live page, a real session: the map grows as tools are called, a ring pulses on the running one. | Watch an agent work, one tool call at a time. |
| 9 | 0:50–0:57 | Live page: the amber question box. The pointer presses **Refuse**. Cut to a second question, **Allow this once**. | Allow it. Or refuse it. No answer means no. |
| 10 | 0:57–1:02 | Live page: the safe copy line, then **What has it changed?** listing files. | It works in a safe copy you can roll back. |
| 11 | 1:02–1:08 | Title card, three lines appearing one by one. | No account. No telemetry. Nothing sent off your computer. |
| 12 | 1:08–1:12 | Title card. | Early. Open from day one. MIT. |
| 13 | 1:12–1:15 | Logo, then the address. | github.com/Pryme8/maestro |

Captions say only what the documentation says works today. Shot 6 shows the preview and stops before the pause, because the buttons have not yet been clicked through by a person. Shot 10 shows buttons the status page lists as not yet clicked; if they misbehave on the day, the shot is replaced by the `--run … --safe` output in a terminal, which has been run for real.

## Shot list: what has to be captured

The window is 1320x860. It is placed at its own size on the 1920x1080 frame, on the dark background with a soft glow, and pushed in on for detail. It is not stretched to fill the frame, which would blur the text.

| Shots | How it is captured | What it shows of yours |
|---|---|---|
| 4, 5 | The window recorded for about 30 seconds while the four views are clicked. Stills from `maestro --tour <folder>` are the fallback. | Your running programs and agents |
| 6 | The window recorded while a program is selected and the preview opened. `--select <program> --preview-focus` opens straight onto it. | Your running programs |
| 7 | The window on `--page network`, recorded for 15 seconds. | Programs, hosts and owners they connect to |
| 8, 9, 10 | One real Claude Code session started from the Live page in a scratch repository, on Haiku, with a staged request: "Add a function that reverses a string to util.py, with a test, then run the test." Recorded from start to stop. | Only the scratch repository |

Everything is captured with `MAESTRO_DATA_DIR` set to a scratch folder, from the build in `target/p4b`, so your own Maestro and its files are not touched.

Before any of this is used: you look at every clip and picture. Shots 4 to 7 show real program and host names. Closing private programs first is the simplest staging; blurring a region in the edit is the other route.

## What capturing needs from you

1. **A yes to open the Maestro window on your desktop**, each time it is opened. It has to be on screen to be recorded.
2. **Someone at the mouse for shots 5, 6, 9 and 10.** Maestro has no way to click its own buttons, so you click while the recorder runs, or you record the screen yourself. I start and stop the recorder.
3. **About ten cents of Claude Code use** for the staged session in shots 8 to 10. Earlier runs on Haiku cost about $0.07 by the tool's own estimate.

## Edit plan

Tools, all checked on this machine:

- **ffmpeg** (2024-02-04 full build, at `C:\ffmpeg\bin`): has `gdigrab` to record one window by its title, `libx264` and `aac` to encode, `drawtext` and `ass` for captions, `xfade` and `zoompan` for cuts and push-ins.
- **ImageMagick 7.1.1**: makes the title cards and the framed background.

Steps:

1. Record each clip: `ffmpeg -f gdigrab -framerate 30 -i title="Maestro" clip.mkv`, lossless.
2. Trim each clip to its shot length.
3. Place each clip on the 1920x1080 background, with a push-in where the shot list says.
4. Build the title cards as stills and hold them for their length.
5. Join the shots with 8-frame cross-fades, hard cuts in shot 5.
6. Burn the captions in from one subtitle file, so wording can change without re-cutting.
7. Encode: H.264, high profile, about 12 Mbit/s, AAC 48 kHz, `+faststart`. That is what YouTube asks for.

### Sound

There is no music on this machine that is cleared for use, and I have not downloaded any. Three routes:

- **Silent with captions.** Ready as soon as the footage is.
- **A track you supply.** Give me a file you have the rights to and I cut to its beat.
- **A generated bed.** ffmpeg can synthesise a plain low pad with a pulse on the cuts. No licence question, and modest as music.

## What is deliberately left out

Router, Timeline and achievements, to keep the reel to 75 seconds. Blocking a program on the network and the stale flags, because the status page says they have not been seen working. macOS, because it has never been compiled.

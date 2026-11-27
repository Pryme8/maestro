"""Renders the sizzle reel from the shot list below.

    python reel/build.py            # writes reel/out/maestro-reel.mp4

It needs ffmpeg and ImageMagick ("magick") on the PATH. It reads footage from
reel/footage/, which git ignores: a recording of Maestro shows the programs
and hosts of whoever made it, so no clip is committed until it has been looked
at. A shot with no footage gets a stand-in card that says so, and the output
is then named maestro-reel-rough.mp4.

For shot 7, put one of these in reel/footage/: shot07.mkv, shot07.mp4 or
shot07.png. A clip is used from its start unless the shot gives "ss".
"""

import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
FOOTAGE = HERE / "footage"
WORK = HERE / "work"
OUT = HERE / "out"
SITE_IMG = HERE.parent / "site" / "img"

W, H, FPS = 1920, 1080, 30
FADE = 0.25
BOLD = "C:/Windows/Fonts/segoeuib.ttf"
REGULAR = "C:/Windows/Fonts/segoeui.ttf"
MONO = "C:/Windows/Fonts/consola.ttf"
NAVY, TEAL, AMBER, TEXT, MUTED = "#080d13", "#4fd6b8", "#f2c462", "#e8eef4", "#93a3b3"

# kind "card": words on the background. kind "shot": footage in a frame with
# a caption under it. "fallback" is a picture already public in the repository
# that stands in until the real footage exists.
SHOTS = [
    dict(n="01", kind="card", t=4.0, text="Your desktop was built for a person\nclicking through windows."),
    dict(n="02", kind="card", t=4.0, text="Now agents do the work.", colour=TEAL),
    dict(n="03", kind="card", t=4.0, text="MAESTRO", sub="The resource manager, rebuilt for agents.", colour=TEAL, logo=True),
    dict(n="04", kind="shot", t=8.0, caption="Everything running on your machine. Your agents, found by themselves.", fallback="base.png", zoom=0.06),
    dict(n="05a", kind="shot", t=1.75, ss=1, caption="Four views of the same machine.", fade=False),
    dict(n="05b", kind="shot", t=1.75, ss=1, caption="Four views of the same machine.", fade=False),
    dict(n="05c", kind="shot", t=1.75, ss=1, caption="Four views of the same machine.", fade=False),
    dict(n="05d", kind="shot", t=1.75, ss=1, caption="Four views of the same machine.", fade=False),
    dict(n="06", kind="shot", t=8.0, caption="Clear the machine for the task. Nothing pauses until you say so."),
    dict(n="07", kind="shot", t=6.0, ss=0.4, caption="See who every program is talking to.", zoom=0.04),
    dict(n="08", kind="shot", t=9.0, caption="Watch an agent work, one tool call at a time.", fallback="live-map.png", zoom=0.05),
    dict(n="09", kind="shot", t=7.0, caption="Refuse it. Allow it once, or for the session. No answer means no."),
    dict(n="10", kind="shot", t=5.0, caption="It works in a safe copy you can roll back."),
    dict(n="11", kind="card", t=6.0, text="No account.\nNo telemetry.\nNothing sent off your computer."),
    dict(n="12", kind="card", t=4.0, text="Early. Open from day one. MIT.", colour=AMBER),
    dict(n="13", kind="card", t=3.0, text="github.com/Pryme8/maestro", colour=TEAL, logo=True, mono=True),
]


def run(*args):
    done = subprocess.run([str(a) for a in args], capture_output=True, text=True)
    if done.returncode != 0:
        sys.exit(f"{args[0]} failed:\n{done.stderr[-2000:]}")


def background():
    path = WORK / "bg.png"
    run("magick", "-size", f"{W}x{H}", f"radial-gradient:#10262c-{NAVY}", path)
    return path


def words(text, font, size, colour, width=1560):
    """One block of centred text on a transparent sheet, as magick arguments."""
    return ["(", "-background", "none", "-fill", colour, "-font", font, "-pointsize", size,
            "-size", f"{width}x", "-gravity", "center", f"caption:{text}", ")"]


def card(shot, bg):
    path = WORK / f"card{shot['n']}.png"
    font = MONO if shot.get("mono") else BOLD
    size = 150 if shot.get("sub") else (72 if shot.get("mono") else 84)
    args = ["magick", bg]
    lift = 0
    if shot.get("logo"):
        args += ["(", SITE_IMG / "icon.png", "-resize", "150x150", ")", "-gravity", "center", "-geometry", "+0-190", "-composite"]
        lift = -30
    args += words(shot["text"], font, size, shot.get("colour", TEXT))
    args += ["-gravity", "center", "-geometry", f"+0{lift:+d}" if lift else "+0+0", "-composite"]
    if shot.get("sub"):
        args += words(shot["sub"], REGULAR, 50, TEXT) + ["-gravity", "center", "-geometry", "+0+120", "-composite"]
    run(*args, path)
    return path


def caption(shot, shade=False):
    """The caption sheet. With shade, a dark fade sits behind the words, for
    footage that fills the frame and so runs under them."""
    path = WORK / f"caption{shot['n']}.png"
    args = ["magick", "-size", f"{W}x{H}", "xc:none"]
    if shade:
        args += ["(", "-size", f"{W}x300", f"gradient:none-{NAVY}", ")", "-gravity", "south", "-composite",
                 "(", "-size", f"{W}x130", f"xc:{NAVY}", ")", "-gravity", "south", "-composite"]
    run(*args, *words(shot["caption"], BOLD, 46, TEXT, 1700),
        "-gravity", "south", "-geometry", "+0+46", "-composite", path)
    return path


def width_of(source):
    done = subprocess.run(["ffprobe", "-v", "error", "-select_streams", "v:0", "-show_entries", "stream=width",
                           "-of", "csv=p=0", str(source)], capture_output=True, text=True)
    return int(done.stdout.strip() or 0)


def standin(shot, bg):
    path = WORK / f"standin{shot['n']}.png"
    run("magick", bg, *words(f"SHOT {shot['n']}  ·  FOOTAGE TO CAPTURE", MONO, 44, AMBER),
        "-gravity", "center", "-geometry", "+0-40", "-composite", path)
    return path


def find(shot):
    for ext in ("mkv", "mp4", "png"):
        path = FOOTAGE / f"shot{shot['n']}.{ext}"
        if path.exists():
            return path, True
    if shot.get("fallback"):
        return SITE_IMG / shot["fallback"], False
    return None, False


def fades(shot):
    if shot.get("fade") is False:
        return ""
    return f",fade=t=in:st=0:d={FADE},fade=t=out:st={shot['t'] - FADE}:d={FADE}"


def encode(inputs, graph, shot):
    path = WORK / f"seg{shot['n']}.mp4"
    run("ffmpeg", "-y", "-hide_banner", "-loglevel", "error", *inputs, "-filter_complex", graph,
        "-map", "[v]", "-t", shot["t"], "-r", FPS, "-c:v", "libx264", "-preset", "medium", "-crf", "16",
        "-pix_fmt", "yuv420p", "-video_track_timescale", "15360", path)
    return path


def still(path, shot):
    return ["-loop", "1", "-framerate", FPS, "-t", shot["t"], "-i", path]


def render_card(shot, picture):
    return encode(still(picture, shot), f"[0]format=yuv420p{fades(shot)}[v]", shot)


def render_shot(shot, bg, source):
    """The window on the background, with a slow push in."""
    clip = source.suffix != ".png"
    first = ["-ss", shot.get("ss", 0), "-t", shot["t"], "-i", source] if clip else still(source, shot)
    frames = int(shot["t"] * FPS)
    zoom = shot.get("zoom", 0)
    # Fit inside the space above the caption, never enlarging: stretched text blurs.
    # A recording of the maximised window fills the frame from the top, and
    # the caption sits over its bottom edge on a dark fade.
    bleed = width_of(source) >= 1800
    if bleed:
        zoom = 0  # a push in would cut the window's edges off
        graph = (
            f"[1]scale='min(iw,{W})':'min(ih,{H})':force_original_aspect_ratio=decrease[win];"
            f"[0][win]overlay=x=(W-w)/2:y='max(0,(900-h)/2)'[framed];"
        )
    else:
        fit = "scale='min(iw,1560)':'min(ih,880)':force_original_aspect_ratio=decrease"
        graph = (
            f"[1]{fit},pad=iw+4:ih+4:2:2:color=0x1f2c3a[win];"
            f"[0][win]overlay=x=(W-w)/2:y=(960-h)/2+10[framed];"
        )
    if zoom:
        # Zooming a doubled picture keeps the push-in from shaking.
        graph += (
            f"[framed]scale={W * 2}:{H * 2}:flags=lanczos,"
            f"zoompan=z='1+{zoom}*on/{frames}':x='iw/2-(iw/zoom/2)':y='ih*0.45-(ih/zoom*0.45)':d=1:s={W}x{H}:fps={FPS}[moved];"
        )
    else:
        graph += "[framed]null[moved];"
    graph += f"[moved][2]overlay=0:0,format=yuv420p{fades(shot)}[v]"
    return encode(still(bg, shot) + first + still(caption(shot, bleed), shot), graph, shot)


def main():
    WORK.mkdir(exist_ok=True)
    OUT.mkdir(exist_ok=True)
    bg = background()
    segments, missing = [], []
    for shot in SHOTS:
        if shot["kind"] == "card":
            segments.append(render_card(shot, card(shot, bg)))
            continue
        source, real = find(shot)
        if not real:
            missing.append(shot["n"])
        if source is None:
            source = standin(shot, bg)
            shot = dict(shot, zoom=0)
            # A stand-in fills the frame, so it is drawn as the background itself.
            segments.append(encode(still(source, shot) + still(caption(shot), shot),
                                   f"[0][1]overlay=0:0,format=yuv420p{fades(shot)}[v]", shot))
        else:
            segments.append(render_shot(shot, bg, source))
        print(f"shot {shot['n']} done", flush=True)

    listing = WORK / "segments.txt"
    listing.write_text("".join(f"file '{s.as_posix()}'\n" for s in segments), encoding="utf-8")
    name = "maestro-reel-rough.mp4" if missing else "maestro-reel.mp4"
    total = sum(s["t"] for s in SHOTS)
    # A silent stereo track, because some players and YouTube's checks expect sound.
    run("ffmpeg", "-y", "-hide_banner", "-loglevel", "error", "-f", "concat", "-safe", "0", "-i", listing,
        "-f", "lavfi", "-t", total, "-i", "anullsrc=channel_layout=stereo:sample_rate=48000",
        "-c:v", "copy", "-c:a", "aac", "-b:a", "128k", "-shortest", "-movflags", "+faststart", OUT / name)
    print(f"wrote {OUT / name}: {total:.0f} seconds")
    if missing:
        print("no real footage yet for shots: " + ", ".join(missing))


if __name__ == "__main__":
    main()

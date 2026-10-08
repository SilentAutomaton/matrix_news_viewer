#!/usr/bin/env python3
# Records the README animations on a headless sway with fake news from news.sql.
# Needs: sway, wf-recorder, grim, ffmpeg, python-pywayland, python-pillow, python-numpy.
# Usage: demo/record.py [colors|click|monitors ...]

import colorsys
import os
import signal
import sqlite3
import subprocess
import sys
import tempfile
import time
from pathlib import Path

import numpy as np
from PIL import Image

DEMO = Path(__file__).resolve().parent
BINARY = DEMO.parent / "target/release/matrix-news-viewer"
SPEEDUP = 1.5
# Outputs render at twice the logical size and are scaled down, so glyphs stay crisp.
SCALE = 2
# Same cell as the default config, in logical pixels.
CELL = (9, 20)

# Only the request that creates the device; a pointer on the seat is enough
# for sway to route `seat cursor` clicks to the wallpaper.
VIRTUAL_POINTER_XML = """<?xml version="1.0" encoding="UTF-8"?>
<protocol name="wlr_virtual_pointer_unstable_v1">
  <interface name="zwlr_virtual_pointer_v1" version="1"/>
  <interface name="zwlr_virtual_pointer_manager_v1" version="1">
    <request name="create_virtual_pointer">
      <arg name="seat" type="object" interface="wl_seat" allow-null="true"/>
      <arg name="id" type="new_id" interface="zwlr_virtual_pointer_v1"/>
    </request>
  </interface>
</protocol>
"""

VIRTUAL_POINTER_CLIENT = """
import sys
sys.path.insert(0, sys.argv[1])
from pywayland.client import Display
from protocols.wlr_virtual_pointer_unstable_v1 import ZwlrVirtualPointerManagerV1
display = Display()
display.connect()
registry = display.get_registry()
names = {}
registry.dispatcher["global"] = lambda _, name, interface, version: names.__setitem__(interface, name)
display.roundtrip()
manager = registry.bind(names["zwlr_virtual_pointer_manager_v1"], ZwlrVirtualPointerManagerV1, 1)
manager.create_virtual_pointer(None)
display.roundtrip()
while display.dispatch(block=True) != -1:
    pass
"""


def hsl(h, s, l):
    return colorsys.hls_to_rgb(h % 1.0, l, s)


# The built-in presets of src/palette.rs written as theme.toml colours, so the
# demo can fade between them in small steps instead of one 1.2 s blend.
ORANGE = (1.0, 0.55, 0.1)
SCHEMES = {
    "theme": {"rain": (1.0, 1.0, 1.0), "cursor": (0.37, 0.37, 0.37), "text": (0.78, 0.78, 0.78)},
    **{
        name: {"rain": hsl(hue, 0.9, 0.7), "cursor": hsl(hue - 0.058, 1.0, 0.73), "text": text}
        for name, hue, text in (
            ("green", 0.3, ORANGE),
            ("amber", 0.1, hsl(0.55, 1.0, 0.6)),
            ("ice", 0.55, ORANGE),
            ("red", 0.0, hsl(0.55, 1.0, 0.6)),
        )
    },
}


def theme_toml(scheme):
    def hex_color(rgb):
        return "#" + "".join(f"{round(c * 255):02x}" for c in rgb)
    return "".join(f'{key} = "{hex_color(rgb)}"\n' for key, rgb in scheme.items()) + 'background = "#000000"\n'


class Desktop:
    # `outputs` holds (width, height, x, y) in logical pixels.
    def __init__(self, work, outputs, titles, scheme="theme"):
        self.work = work
        self.config = work / "config" / "matrix-news-viewer"
        self.config.mkdir(parents=True)
        self.theme(SCHEMES[scheme])
        self.database(titles)
        self.write_config()
        runtime = Path(os.environ["XDG_RUNTIME_DIR"])
        before = set(runtime.glob("wayland-*"))
        env = {k: v for k, v in os.environ.items() if k not in ("WAYLAND_DISPLAY", "DISPLAY", "SWAYSOCK")}
        env |= {"WLR_BACKENDS": "headless", "WLR_RENDERER": "gles2", "WLR_HEADLESS_OUTPUTS": str(len(outputs)),
                "WLR_RENDER_DRM_DEVICE": str(min(Path("/dev/dri").glob("renderD*")))}
        self.sway = subprocess.Popen(["sway", "-c", "/dev/null"], env=env, stderr=subprocess.DEVNULL)
        sockets = set()
        while not any(s.suffix != ".lock" for s in sockets):
            time.sleep(0.2)
            sockets = set(runtime.glob("wayland-*")) - before
        socket = next(s for s in sockets if s.suffix != ".lock")
        self.env = os.environ | {
            "WAYLAND_DISPLAY": socket.name,
            "SWAYSOCK": str(runtime / f"sway-ipc.{os.getuid()}.{self.sway.pid}.sock"),
            "XDG_CONFIG_HOME": str(work / "config"),
        }
        time.sleep(1)
        self.outputs = outputs
        self.names = [f"HEADLESS-{i + 1}" for i in range(len(outputs))]
        for name, (width, height, x, y) in zip(self.names, outputs):
            self.swaymsg("output", name, "mode", f"{width * SCALE}x{height * SCALE}", "scale", str(SCALE),
                         "pos", str(x), str(y))
        self.swaymsg("seat", "seat0", "hide_cursor", "2500")
        self.pointer = self.start_pointer()
        self.wallpaper = subprocess.Popen([BINARY], env=self.env)
        self.clicked = []

    def theme(self, scheme):
        (self.config / "theme.toml").write_text(theme_toml(scheme))

    def database(self, titles):
        db = sqlite3.connect(self.work / "news.db")
        db.executescript((DEMO / "news.sql").read_text())
        db.execute(f"DELETE FROM rss_item WHERE title NOT IN ({','.join('?' * len(titles))})", titles)
        db.commit()
        db.close()

    def write_config(self):
        (self.config / "config.toml").write_text(f"""\
[palette]
preset = "theme"

[rain]
density = 0.3
fall_speed = 0.05
cycle_speed = 0.6
skip_intro = true

[text]
interval_min = 1.0
interval_max = 3.0
dwell = 6.0

[feed]
db = "{self.work / 'news.db'}"
window_hours = 1000000.0
""")

    def start_pointer(self):
        xml = self.work / "virtual-pointer.xml"
        xml.write_text(VIRTUAL_POINTER_XML)
        subprocess.run(
            [sys.executable, "-m", "pywayland.scanner", "-i", "/usr/share/wayland/wayland.xml", str(xml),
             "-o", str(self.work / "protocols")],
            check=True, stderr=subprocess.DEVNULL,
        )
        return subprocess.Popen([sys.executable, "-c", VIRTUAL_POINTER_CLIENT, str(self.work)], env=self.env)

    def swaymsg(self, *args):
        subprocess.run(["swaymsg", *args], env=self.env, check=True, stdout=subprocess.DEVNULL)

    def set(self, key, value):
        subprocess.run([BINARY, "set", key, value], env=self.env, check=True)

    # Fades to another scheme through small theme.toml steps, about one per config poll.
    def fade(self, start, end, seconds):
        steps = max(1, round(seconds / 2))
        for i in range(1, steps + 1):
            k = i / steps
            self.theme({key: tuple(a + (b - a) * k for a, b in zip(start[key], end[key])) for key in start})
            time.sleep(seconds / steps)

    def shot(self):
        path = self.work / "shot.png"
        width, height, _, _ = self.outputs[0]
        subprocess.run(["grim", "-o", self.names[0], str(path)], env=self.env, check=True)
        return np.asarray(Image.open(path).convert("L").resize((width, height))).astype(int)

    # A headline is a row of lit cells that stay the same while the rain around them changes.
    def headlines(self):
        frames = [self.shot()]
        for _ in range(2):
            time.sleep(0.7)
            frames.append(self.shot())
        lit = np.all([f > 120 for f in frames], axis=0) & (np.ptp(frames, axis=0) < 12)
        rows, cols = lit.shape[0] // CELL[1], lit.shape[1] // CELL[0]
        cells = lit[:rows * CELL[1], :cols * CELL[0]].reshape(rows, CELL[1], cols, CELL[0]).sum(axis=(1, 3)) > 4
        found = []
        for row in range(rows):
            best, start, end = (0, 0), None, None
            for col in range(cols + 2):
                if col < cols and cells[row, col]:
                    start = col if start is None else start
                    end = col
                elif start is not None and col - end > 1:
                    best = max(best, (end - start + 1, start))
                    start = None
            if best[0] >= 6:
                found.append((best[0], row, best[1]))
        return [((start + length / 2) * CELL[0], (row + 0.5) * CELL[1]) for length, row, start in sorted(found, reverse=True)]

    # Clicks the longest headline that was not clicked before.
    def click_headline(self):
        fresh = []
        while not fresh:
            fresh = [p for p in self.headlines() if all(abs(p[1] - q[1]) > CELL[1] * 2 for q in self.clicked)]
        x, y = fresh[0]
        self.clicked.append((x, y))
        self.swaymsg("seat", "seat0", "cursor", "set", str(int(x)), str(int(y)))
        self.swaymsg("seat", "seat0", "cursor", "press", "button1")
        self.swaymsg("seat", "seat0", "cursor", "release", "button1")

    def record(self):
        return [
            subprocess.Popen(["wf-recorder", "-y", "-o", name, "-r", "30", "-c", "libx264rgb",
                              "-f", str(self.work / f"{name}.mp4")],
                             env=self.env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            for name in self.names
        ]

    def stop(self, recorders):
        for recorder in recorders:
            recorder.send_signal(signal.SIGINT)
            recorder.wait()
        return [self.work / f"{name}.mp4" for name in self.names]

    def close(self):
        for process in (self.wallpaper, self.pointer, self.sway):
            process.terminate()
            process.wait()


# Several outputs are placed on a dark desk with a gap between them, like real monitors.
def to_webp(videos, outputs, webp):
    gap = 12
    left, top = min(o[2] for o in outputs), min(o[3] for o in outputs)
    place = [(x - left + gap * (i + 1), y - top + gap) for i, (_, _, x, y) in enumerate(outputs)]
    inputs = [arg for video in videos for arg in ("-i", str(video))]
    scaled = ";".join(f"[{i}:v]scale={w}:{h}:flags=lanczos[s{i}]" for i, (w, h, _, _) in enumerate(outputs))
    if len(videos) == 1:
        graph = f"{scaled};[s0]null"
    else:
        width = max(px + w for (px, _), (w, _, _, _) in zip(place, outputs)) + gap
        height = max(py + h for (_, py), (_, h, _, _) in zip(place, outputs)) + gap
        graph = f"{scaled};color=c=0x16161a:s={width}x{height}:r=30[o]"
        for i, (px, py) in enumerate(place):
            graph += f";[o][s{i}]overlay={px}:{py}:shortest=1[o]"
        graph += ";[o]null"
    graph += f",setpts=PTS/{SPEEDUP},fps=30"
    subprocess.run(["ffmpeg", "-y", "-loglevel", "error", *inputs, "-filter_complex", graph,
                    "-c:v", "libwebp_anim", "-quality", "75", "-loop", "0", str(webp)], check=True)
    print(f"{webp.name}: {webp.stat().st_size / 1e6:.1f} MB")


def colors(work):
    desktop = Desktop(work, [(800, 450, 0, 0)], [
        "AI cured cancer", "All wars are over now", "Never gonna give you up",
        "Котик перешёл дорогу", "Понедельник отменён навсегда", "Кофе признан овощем",
    ])
    time.sleep(4)
    recorders = desktop.record()
    time.sleep(8)
    order = ["theme", "green", "amber", "ice", "red", "theme"]
    for start, end in zip(order, order[1:]):
        desktop.fade(SCHEMES[start], SCHEMES[end], 6)
        time.sleep(4)
    videos = desktop.stop(recorders)
    desktop.close()
    to_webp(videos, desktop.outputs, DEMO / "colors.webp")


def click(work):
    desktop = Desktop(work, [(800, 450, 0, 0)], ["AI cured cancer", "Котик перешёл дорогу", "Never gonna give you up"])
    time.sleep(2)
    recorders = desktop.record()
    desktop.click_headline()
    time.sleep(4)
    desktop.click_headline()
    time.sleep(14)
    videos = desktop.stop(recorders)
    desktop.close()
    to_webp(videos, desktop.outputs, DEMO / "click.webp")


def monitors(work):
    desktop = Desktop(work, [(560, 315, 0, 40), (640, 360, 560, 0)], [
        "Bug-free release shipped on Friday", "There is no spoon shortage",
        "Wi-Fi теперь бесплатный везде", "Понедельник отменён навсегда",
    ], scheme="green")
    time.sleep(4)
    recorders = desktop.record()
    time.sleep(10)
    desktop.set("rain.density", "0.6")
    desktop.set("rain.fall_speed", "0.12")
    time.sleep(14)
    videos = desktop.stop(recorders)
    desktop.close()
    to_webp(videos, desktop.outputs, DEMO / "monitors.webp")


SCENES = {"colors": colors, "click": click, "monitors": monitors}

for scene in sys.argv[1:] or SCENES:
    with tempfile.TemporaryDirectory(prefix="mnv-demo-") as work:
        SCENES[scene](Path(work))

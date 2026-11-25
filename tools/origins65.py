#!/usr/bin/env python3
"""
HyperX Alloy Origins 65 (03f0:038f) lighting driver for Linux.

Effects are written as layer specs, top layer first:
    NAME[:key=value,...]
      speed=MS           cycle length (each effect has NGENUITY's range, see `effects`)
      color=RRGGBB|name  single color, or color=random
      colors=C1/C2/...   gradient / color list (wave, cycle, swipe)
      keys=w+a+s+d       restrict the layer to some keys (see `keys`)
      opacity=0..100     layer opacity
      angle=DEG          wave direction (default 180)
      direction=D        swipe: left|right|up|down

Examples:
    origins65.py live wave
    origins65.py live explosion:color=random wave:speed=4000
    origins65.py live solid:color=ff0000,keys=w+a+s+d breathing:color=0040ff
    origins65.py save --slot 0 wave:speed=3000 fade:color=ffffff
    origins65.py brightness 50
"""
import argparse
import json
import os
import signal
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from o65.device import ONBOARD_TRIGGERS, SLOTS, Origins65  # noqa: E402
from o65.effects import EFFECTS, Engine, render_sync_frames  # noqa: E402
from o65.layout import BY_NAME, BY_USAGE, KEYS, NUM_LEDS  # noqa: E402

NAMED = {"red": (255, 0, 0), "green": (0, 255, 0), "blue": (0, 0, 255), "white": (255, 255, 255),
         "black": (0, 0, 0), "off": (0, 0, 0), "yellow": (255, 200, 0), "cyan": (0, 255, 255),
         "magenta": (255, 0, 255), "purple": (128, 0, 255), "orange": (255, 80, 0), "pink": (255, 60, 120)}
SYNC_FPS = 20                    # KeyboardAlloyOrigins65.FramePerSecond
SYNC_INTERVAL = 1000 // SYNC_FPS - 16  # value NGENUITY stores as the frame interval


def color(s):
    s = s.strip().lower()
    if s in NAMED:
        return NAMED[s]
    s = s.lstrip("#")
    if len(s) != 6:
        raise ValueError(f"bad color {s!r}")
    return tuple(int(s[i:i + 2], 16) for i in (0, 2, 4))


def parse_layer(spec):
    name, _, rest = spec.partition(":")
    name = name.lower()
    if name not in EFFECTS:
        raise SystemExit(f"unknown effect {name!r}; run `origins65.py effects`")
    kw = {}
    for item in filter(None, rest.split(",")):
        k, _, v = item.partition("=")
        k = k.strip().lower()
        if k == "color":
            if v.lower() == "random":
                kw["random_color"] = True
            else:
                kw["color"] = color(v)
        elif k == "colors":
            kw["colors"] = [color(c) for c in v.split("/")]
        elif k == "keys":
            names = [n.lower() for n in v.split("+")]
            bad = [n for n in names if n not in BY_NAME]
            if bad:
                raise SystemExit(f"unknown keys {bad}; run `origins65.py keys`")
            kw["keys"] = set(names)
        elif k in ("speed", "opacity"):
            kw[k] = int(v)
        elif k == "angle":
            kw["angle"] = float(v)
        elif k == "direction":
            kw["direction"] = v.lower()
        else:
            raise SystemExit(f"unknown option {k!r} in {spec!r}")
    cls = EFFECTS[name]
    if "speed" in kw and not cls.min_speed <= kw["speed"] <= cls.max_speed:
        print(f"note: {name} speed {kw['speed']} is outside NGENUITY's range "
              f"{cls.min_speed}..{cls.max_speed} ms", file=sys.stderr)
    return cls(**kw)


def scaled(frame, k):
    return [(int(r * k), int(g * k), int(b * k)) for r, g, b in frame]


# ---- commands ---------------------------------------------------------------------------------------
def _profile_dict(prof):
    d = prof.disabled_combos
    names = [n for bit, n in ((1, "Alt+Tab"), (2, "Alt+F4"), (4, "Shift+Tab"), (8, "Ctrl+Esc")) if d & bit]
    return {
        "slot": prof.slot,
        "brightness": prof.brightness,
        "brightness_percent": round(prof.brightness / 2.55),
        "game_mode": prof.game_mode,
        "game_mode_blocks": names,
        "fnlock_key": prof.raw[15],
        "fnlock_color": prof.raw[16:19].hex(),
    }


def cmd_info(kb, args):
    info = kb.device_info()
    current = kb.read_profile()
    if getattr(args, "json", False):
        slots = []
        for s in range(SLOTS):
            try:
                slots.append({"index": s, "profile": _profile_dict(kb.read_profile(slot=s)), "error": None})
            except Exception as e:  # noqa: BLE001
                slots.append({"index": s, "profile": None, "error": str(e)})
        payload = {
            **info,
            "current": _profile_dict(current),
            "slots": slots,
        }
        print(json.dumps(payload))
        return
    print(f"HyperX Alloy Origins 65  {info['vid']}:{info['pid']}  firmware {info['firmware']}  "
          f"country code {info['country_code']}")
    print("current profile:", current.describe())
    for s in range(SLOTS):
        try:
            print(f"slot {s} profile:  ", kb.read_profile(slot=s).describe())
        except Exception as e:  # noqa: BLE001
            print(f"slot {s} profile:   unreadable ({e})")


def cmd_brightness(kb, args):
    prof = kb.set_brightness(args.percent)
    print("brightness set:", prof.describe())


def cmd_slot(kb, args):
    kb.select_slot(args.slot)
    print("active onboard slot:", kb.read_profile().slot)


def cmd_live(kb, args):
    layers = [parse_layer(s) for s in args.layers]
    engine = Engine(layers)
    stop = []
    signal.signal(signal.SIGINT, lambda *_: stop.append(1))
    signal.signal(signal.SIGTERM, lambda *_: stop.append(1))
    period = 1.0 / args.fps
    t0 = time.monotonic()
    print("streaming (Ctrl+C to stop; the keyboard returns to its onboard lighting afterwards)")
    while not stop:
        start = time.monotonic()
        if args.duration and start - t0 >= args.duration:
            break
        for rep in kb.read_notifications():
            if len(rep) >= 4 and rep[0] == 5 and rep[1] == 4 and rep[3] == 1:
                key = BY_USAGE.get(rep[2])
                if key:
                    engine.key_pressed(key)
            elif len(rep) >= 3 and rep[0] == 5 and rep[1] == 8:
                print(f"hardware brightness changed to {round(rep[2] / 2.55)}%")
        kb.send_direct_frame(scaled(engine.frame(), args.brightness / 100))
        time.sleep(max(0.0, period - (time.monotonic() - start)))


def cmd_save(kb, args):
    layers = [parse_layer(s) for s in args.layers]
    looped = [l for l in layers if not l.triggered]
    triggers = [l for l in layers if l.triggered]
    if len(triggers) > 1:
        raise SystemExit("only one key-press effect can be stored onboard")
    trigger = None
    if triggers:
        t = triggers[0]
        if t.name not in ONBOARD_TRIGGERS:
            raise SystemExit(f"{t.name} can't run onboard; the firmware supports: {', '.join(ONBOARD_TRIGGERS)}")
        sp = (min(max(t.speed, t.min_speed), t.max_speed) - t.min_speed) / (t.max_speed - t.min_speed) * 255
        trigger = {"kind": t.name, "colors": t.colors or [t.color], "random": t.random_color, "speed": int(sp)}
    frames = render_sync_frames(looped, fps=SYNC_FPS, max_frames=120)
    frames = scaled_frames = [scaled(f, args.brightness / 100) for f in frames]
    print(f"rendered {len(scaled_frames)} frames ({len(frames) / SYNC_FPS:.1f} s loop at {SYNC_FPS} fps)"
          + (f", onboard key-press effect: {trigger['kind']}" if trigger else ""))

    def progress(done, total):
        print(f"\ruploading to slot {args.slot}: {done * 100 // total:3d}%", end="", flush=True)

    kb.upload_animation(args.slot, frames, SYNC_INTERVAL, trigger, progress)
    print()
    try:  # store the slot number in the slot's own profile, as NGENUITY does
        prof = kb.read_profile(slot=args.slot)
        prof.slot = args.slot
        kb.write_profile(prof, slot=args.slot)
    except Exception as e:  # noqa: BLE001
        print(f"note: could not update slot {args.slot} profile ({e})", file=sys.stderr)
    if not args.no_activate:
        kb.select_slot(args.slot, force_reload=True)
    elif kb.active_slot() == args.slot:
        kb.select_slot(args.slot, force_reload=True)  # slot is showing: reload so the new animation plays
    print(f"saved to onboard slot {args.slot}" + ("" if args.no_activate else " and activated"))


def cmd_identify(kb, args):
    order = sorted(KEYS, key=lambda k: (k.row, k.col))
    stop = []
    signal.signal(signal.SIGINT, lambda *_: stop.append(1))
    for k in order:
        if stop:
            break
        print(f"LED {k.led:2d}  row {k.row} col {k.col:2d}  {k.name}", flush=True)
        end = time.monotonic() + args.step
        while time.monotonic() < end and not stop:
            frame = [(0, 0, 25)] * NUM_LEDS
            frame[k.led] = (255, 255, 255)
            kb.send_direct_frame(frame)
            time.sleep(0.03)


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    inf = sub.add_parser("info", help="device, firmware and profile information")
    inf.add_argument("--json", action="store_true", help="output machine-readable JSON")
    b = sub.add_parser("brightness", help="set hardware brightness (0-100)")
    b.add_argument("percent", type=int)
    s = sub.add_parser("slot", help="activate onboard slot 0-2")
    s.add_argument("slot", type=int, choices=range(SLOTS))
    lv = sub.add_parser("live", help="stream effects (key-press effects react to typing)")
    lv.add_argument("layers", nargs="+")
    lv.add_argument("--fps", type=float, default=30)
    lv.add_argument("--brightness", type=float, default=100, help="software brightness 0-100")
    lv.add_argument("--duration", type=float, default=0)
    sv = sub.add_parser("save", help="store effects in onboard memory (runs without this tool)")
    sv.add_argument("layers", nargs="+")
    sv.add_argument("--slot", type=int, choices=range(SLOTS), required=True)
    sv.add_argument("--brightness", type=float, default=100)
    sv.add_argument("--no-activate", action="store_true")
    i = sub.add_parser("identify", help="light every key in turn")
    i.add_argument("--step", type=float, default=0.5)
    sub.add_parser("keys", help="list key names")
    sub.add_parser("effects", help="list effects")
    args = ap.parse_args()

    if args.cmd == "keys":
        print(" ".join(k.name for k in KEYS))
        return
    if args.cmd == "effects":
        for name, cls in EFFECTS.items():
            kind = "key-press" + (" (onboard ok)" if name in ONBOARD_TRIGGERS else " (live only)") \
                if cls.triggered else "looped"
            print(f"{name:10s} {kind:22s} speed {cls.min_speed}-{cls.max_speed} ms, default {cls.default_speed}")
        return
    with Origins65() as kb:
        {"info": cmd_info, "brightness": cmd_brightness, "slot": cmd_slot, "live": cmd_live,
         "save": cmd_save, "identify": cmd_identify}[args.cmd](kb, args)


if __name__ == "__main__":
    main()

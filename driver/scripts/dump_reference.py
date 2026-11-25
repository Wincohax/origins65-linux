#!/usr/bin/env python3
"""Dump deterministic reference outputs from the Python driver (tools/o65) as JSON for Rust parity tests."""
import json
import os
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
sys.path.insert(0, os.path.join(REPO_ROOT, "tools"))

import origins65 as cli  # noqa: E402
from o65 import device, effects, layout  # noqa: E402


DETERMINISTIC_CASES = [
    {"name": "solid_red", "specs": ["solid:color=ff0000"], "brightness": 100.0},
    {"name": "solid_named_cyan", "specs": ["solid:color=cyan"], "brightness": 100.0},
    {"name": "solid_wasd_mask", "specs": ["solid:color=00ff88,keys=w+a+s+d"], "brightness": 100.0},
    {"name": "solid_half_opacity", "specs": ["solid:color=ff00ff,opacity=50"], "brightness": 100.0},
    {"name": "breathing_default", "specs": ["breathing:color=0040ff"], "brightness": 100.0},
    {"name": "breathing_fast", "specs": ["breathing:color=ff8020,speed=500"], "brightness": 100.0},
    {"name": "breathing_slow_masked", "specs": ["breathing:color=ffffff,speed=4000,keys=esc+space+enter"], "brightness": 100.0},
    {"name": "cycle_default", "specs": ["cycle"], "brightness": 100.0},
    {"name": "cycle_custom_rgb", "specs": ["cycle:speed=1000,colors=ff0000/00ff00/0000ff"], "brightness": 100.0},
    {"name": "cycle_single_color", "specs": ["cycle:speed=3000,colors=123456"], "brightness": 100.0},
    {"name": "wave_default_180", "specs": ["wave"], "brightness": 100.0},
    {"name": "wave_angle_0", "specs": ["wave:angle=0,speed=2000"], "brightness": 100.0},
    {"name": "wave_angle_45", "specs": ["wave:angle=45,speed=1500"], "brightness": 100.0},
    {"name": "wave_angle_90", "specs": ["wave:angle=90,speed=3000"], "brightness": 100.0},
    {"name": "wave_angle_135", "specs": ["wave:angle=135,speed=4000"], "brightness": 100.0},
    {"name": "wave_angle_270", "specs": ["wave:angle=270,speed=2500"], "brightness": 100.0},
    {"name": "wave_angle_315", "specs": ["wave:angle=315,speed=6000"], "brightness": 100.0},
    {"name": "wave_two_colors", "specs": ["wave:angle=180,speed=1000,colors=ff0000/0000ff"], "brightness": 100.0},
    {"name": "wave_single_color", "specs": ["wave:angle=60,speed=2000,colors=00ff00"], "brightness": 100.0},
    {"name": "swipe_right_default", "specs": ["swipe:direction=right,speed=2000"], "brightness": 100.0},
    {"name": "swipe_left_custom", "specs": ["swipe:direction=left,speed=1500,colors=ff0000/0000ff"], "brightness": 100.0},
    {"name": "swipe_up", "specs": ["swipe:direction=up,speed=1000,colors=00ff00/ffff00"], "brightness": 100.0},
    {"name": "swipe_down_single_color", "specs": ["swipe:direction=down,speed=2500,colors=ff00ff"], "brightness": 100.0},
    {"name": "stack_solid_mask_over_wave", "specs": ["solid:color=ff0000,keys=w+a+s+d", "wave:speed=3000"], "brightness": 100.0},
    {"name": "stack_alpha_over_breathing", "specs": ["solid:color=ffffff,opacity=50,keys=w+a+s+d+space", "breathing:color=0040ff,speed=2000"], "brightness": 100.0},
    {"name": "stack_fit_wave_cycle", "specs": ["wave:speed=2500", "cycle:speed=2000"], "brightness": 100.0},
    {"name": "stack_fit_breathing_wave", "specs": ["breathing:speed=1500", "wave:speed=4000"], "brightness": 100.0},
    {"name": "stack_fit_swipe_wave", "specs": ["swipe:speed=1800", "wave:speed=2500"], "brightness": 100.0},
    {"name": "stack_capped_120", "specs": ["wave:speed=6000", "cycle:speed=4000"], "brightness": 100.0},
    {"name": "trigger_only_blank", "specs": ["fade:color=ffffff"], "brightness": 100.0},
    {"name": "scaled_brightness_50", "specs": ["solid:color=ff0000,keys=esc,opacity=40", "wave:speed=2000"], "brightness": 50.0},
    {"name": "scaled_brightness_73", "specs": ["cycle:speed=1500", "solid:color=102030,opacity=35"], "brightness": 73.0},
]


class MockUsbDev:
    def __init__(self, initial_profiles=None):
        self.sent = []
        self.last_cmd = 0
        self.profiles = initial_profiles or {}
        self.next_get_override = None

    def hid_set_report(self, intf, report_type, report_id, buf):
        assert intf == device.INTERFACE
        assert report_type == 3 and report_id == 0
        raw = bytes(buf)
        assert len(raw) == 64
        self.sent.append(list(raw))
        if raw[0] == 0x04:
            self.last_cmd = raw[1]

    def control(self, req_type, req, value, index, length, timeout=1000):
        assert req_type == 0xA1 and req == 0x01 and value == 0x0300 and index == device.INTERFACE and length == 64
        if self.next_get_override is not None:
            resp = self.next_get_override
            self.next_get_override = None
            return resp
        # Command status ack
        resp = bytearray(64)
        resp[1] = self.last_cmd
        resp[3] = 1
        # If the command is a read profile, queue the profile payload for the next GET
        if self.last_cmd in (0x56, 0x66, 0x26, 0x36, 0x46):
            prof = self.profiles.get(self.last_cmd, bytes(64))
            self.next_get_override = prof
        return bytes(resp)


def make_mock_kb(initial_profiles=None):
    kb = device.Origins65.__new__(device.Origins65)
    kb.dev = MockUsbDev(initial_profiles)
    kb.nfd = None
    kb.pending_events = []
    kb._wait_ack = lambda timeout=0.5: True
    return kb


def dump_protocol_reference():
    out = {}

    # 1. Direct frame
    kb = make_mock_kb()
    frame = [((i * 7) & 0xFF, (i * 13) & 0xFF, (i * 29) & 0xFF) for i in range(layout.NUM_LEDS)]
    kb.send_direct_frame(frame)
    out["direct_frame"] = kb.dev.sent

    # 2. Profile write preserving unknown bytes (current, slot 0..2, app_mode)
    base_raw = bytearray(range(64))
    base_raw[0] = 1
    base_raw[4] = 1
    base_raw[8] = 5
    base_raw[12] = 200
    base_raw[62] = 0x11
    base_raw[63] = 0x22
    prof = device.Profile(base_raw)

    for label, slot, app_mode in [
        ("write_profile_current", None, False),
        ("write_profile_slot0", 0, False),
        ("write_profile_slot1", 1, False),
        ("write_profile_slot2", 2, False),
        ("write_profile_app", None, True),
    ]:
        kb = make_mock_kb()
        kb.write_profile(prof, slot=slot, app_mode=app_mode)
        out[label] = kb.dev.sent

    # 3. Brightness values (testing round-half-to-even edge cases)
    out["brightness_bytes"] = {
        str(p): round(max(0, min(100, p)) * 2.55) for p in (-5, 0, 10, 25, 30, 50, 75, 90, 100, 120)
    }
    out["brightness_percents"] = {
        str(b): round(b / 2.55) for b in (0, 1, 25, 26, 76, 77, 127, 128, 204, 254, 255)
    }

    # 4. select_slot with force_reload on active slot 1 (must hop 1 -> 2 -> 1)
    cur_raw = bytearray(64)
    cur_raw[0] = 1
    cur_raw[12] = 180
    cur_raw[33] = 0xAB
    kb = make_mock_kb({0x56: bytes(cur_raw)})
    kb.select_slot(1, force_reload=True)
    out["select_slot_force_reload_active"] = kb.dev.sent

    kb = make_mock_kb({0x56: bytes(cur_raw)})
    kb.select_slot(2, force_reload=True)
    out["select_slot_force_reload_different"] = kb.dev.sent

    # 5. upload_animation cases
    f0 = [((i * 3) & 0xFF, (i * 5) & 0xFF, (i * 11) & 0xFF) for i in range(layout.NUM_LEDS)]
    f1 = [((255 - i) & 0xFF, (i * 2) & 0xFF, (i * 4) & 0xFF) for i in range(layout.NUM_LEDS)]

    kb = make_mock_kb()
    kb.upload_animation(0, [f0, f1], cli.SYNC_INTERVAL, trigger=None)
    out["upload_slot0_no_trigger"] = kb.dev.sent

    kb = make_mock_kb()
    kb.upload_animation(
        1,
        [f0],
        cli.SYNC_INTERVAL,
        trigger={"kind": "fade", "colors": [(10, 20, 30)], "random": False, "speed": 63},
    )
    out["upload_slot1_fade"] = kb.dev.sent

    kb = make_mock_kb()
    kb.upload_animation(
        2,
        [f0, f1],
        cli.SYNC_INTERVAL,
        trigger={"kind": "explosion", "colors": [(255, 0, 128), (0, 255, 64)], "random": True, "speed": 200},
    )
    out["upload_slot2_explosion_random"] = kb.dev.sent

    kb = make_mock_kb()
    kb.upload_animation(
        0,
        [f1],
        cli.SYNC_INTERVAL,
        trigger={"kind": "flame", "colors": [(242, 0, 0)], "random": False, "speed": 128},
    )
    out["upload_slot0_flame"] = kb.dev.sent

    return out


def main():
    det = []
    for case in DETERMINISTIC_CASES:
        layers = [cli.parse_layer(s) for s in case["specs"]]
        frames = effects.render_sync_frames(layers, fps=cli.SYNC_FPS, max_frames=120)
        if case["brightness"] != 100.0:
            frames = [cli.scaled(f, case["brightness"] / 100.0) for f in frames]
        det.append({
            "name": case["name"],
            "specs": case["specs"],
            "brightness": case["brightness"],
            "frames": [[list(rgb) for rgb in frame] for frame in frames],
        })

    effect_lines = []
    for name, cls in effects.EFFECTS.items():
        kind = (
            "key-press" + (" (onboard ok)" if name in device.ONBOARD_TRIGGERS else " (live only)")
            if cls.triggered
            else "looped"
        )
        effect_lines.append(
            f"{name:10s} {kind:22s} speed {cls.min_speed}-{cls.max_speed} ms, default {cls.default_speed}"
        )

    payload = {
        "keys": [
            {"name": k.name, "usage": k.usage, "row": k.row, "col": k.col, "led": k.led}
            for k in layout.KEYS
        ],
        "effect_lines": effect_lines,
        "deterministic_cases": det,
        "protocol": dump_protocol_reference(),
    }
    json.dump(payload, sys.stdout)


if __name__ == "__main__":
    main()

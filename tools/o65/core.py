"""HyperX Alloy Origins Core (TKL) command protocol (NGENUITY 2.39 KeyboardAlloyOriginsCore + CMDKBAlloyOriginsCore*).

Not verified on hardware: written from NGENUITY and cross-checked against OpenRGB's Origins Core controller.

Transport
  * USB 03f0:098f (HP) or 0951:16e6 (Kingston), interface 2 (NGENUITY "MI_02"). usbhid binds it, so the
    hidraw node is used directly: 64-byte output reports (report ID 0) out, 64-byte input reports in.
  * Every reply is an input report whose byte 0 echoes the command. Key events arrive on the same
    interface:  73 .. .. .. <usage> <1|0>  (usage 0xE8 = Fn),  71 .. .. .. <0|1>  game mode.

Command frame (payload offsets; NGENUITY's are +1 because its buffer starts with the report ID):
    [0]=cmd [1]=profile (1..3) or sub-index [2]=sub-command [3]=data length [4..]=data
  0x10  device info (country code at [56])       0x14  game-mode info of a profile ([4..8])
  0x15  active profile ([4], 1..3)               0x27  brightness ([4], 0..255)
  0x93  set the preset GUID of a profile         0x95  activate profile ([4] = 1..3)
  0xA0  effect info of a profile (onboard animation + key-press trigger)
  0xA2  live frame: 7 packets ([1] = 0..6, 60 + 20 bytes of a 380-byte buffer)
  0xA4  begin (0x55) / end (0xAA) an animation upload   0xA5  animation frame packet
  0xA7  set brightness ([3]=1, [4]=0..255)
  (0x20/0x30/0x31/0xB0/0xB1 read/write LED info, key matrix and macros: not used here)

Frame buffer: 380 bytes in 48-byte segments of 12 LEDs. LED index i lands at
    pos = (i // 12) * 48 + ((i // 6) & 1) * 8 + i % 6,   G at pos, R at pos + 16, B at pos + 32.
"""
import glob
import os
import select
import time
import uuid

from .device import Profile, ProtocolError
from .layout import NUM_LEDS
from .models import ORIGINS_CORE

SLOTS = 3
INTERFACE = 2
FRAME_BYTES = 380
ONBOARD_TRIGGERS = {"fade": 4, "explosion": 5, "flame": 6}
FN_USAGE = 0xE8
REPLY_TIMEOUT = 1.0        # NGENUITY waits 1 s for a reply (and carries on without one)
MAX_MISSED_REPLIES = 5     # give up instead of waiting a second per packet on a silent keyboard


def find_hidraw():
    """(path, vid, pid, bcdDevice) of the Origins Core's interface 2 hidraw node."""
    ids = {f"HID_ID=0003:{vid:08X}:{pid:08X}": (vid, pid) for vid, pid in ORIGINS_CORE.usb_ids}
    for node in sorted(glob.glob("/sys/class/hidraw/hidraw*")):
        try:
            uevent = open(node + "/device/uevent").read()
        except OSError:
            continue
        match = next((v for k, v in ids.items() if k in uevent), None)
        if not match:
            continue
        intf_dir = os.path.dirname(os.path.realpath(node + "/device"))
        if not intf_dir.endswith(f":1.{INTERFACE}"):
            continue
        try:
            bcd = int(open(os.path.dirname(intf_dir) + "/bcdDevice").read(), 16)
        except (OSError, ValueError):
            bcd = 0
        return "/dev/" + os.path.basename(node), match[0], match[1], bcd
    return None


def present():
    return find_hidraw() is not None


def encode_frame(colors):
    """NUM_LEDS (r, g, b) tuples -> the 380-byte G/R/B buffer of KeyboardAlloyOriginsCore.SetLightings."""
    buf = bytearray(FRAME_BYTES)
    for i, (r, g, b) in enumerate(colors[:NUM_LEDS]):
        pos = (i // 12) * 48 + ((i // 6) & 1) * 8 + i % 6
        if pos + 32 < FRAME_BYTES:
            buf[pos], buf[pos + 16], buf[pos + 32] = g, r, b
    return buf


def _chunks(buf):
    """The 7 (index, length, data) pieces NGENUITY splits a 380-byte buffer into."""
    for i in range(7):
        n = 20 if i == 6 else 60
        yield i, n, buf[i * 60:i * 60 + n]


def trigger_color_bytes(trigger):
    """Bytes [19..24] of the 0xA0 packet: colour type then colours, exactly as CMDKBAlloyOriginsCoreSync
    writes them (its two-colour case puts the second colour at [22..24], over the first one's blue)."""
    out = bytearray(6)
    colors = trigger.get("colors") or [(242, 0, 0)]
    out[1:4] = bytes(colors[0])
    if len(colors) > 1:
        out[0] = 2
        out[3:6] = bytes(colors[1])
    else:
        out[0] = 1
    if trigger.get("random"):
        out[0] = 4
    return out


class OriginsCore:
    model = ORIGINS_CORE
    SLOTS = SLOTS

    def __init__(self, path=None):
        found = find_hidraw() if path is None else (path, 0, 0, 0)
        if not found:
            ids = " or ".join(f"{v:04x}:{p:04x}" for v, p in ORIGINS_CORE.usb_ids)
            raise RuntimeError(f"{ids} not found")
        self.path, self.vid, self.pid, self.bcd = found
        self.fd = os.open(self.path, os.O_RDWR | os.O_NONBLOCK)
        self.pending = []  # input reports that were not the reply being waited for
        self.fn_down = False
        self.missed = 0

    def close(self):
        os.close(self.fd)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()

    # ---- transport -------------------------------------------------------------------------
    def _write(self, payload):
        os.write(self.fd, b"\0" + bytes(payload).ljust(64, b"\0")[:64])

    def _read(self, timeout):
        r, _, _ = select.select([self.fd], [], [], max(0.0, timeout))
        if not r:
            return None
        try:
            return os.read(self.fd, 64)
        except BlockingIOError:
            return None

    def _reply(self, cmd, timeout=REPLY_TIMEOUT):
        end = time.monotonic() + timeout
        while True:
            rep = self._read(end - time.monotonic())
            if rep is None:
                self.missed += 1
                if self.missed >= MAX_MISSED_REPLIES:
                    raise ProtocolError(f"keyboard stopped answering (last command 0x{cmd:02x})")
                return None
            if rep and rep[0] == cmd:
                self.missed = 0
                return rep
            self.pending.append(rep)

    def _request(self, payload, wait=True, timeout=REPLY_TIMEOUT):
        self._write(payload)
        return self._reply(payload[0], timeout) if wait else None

    @staticmethod
    def _cmd(cmd, profile=None, sub=0, length=0):
        p = bytearray(64)
        p[0] = cmd
        if profile is not None:
            p[1], p[2] = max(1, min(3, profile)), sub
        p[3] = length
        return p

    def _query(self, payload):
        rep = self._request(payload)
        if rep is None:
            raise ProtocolError(f"no reply to command 0x{payload[0]:02x}")
        return rep

    # ---- queries -----------------------------------------------------------------------------
    def device_info(self):
        rep = self._query(self._cmd(0x10))
        return {
            "vid": f"{self.vid:04x}",
            "pid": f"{self.pid:04x}",
            "firmware": f"{self.bcd >> 8:x}.{self.bcd & 0xFF:02x}",
            "country_code": rep[56],
        }

    def active_slot(self):
        return max(0, min(SLOTS - 1, self._query(self._cmd(0x15))[4] - 1))

    def hardware_brightness(self):
        return self._query(self._cmd(0x27))[4]

    def game_mode_info(self, slot):
        p = self._cmd(0x14)
        p[1] = slot + 1
        rep = self._query(p)
        combos = sum(bit for bit, i in ((1, 4), (2, 5), (4, 6), (8, 7)) if rep[i] == 1)
        return rep[8] == 1, combos

    def read_profile(self, slot=None):
        """The Origins Core has no profile record; synthesize one so `info` prints the same fields."""
        if slot is None:
            slot = self.active_slot()
        game_mode, combos = self.game_mode_info(slot)
        prof = Profile(bytes(64))
        prof.slot = slot
        prof.game_mode = game_mode
        prof.disabled_combos = combos
        prof.brightness = self.hardware_brightness()  # one setting for the whole keyboard
        return prof

    # ---- settings ----------------------------------------------------------------------------
    def set_brightness(self, percent):
        p = self._cmd(0xA7, length=1)
        p[4] = max(0, min(100, int(percent))) * 255 // 100  # CMDKBAlloyOriginsCoreSetBrightness
        self._request(p)
        return self.read_profile()

    def _set_profile(self, slot):
        p = self._cmd(0x95, length=1)
        p[4] = slot + 1
        self._write(p)  # NGENUITY reads a feature report here instead of waiting for a reply
        time.sleep(0.05)

    def select_slot(self, slot, force_reload=False):
        if not 0 <= slot < SLOTS:
            raise ValueError("slot must be 0..2")
        if force_reload and self.active_slot() == slot:
            self._set_profile((slot + 1) % SLOTS)
        self._set_profile(slot)

    # ---- notifications -----------------------------------------------------------------------
    def events(self, timeout=0.0):
        """Parsed notifications: ("key", usage, pressed), ("brightness", percent), ("game_mode", on)."""
        reports, self.pending = self.pending, []
        end = time.monotonic() + timeout
        while True:
            rep = self._read(end - time.monotonic())
            if rep is None:
                break
            reports.append(rep)
        out = []
        for rep in reports:
            if len(rep) >= 6 and rep[0] == 0x73:
                usage, pressed = rep[4], rep[5] == 1
                if usage == FN_USAGE:
                    self.fn_down = pressed
                out.append(("key", usage, pressed))
                if self.fn_down and pressed and usage in (0x51, 0x52):  # Fn+Down / Fn+Up
                    out.append(("brightness", round(self.hardware_brightness() / 2.55)))
            elif len(rep) >= 5 and rep[0] == 0x71:
                out.append(("game_mode", rep[4] == 1))
        return out

    # ---- lighting ------------------------------------------------------------------------------
    def send_direct_frame(self, colors):
        """Volatile; the keyboard returns to its profile if frames stop for a few seconds."""
        for i, n, data in _chunks(encode_frame(colors)):
            p = self._cmd(0xA2, length=n)
            p[1] = i
            p[4:4 + n] = data
            self._write(p)

    def upload_animation(self, slot, frames, interval, trigger=None, progress=None, preset_id=None):
        """CMDKBAlloyOriginsCoreSync: store <= 100 frames in onboard profile slot 0..2.

        Leaves `slot` active (NGENUITY activates the profile before writing it).
        trigger: None or dict(kind=fade|explosion|flame, colors=[(r,g,b)] (1 or 2), random=bool, speed=0..63)
        """
        if not 0 <= slot < SLOTS:
            raise ValueError("slot must be 0..2")
        if not 1 <= len(frames) <= ORIGINS_CORE.max_sync_frames:
            raise ValueError(f"1..{ORIGINS_CORE.max_sync_frames} frames")
        board = slot + 1
        total = len(frames) * 7
        done = 0
        self._set_profile(slot)

        p = self._cmd(0x93, length=32)
        p[1] = board << 4
        p[4:20] = preset_id if preset_id is not None else uuid.uuid4().bytes_le
        self._request(p)

        p = self._cmd(0xA4, board, 0, 2)
        p[4], p[5] = 0x55, len(frames)
        self._request(p, wait=False)
        for f, frame in enumerate(frames):
            for i, n, data in _chunks(encode_frame(frame)):
                p = self._cmd(0xA5, length=n)
                p[1] = (board << 4) | i
                p[2] = f + 1
                p[4:4 + n] = data
                self._request(p)
                done += 1
                if progress:
                    progress(done, total)
        p = self._cmd(0xA4, board, 0, 2)
        p[4], p[5] = 0xAA, len(frames)
        self._request(p, wait=False)

        p = self._cmd(0xA0, board, 0, 23)
        p[4] = 9                      # custom animation
        p[5] = interval & 0xFF
        p[14] = len(frames)
        if trigger:
            p[16] = ONBOARD_TRIGGERS[trigger["kind"]]
            p[17] = min(63, trigger.get("speed", 32))
            p[19:25] = trigger_color_bytes(trigger)
        self._request(p)
        time.sleep(1.0)               # NGENUITY lets the firmware settle before the next command

    def save_animation(self, slot, frames, trigger, activate, progress=None):
        """Upload, then leave the right slot showing (the new one, or the one that was active)."""
        previous = self.active_slot()
        self.upload_animation(slot, frames, self.model.sync_interval, trigger, progress)
        if activate or previous == slot:
            self.select_slot(slot, force_reload=True)  # uploading activated it: hop so it reloads
        else:
            self._set_profile(previous)
        return None

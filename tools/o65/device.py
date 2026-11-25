"""HyperX Alloy Origins 65 command protocol (reverse engineered from NGENUITY 2.39 NGenuity2Helper.exe).

Transport
  * Commands/lighting: HID feature report, no report ID, 64 bytes, on USB interface 3
    (vendor page 0xFF90). Linux usbhid does not bind interface 3, so it is driven via usbfs.
  * Notifications: input report 5 on interface 1 (hidraw, vendor page 0xFFFF):
        05 AC AC AC ...      command ACK
        05 04 <usage> <1|0>  key press / release (usage 0xA4 = Fn)
        05 07 <0|1>          game mode changed
        05 08 <level>        brightness changed with Fn keys (0..255)
        05 09 ...            Fn-lock state

Command frame (payload offsets; NGENUITY's offsets are +1 because its buffer starts with a report ID):
    [0]=0x04 [1]=cmd [8..9]=packet count / argument      -> ACK, then GET_FEATURE: [1]==cmd and [3]==1
  0x02            commit ("end command")
  0x05            device info (country code at [24])
  0x23+16*slot    animation header for onboard slot (2 packets)
  0x26+16*slot    read profile of slot      0x27+16*slot  write profile of slot
  0x53+16*slot    upload animation frames (u16 LE packet count)
  0x56 / 0x57     read / write the *current* profile (its byte 0 selects the active slot)
  0x66 / 0x67     read / write the app-mode profile
  0xF2            direct (live) frame: 5 packets of 16 x [0x81 R G B]
  0xFD            factory reset (not exposed here)
"""
import glob
import os
import select
import time

from .layout import NUM_LEDS
from .usb import UsbDev, find_device

VID, PID = 0x03F0, 0x038F
INTERFACE = 3
SLOTS = 3
TRAILER = (0xAA, 0x55)

ONBOARD_TRIGGERS = {"fade": 4, "explosion": 5, "flame": 6}


class ProtocolError(RuntimeError):
    pass


def find_notification_hidraw():
    """hidraw node of interface 1 (the collection carrying report 5)."""
    for node in sorted(glob.glob("/sys/class/hidraw/hidraw*")):
        try:
            uevent = open(node + "/device/uevent").read()
        except OSError:
            continue
        if "HID_ID=0003:000003F0:0000038F" not in uevent:
            continue
        intf = os.path.basename(os.path.dirname(os.path.realpath(node + "/device")))
        if intf.endswith(":1.1"):
            return "/dev/" + os.path.basename(node)
    return None


class Profile:
    """64-byte profile record (ProfileAlloyOrigins65). Unknown bytes are preserved."""

    def __init__(self, raw):
        self.raw = bytearray(raw[:64].ljust(64, b"\0"))

    slot = property(lambda s: s.raw[0], lambda s, v: s.raw.__setitem__(0, v))
    game_mode = property(lambda s: s.raw[4] == 1, lambda s, v: s.raw.__setitem__(4, 1 if v else 0))
    disabled_combos = property(lambda s: s.raw[8], lambda s, v: s.raw.__setitem__(8, v))
    brightness = property(lambda s: s.raw[12], lambda s, v: s.raw.__setitem__(12, max(0, min(255, int(v)))))

    def describe(self):
        d = self.disabled_combos
        names = [n for bit, n in ((1, "Alt+Tab"), (2, "Alt+F4"), (4, "Shift+Tab"), (8, "Ctrl+Esc")) if d & bit]
        return (f"slot={self.slot} brightness={self.brightness} ({round(self.brightness / 2.55)}%) "
                f"game_mode={'on' if self.game_mode else 'off'} game_mode_blocks={'/'.join(names) or 'none'} "
                f"fnlock_key={self.raw[15]} fnlock_color={self.raw[16:19].hex()}")


class Origins65:
    def __init__(self):
        self.dev = UsbDev(find_device(VID, PID))
        self.dev.claim(INTERFACE)
        path = find_notification_hidraw()
        self.nfd = os.open(path, os.O_RDONLY | os.O_NONBLOCK) if path else None
        self.pending_events = []  # notifications read while waiting for ACKs

    def close(self):
        try:
            self.dev.release(INTERFACE)
        finally:
            self.dev.close()
            if self.nfd is not None:
                os.close(self.nfd)

    def __enter__(self):
        return self

    def __exit__(self, *exc):
        self.close()

    # ---- transport -------------------------------------------------------------------------
    def _set(self, payload):
        buf = bytes(payload).ljust(64, b"\0")[:64]
        self.dev.hid_set_report(INTERFACE, 3, 0, buf)

    def _get(self):
        return self.dev.control(0xA1, 0x01, 0x0300, INTERFACE, 64)

    def read_notifications(self, timeout=0.0):
        """Return queued + newly arrived notification reports (ACKs filtered out)."""
        out, self.pending_events = self.pending_events, []
        if self.nfd is None:
            return out
        end = time.monotonic() + timeout
        while True:
            r, _, _ = select.select([self.nfd], [], [], max(0.0, end - time.monotonic()))
            if not r:
                return out
            try:
                rep = os.read(self.nfd, 64)
            except BlockingIOError:
                return out
            if not (len(rep) >= 2 and rep[0] == 5 and rep[1] == 0xAC):
                out.append(rep)

    def _wait_ack(self, timeout=0.5):  # NGENUITY HyperXDevice.IOTimeout = 500 ms
        if self.nfd is None:
            time.sleep(0.01)
            return False
        end = time.monotonic() + timeout
        while True:
            r, _, _ = select.select([self.nfd], [], [], max(0.0, end - time.monotonic()))
            if not r:
                return False
            rep = os.read(self.nfd, 64)
            if len(rep) >= 2 and rep[0] == 5 and rep[1] == 0xAC:
                return True
            self.pending_events.append(rep)

    def _packet(self, payload):
        self._set(payload)
        self._wait_ack()

    def _command(self, cmd, arg=1, arg16=False):
        p = bytearray(64)
        p[0], p[1] = 0x04, cmd
        if arg16:
            p[8], p[9] = arg & 0xFF, (arg >> 8) & 0xFF
        else:
            p[8] = arg
        self._set(p)
        self._wait_ack()
        # Flash operations (commit after an upload) can still be busy after the ACK: poll the status.
        deadline = time.monotonic() + 2.0
        while True:
            resp = self._get()
            if resp[1] == cmd and resp[3] == 1:
                return resp
            if time.monotonic() > deadline:
                raise ProtocolError(f"command 0x{cmd:02x} not acknowledged: {resp[:8].hex(' ')}")
            time.sleep(0.05)

    def _commit(self):
        self._command(0x02, 0)

    @staticmethod
    def _with_trailer(p):
        p = bytearray(p).ljust(64, b"\0")
        p[62], p[63] = TRAILER
        return p

    # ---- queries -----------------------------------------------------------------------------
    def device_info(self):
        self._command(0x05)
        info = self._get()
        return {
            "vid": f"{info[8] | info[9] << 8:04x}",
            "pid": f"{info[12] | info[13] << 8:04x}",
            "firmware": f"{info[18]:x}.{info[17]:02x}",
            "country_code": info[24],
        }

    def read_profile(self, slot=None, app_mode=False):
        cmd = 0x66 if app_mode else (0x56 if slot is None else 0x26 + 16 * slot)
        self._command(cmd)
        return Profile(self._get())

    def write_profile(self, profile, slot=None, app_mode=False):
        cmd = 0x67 if app_mode else (0x57 if slot is None else 0x27 + 16 * slot)
        self._command(cmd)
        self._packet(self._with_trailer(profile.raw))
        self._commit()

    # ---- settings ----------------------------------------------------------------------------
    def set_brightness(self, percent):
        """Hardware brightness of the active profile (same field as Fn+[ / Fn+])."""
        prof = self.read_profile()
        prof.brightness = round(max(0, min(100, percent)) * 2.55)
        self.write_profile(prof)
        return prof

    def active_slot(self):
        return self.read_profile().slot

    def select_slot(self, slot, force_reload=False):
        """Make `slot` the active onboard slot.

        The firmware only (re)loads a slot's animation when the active slot changes, so after
        re-uploading the slot that is already active, force_reload hops through another slot.
        """
        prof = self.read_profile()
        if force_reload and prof.slot == slot:
            prof.slot = (slot + 1) % SLOTS
            self.write_profile(prof)
        prof.slot = slot
        self.write_profile(prof)

    # ---- lighting ------------------------------------------------------------------------------
    def send_direct_frame(self, colors):
        """colors: NUM_LEDS (r, g, b) tuples indexed by LED slot. Volatile; must be re-sent continuously."""
        init = bytearray(64)
        init[0], init[1], init[8] = 0x04, 0xF2, 5
        self._set(init)
        for p in range(5):
            buf = bytearray(64)
            for i in range(16):
                r, g, b = colors[p * 16 + i]
                buf[4 * i:4 * i + 4] = (0x81, r, g, b)
            self._set(buf)

    def upload_animation(self, slot, frames, interval, trigger=None, progress=None):
        """Store a looping animation (<=120 frames of NUM_LEDS colors) in onboard slot 0..2.

        trigger: None or dict(kind=fade|explosion|flame, colors=[(r,g,b)] (1 or 2), random=bool, speed=0..255)
        """
        if not 0 <= slot < SLOTS:
            raise ValueError("slot must be 0..2")
        if not 1 <= len(frames) <= 120:
            raise ValueError("1..120 frames")
        total = len(frames) * 5
        self._command(0x53 + 16 * slot, total, arg16=True)
        done = 0
        for frame in frames:
            for p in range(5):
                buf = bytearray(64)
                for i in range(16):
                    r, g, b = frame[p * 16 + i]
                    buf[4 * i:4 * i + 4] = (0x80, r, g, b)
                self._packet(buf)
                done += 1
                if progress:
                    progress(done, total)
        self._commit()

        self._command(0x23 + 16 * slot, 2)
        hdr = bytearray(64)
        hdr[0], hdr[59], hdr[61] = 8, interval & 0xFF, len(frames)
        self._packet(self._with_trailer(hdr))
        trg = bytearray(64)
        if trigger:
            trg[0] = ONBOARD_TRIGGERS[trigger["kind"]]
            c = trigger.get("colors") or [(255, 0, 0)]
            c1, c2 = c[0], c[1] if len(c) > 1 else c[0]
            trg[1:4], trg[5:8] = bytes(c1), bytes(c2)
            if trigger.get("random"):
                trg[57] = 1
            trg[59] = trigger.get("speed", 128) & 0xFF
        self._packet(self._with_trailer(trg))
        self._commit()


def blank_frame():
    return [(0, 0, 0)] * NUM_LEDS

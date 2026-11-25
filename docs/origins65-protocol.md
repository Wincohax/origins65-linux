# HyperX Alloy Origins 65 (03f0:038f) lighting protocol

Reverse engineered from NGENUITY 2.39 (`Assets/Native/NGenuity2Helper.exe`, a managed .NET assembly:
`NGenuity2.Devices.Keyboard.KeyboardAlloyOrigins65` and `NGenuity2.Devices.Keyboard.Origins65.*`).
Tested on firmware 21.04. The newer Avalonia NGENUITY recognises this keyboard but has no lighting
support for it.

## USB interfaces

| Interface | Linux | Purpose |
|---|---|---|
| 0 | hidraw (boot keyboard) | typing |
| 1 | hidraw, report 5 on vendor page `0xFFFF` | notifications (NGENUITY: `MI_01&Col05`) |
| 2 | hidraw | NKRO keyboard |
| 3 | **no hidraw**: usbhid rejects it ("couldn't find an input interrupt endpoint") | commands and lighting (NGENUITY: `MI_03`), page `0xFF90`, 64-byte feature report, no report IDs |

`03f0:048f` is the DFU (firmware update) mode.

Commands are HID `SET_REPORT` (feature, report ID 0, 64 bytes) to interface 3 through usbfs. Status is
read back with `GET_REPORT` (feature) on the same interface. NGENUITY's buffers are 65 bytes with a
report-ID byte first, so its offsets are one higher than the payload offsets below.

## Notifications (interface 1, input report 5)

| Bytes | Meaning |
|---|---|
| `05 AC AC AC …` | ACK, sent after every SET_REPORT (NGENUITY waits up to 500 ms) |
| `05 04 <usage> <1/0>` | key press/release, standard HID usage (`A4` = Fn) |
| `05 07 <0/1>` | game mode changed |
| `05 08 <level>` | brightness changed with Fn+[ / Fn+] (0..255) |
| `05 09 <on> <key> <r g b>` | Fn-lock state |

## Commands

Request: `[0]=04 [1]=cmd [8..9]=argument (packet count, u16 little-endian)`. After the ACK, `GET_REPORT`
returns `[1]=cmd [3]=01` on success. Commits can take longer than the ACK, so poll the status.

| cmd | Meaning |
|---|---|
| `02` | commit (ends a multi-packet write) |
| `05` | device info. The next GET gives VID `[8..9]`, PID `[12..13]`, firmware `[17..18]`, country code `[24]` (57 = US 67-key) |
| `23+16·slot` | animation header for onboard slot 0–2: 2 packets (below) |
| `26+16·slot` / `27+16·slot` | read / write a slot's profile |
| `53+16·slot` | upload animation frames. Argument = frames × 5 packets |
| `56` / `57` | read / write the current profile. Its byte 0 selects the active slot |
| `66` / `67` | read / write the app-mode profile (NGENUITY's brightness slider writes here) |
| `F2` | direct (live) frame, argument 5 |
| `FD` | factory reset |

Writes: the command, then data packet(s) ending in `AA 55` at `[62..63]`, then `02` commit.

### Profile (64 bytes)
`[0]` slot, `[4]` game mode, `[8]` game-mode blocks (1 Alt+Tab, 2 Alt+F4, 4 Shift+Tab, 8 Ctrl+Esc),
`[12]` brightness 0–255, `[13..15]` Fn-lock count/sequence/key, `[16..18]` Fn-lock colour.

### LED frames
80 LED slots, sent as 5 packets of 16 × `[tag R G B]`. LED index = `LEDMatrix.Y * 6 + LEDMatrix.X`
(see `tools/o65/layout.py`).

- Live: `F2` command, then 5 packets with tag `81`. The keyboard reverts to onboard lighting shortly
  after frames stop, so stream continuously.
- Onboard: `53+16·slot` with frames×5, then the frames with tag `80`, then `02`. Next `23+16·slot` with
  argument 2:
  - packet 1: `[0]=08`, `[59]` frame interval (NGENUITY: 1000/20 − 16 = 34), `[61]` frame count (≤120), `AA 55`
  - packet 2, firmware key-press effect: `[0]` 4 Fade / 5 Explosion / 6 HyperX Flame, `[1..3]` colour 1,
    `[5..7]` colour 2, `[57]` random, `[59]` speed 0–255 (normalised over the effect's speed range), `AA 55`
  - then `02`. NGENUITY then writes the slot profile and the current profile to activate the slot.
  - The firmware only reloads an animation when the active slot changes. After re-uploading the slot
    that is already active, switch to another slot and back, or the old animation keeps playing
    (seen on hardware).

## Effects
NGENUITY renders every effect in software. "Saving to device" stores ≤120 frames rendered at 20 fps
(a loop of ≤6 s) plus one firmware key-press effect. These effects have implementations:
Solid, Breathing, Cycle, Wave, Swipe, Confetti, Sun, Twilight (looped) and Fade, Explosion, Blaze/Flame,
Implosion (key press; the firmware can only run Fade, Explosion and Flame). The others in the enum
(Moon, Rain, Snow, Heatmap, Fireworks, Frost, …) are stubs in 2.39. The per-model UI list
(`GetSupportedEffectsByDeviceModel`) is compiled native code in `NGenuity2.dll` and was not recovered.

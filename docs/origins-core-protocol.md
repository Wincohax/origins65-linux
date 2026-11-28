# HyperX Alloy Origins Core (TKL) lighting protocol

**Not verified on hardware.** Everything here comes from NGENUITY 2.39 (`Assets/Native/NGenuity2Helper.exe`:
`NGenuity2.Devices.Keyboard.KeyboardAlloyOriginsCore`, `CMDKBAlloyOriginsCore*`,
`ProfileAlloyOriginsCore`), read with REA and ILSpy. The key map and the live-frame encoding agree with
OpenRGB's `HyperXAlloyOriginsCoreController`, which is the only part another project has tested.

This is a different protocol from the Origins 65's (see [origins65-protocol.md](origins65-protocol.md)):
it is older Sonix firmware, driven with output/input reports instead of feature reports.

## USB

| | |
|---|---|
| IDs | `03f0:098f` (HP), `0951:16e6` (Kingston). DFU mode: `03f0:0a8f`, `0951:16f6` |
| Interface | 2 (NGENUITY matches `MI_02`). usbhid binds it, so it has a hidraw node |
| Out | 64-byte output report, report ID 0 (`write()` 65 bytes, a leading `00` first) |
| In | 64-byte input reports on the same interface: command replies and notifications |

NGENUITY's buffers are 65 bytes with the report ID first, so its offsets are one higher than the
payload offsets below.

## Command frame

```
[0] command   [1] profile 1..3 (or sub-index)   [2] sub-command   [3] data length   [4..] data
```

The reply is an input report whose byte 0 is the command. NGENUITY waits up to 1 s for it (50 ms
for live frames) and carries on without it; the driver does the same, but gives up after five
replies in a row go missing.

| Cmd | Name | Request | Reply |
|---|---|---|---|
| `10` | device info | | `[56]` country code (9 = US ANSI) |
| `14` | game-mode info | `[1]` profile | `[4..7]` Alt+Tab, Alt+F4, Shift+Tab, Ctrl+Esc blocked; `[8]` game mode |
| `15` | active profile | | `[4]` profile 1..3 |
| `27` | brightness | | `[4]` 0..255 (one setting for the keyboard) |
| `93` | preset tag | `[1]` profile << 4, `[3]` 32, `[4..19]` 16-byte GUID | ack |
| `95` | activate profile | `[3]` 1, `[4]` profile 1..3 | none (NGENUITY reads a feature report) |
| `A0` | effect info | `[1]` profile, `[3]` 23, `[4]` effect, … (see below) | ack |
| `A2` | live frame | `[1]` packet 0..6, `[3]` 60 (20 for the last), `[4..]` data | ack |
| `A4` | upload begin/end | `[1]` profile, `[3]` 2, `[4]` `55` begin / `AA` end, `[5]` frame count | none |
| `A5` | animation frame | `[1]` profile << 4 \| packet, `[2]` frame 1.., `[3]` 60/20, `[4..]` data | ack |
| `A7` | set brightness | `[3]` 1, `[4]` `percent * 255 / 100` (truncated) | ack |

NGENUITY also reads and writes LED info (`20`), the key matrix (`30`/`B0`) and macros (`31`/`B1`), and
has commands for per-key lighting status (`A6`) and the stock effects (`A0` with other effect
numbers). The driver doesn't use them.

## Frame buffer

A frame is 380 bytes: 48-byte segments of 12 LEDs, each split into green, red and blue runs of 16
bytes. LED index `i` lands at

```
pos = (i / 12) * 48 + ((i / 6) & 1) * 8 + i % 6      G at pos, R at pos + 16, B at pos + 32
```

which is OpenRGB's segment/sector/sequence formula. NGENUITY's `KeyMap.LEDMatrix` stores the same thing
as `X` (0..5 or 8..13) and `Y` (segment): `i = (Y * 2 + X / 8) * 6 + X % 8`. A live frame is sent as
seven `A2` packets of 60 + 60 + 60 + 60 + 60 + 60 + 20 bytes.

Like the Origins 65, the keyboard falls back to its active profile if live frames stop for a few
seconds (OpenRGB resends every 50 ms for this reason).

## Onboard animation upload (`CMDKBAlloyOriginsCoreSync`)

Up to 100 frames, played at 25 fps.

1. `95` activate the target profile.
2. `93` write the preset tag (NGENUITY uses its preset's GUID; the driver writes a random one).
3. `A4 [4]=55` begin, with the frame count.
4. For every frame, seven `A5` packets.
5. `A4 [4]=AA` end.
6. `A0` effect info: `[4]=9` (custom animation), `[5]` frame interval `1000 / 25 - 16 = 24`,
   `[14]` frame count, and for a key-press effect: `[16]` 4 fade / 5 explosion / 6 flame,
   `[17]` speed 0..63 (`(speed - min) / (max - min) * 63`), `[19]` colour type (1 single,
   2 two colours, 3 gradient, 4 random), `[20..22]` colour. NGENUITY writes the second colour of a
   two-colour effect at `[22..24]`, over the first colour's blue; the driver copies that.
7. Wait 1 s, then `95` back to the profile that should show.

## Notifications

| Bytes | Meaning |
|---|---|
| `73 .. .. .. <usage> <1/0>` | key press/release, HID usage; Fn is `E8` |
| `71 .. .. .. <0/1>` | game mode changed |

Fn+Up / Fn+Down change the brightness without a notification; NGENUITY reads it back with `27`.

## Key layout (US 87-key)

`KeyboardAlloyOriginsCore.Setup87Keyboard`, used for country codes 9, 10, 21, 25 and 32 to 34. Other
countries get 88, 89 or 91-key ISO/JIS maps that the driver doesn't include yet.

## Effect speeds

NGENUITY gives the Origins Core its own ranges for three effects (ms): wave 1000-9000 (the 65 caps it at
6000), explosion 500-3000, flame 800-1500 (default 1000). The rest match the Origins 65.

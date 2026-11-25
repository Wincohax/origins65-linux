# origins65-linux

Linux lighting driver, CLI and GUI for the **HyperX Alloy Origins 65** keyboard (USB `03f0:038f`).
It reimplements the lighting features of HyperX NGENUITY 2.39: live effects, effects that react to
key presses, saving effects to the keyboard's 3 onboard slots, hardware brightness and slot switching.

Tested on firmware 21.04 (US 67-key layout). Not affiliated with HP or HyperX.

## Contents

| Path | What it is |
|---|---|
| `driver/` | Rust library and `origins65` CLI (no system libraries needed) |
| `gui/` | Desktop GUI (GPUI-CE) that uses the library directly |
| `udev/60-origins65.rules` | Lets your user access the keyboard without root |
| `docs/origins65-protocol.md` | The reverse-engineered USB protocol |
| `tools/` | Original Python implementation, used as the reference for the Rust parity tests |

## Setup

Allow non-root access (once):

```bash
sudo cp udev/60-origins65.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

Then unplug and replug the keyboard.

## CLI

```bash
cargo build -p origins65 --release

./target/release/origins65 info
./target/release/origins65 live wave                                     # live, until Ctrl+C
./target/release/origins65 live explosion:color=random wave:speed=4000   # reacts to typing
./target/release/origins65 save --slot 0 wave fade:color=white           # stored on the keyboard
./target/release/origins65 brightness 60
./target/release/origins65 slot 1
./target/release/origins65 effects    # effect list with speed ranges
./target/release/origins65 keys       # key names for keys=...
```

Layers are written `NAME[:option=value,...]`, top layer first. The options are `speed`, `color`
(`RRGGBB`, a name, or `random`), `colors=C1/C2/...`, `keys=w+a+s+d`, `opacity`, `angle` (wave) and
`direction` (swipe). The keyboard can store up to 120 frames per slot (a 6 s loop at 20 fps) plus one
key-press effect: `fade`, `explosion` or `flame`.

## GUI

See [`gui/README.md`](gui/README.md). GPUI needs some native development libraries to build; the README
describes a distrobox setup for immutable distros. The built binary runs on the host.

While the GUI is open it holds the keyboard's lighting interface, so close it before using the CLI.

## Tests

```bash
cargo test -p origins65    # needs python3: compares frames and USB packets with tools/
```

## How it works

Lighting commands are HID feature reports sent to USB interface 3. Linux's usbhid doesn't bind that
interface, which is why other tools that only use hidraw can't change the lighting on Linux. The driver
talks to it through usbfs and reads key and brightness notifications from interface 1's hidraw node.
See [`docs/origins65-protocol.md`](docs/origins65-protocol.md) for the details.

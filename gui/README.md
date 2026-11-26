# Origins 65 Lighting: desktop app

This is the source for the desktop app (`origins65-gui`). If you just want to use it, grab the AppImage;
see the [main README](../README.md). These notes are for building it yourself.

The app is written in Rust with [GPUI-CE](https://github.com/gpui-ce/gpui-ce) (`gpui-ce` /
`gpui_ce_platform` 0.3.0) and uses the `origins65` driver library in `../driver` directly, so there's no
Python or helper process.

## What's inside

- **Layer editor**: add, duplicate, reorder and remove effect layers; tune speed, opacity, angle,
  direction and colours; limit a layer to specific keys on a clickable keyboard map.
- **Live preview**: streams the effect to the keyboard and reacts to key presses and the `Fn` brightness
  keys.
- **Save to keyboard**: writes to onboard slot 0, 1 or 2, with a progress bar and a check that the layers
  fit what the keyboard can store.
- **Device panel**: firmware, active slot, per-slot brightness, hardware brightness and slot switching.
- **System tray**: closing the window keeps the app (and live lighting) running; *Quit* in the tray menu
  or `Ctrl+Q` exits. `--background` starts straight into the tray. Launching it again while it's running
  just brings the existing window forward.

All USB access runs on one worker thread (`src/driver.rs`), so the window never freezes while talking to
the keyboard.

## Building

GPUI needs a few development libraries. On Fedora:

```bash
sudo dnf install gcc pkgconf-pkg-config libxkbcommon-devel libxkbcommon-x11-devel wayland-devel \
  libxcb-devel xcb-util-devel fontconfig-devel freetype-devel vulkan-headers vulkan-loader-devel \
  libX11-devel
```

On immutable systems (Bazzite, Silverblue, …) install those in a Fedora
[distrobox](https://distrobox.it) instead and build there. The finished binary runs on the host:

```bash
distrobox create --name gpui-dev --image registry.fedoraproject.org/fedora:43
distrobox enter gpui-dev -- sudo dnf install -y <packages above>
distrobox enter gpui-dev -- bash -lc 'cd /path/to/origins65-linux && cargo build -p origins65-gui --release'
./target/release/origins65-gui
```

Use Fedora 43 (or older) for the container: the binary then needs glibc 2.42 or newer, so it runs on newer
hosts as well.

## Tests

```bash
cargo test -p origins65-gui                                   # no keyboard needed
cargo clippy -p origins65-gui --all-targets -- -D warnings
```

The tests check the on-screen layout against the driver's LED map, the layer model, the worker thread
(against a mock keyboard), tray menu handling and the single-instance socket.

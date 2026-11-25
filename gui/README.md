# HyperX Alloy Origins 65 — Desktop GUI (`origins65-gui`)

Desktop lighting controller for the HyperX Alloy Origins 65 (`03f0:038f`), built in Rust with **GPUI-CE** (`gpui-ce = "=0.3.0"` and `gpui_ce_platform = "=0.3.0"`).

The GUI links directly against the Rust `origins65` library crate (`driver/`) in-process—no Python runtime or subprocesses are needed. A single dedicated worker thread owns the `Origins65` USB/hidraw handle and runs the live rendering loop alongside `info`, `brightness`, `slot`, and `save` operations.

## Features

- **Layer editor**: add, duplicate, remove, and reorder layers (top-to-bottom); pick any of the 12 looped and key-press effects; edit speed (clamped to each effect's range), opacity, wave angle, swipe direction, single/random/list colors (swatches, RGB sliders, and `#RRGGBB` hex entry), and per-key masks on an interactive 69-LED (US 67-key + outer spacebar LEDs) keyboard layout.
- **Live preview**: Start/Stop live streaming directly from the worker thread with software brightness and FPS controls; reacts in real time to key-press notifications and hardware `Fn+[` / `Fn+]` brightness notifications, updates immediately when layers or parameters change, and cleanly releases the device on exit so the keyboard returns to onboard lighting.
- **Save to keyboard**: target onboard slot `0`–`2`, rendered brightness, activate-after-save toggle, pre-upload validation of onboard rules (at most one key-press effect, and only `fade`, `explosion`, or `flame`), and live upload progress callbacks straight from `Origins65::save_prepared`.
- **Device panel**: displays firmware, VID/PID, country code, active slot, and per-slot brightness and game mode, plus a hardware brightness slider and active-slot switcher.
- **USB arbitration & error reporting**: all hardware I/O is serialized on the single device thread (releasing the handle on error so reconnects recover cleanly) and errors are displayed in an inline banner.

## Build prerequisites (Bazzite / immutable Fedora via Distrobox)

Immutable hosts like Bazzite include the runtime libraries (`libxkbcommon`, `libwayland-client`, `libxcb`, `libvulkan`, `libfontconfig`, `libfreetype`) needed to **run** the binary, but omit the `-devel` headers and `.pc` files needed to **build** GPUI-CE. Use a Fedora 43 Distrobox container so the host OS stays untouched and the resulting binary links against an older-or-equal glibc (2.42) that runs natively on Bazzite 45 (glibc 2.44):

```bash
distrobox create --yes --name gpui-dev --image registry.fedoraproject.org/fedora:43
distrobox enter gpui-dev -- sudo dnf install -y \
    gcc \
    pkgconf-pkg-config \
    libxkbcommon-devel \
    libxkbcommon-x11-devel \
    wayland-devel \
    libxcb-devel \
    xcb-util-devel \
    fontconfig-devel \
    freetype-devel \
    vulkan-headers \
    vulkan-loader-devel \
    libX11-devel \
    openssl-devel \
    dbus-devel
```

## Building

Build the release binary from the workspace root inside the `gpui-dev` Distrobox (reusing your host `~/.cargo` toolchain):

```bash
distrobox enter gpui-dev -- bash -lc '
  export PATH="$HOME/.cargo/bin:$PATH"
  cd /path/to/my-keyboard-driver-2
  cargo build -p origins65-gui --release
'
```

Run unit tests and clippy (layout verification against `origins65::layout` and mock-transport worker thread tests, no hardware or Python required):

```bash
distrobox enter gpui-dev -- bash -lc '
  export PATH="$HOME/.cargo/bin:$PATH"
  cd /path/to/my-keyboard-driver-2
  cargo test -p origins65-gui
  cargo clippy -p origins65-gui --all-targets -- -D warnings
'
```

## Running

The compiled binary runs directly on the Bazzite host (no Distrobox or Python needed at runtime):

```bash
./target/release/origins65-gui
```

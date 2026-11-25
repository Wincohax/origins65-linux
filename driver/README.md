# HyperX Alloy Origins 65 — Rust Lighting Driver & CLI (`origins65`)

Pure-Rust Linux lighting driver library and CLI for the HyperX Alloy Origins 65 keyboard (`03f0:038f`), ported faithfully from the reference Python implementation in `tools/origins65.py` and `tools/o65/`.

## Architecture

- **`driver/src/layout.rs`**: US 67-key + 2 outer spacebar LED layout (`69` LEDs mapped into `80` wire slots), HID usages, grid coordinates, and name/usage lookups.
- **`driver/src/effects.rs`**: Exact port of the NGENUITY 2.39 software effect engine (`TimePosition` live and sync timing, layer compositing with alpha/opacity, all 8 looped effects and 4 key-press trigger effects, `Engine`, `render_sync_frames`, and CLI layer-spec parsing).
- **`driver/src/device.rs`**: USB/HID command protocol over a mockable `Transport` trait, with `NusbTransport` (`nusb` 0.1 pure-Rust usbfs control transfers on USB interface 3 + non-blocking `/dev/hidraw*` polling on interface 1 for ACKs, key events, and Fn brightness notifications).
- **`driver/src/main.rs`**: Drop-in `clap` CLI replacement for `tools/origins65.py`.

## Building

No system `-dev` packages or `libusb` headers are required. Build directly on the host:

```bash
cargo build -p origins65 --release
```

The binary is placed at `./target/release/origins65`.

## Usage

```bash
# Query firmware, active slot, and onboard profiles
./target/release/origins65 info
./target/release/origins65 info --json

# List available effects and key names
./target/release/origins65 effects
./target/release/origins65 keys

# Stream live effects (Ctrl+C returns the keyboard to onboard lighting)
./target/release/origins65 live wave
./target/release/origins65 live explosion:color=random wave:speed=4000
./target/release/origins65 live solid:color=ff0000,keys=w+a+s+d breathing:color=0040ff --fps 30 --brightness 80

# Save effects into onboard memory (slots 0-2)
./target/release/origins65 save --slot 0 wave:speed=3000 fade:color=ffffff

# Hardware brightness (0-100) and active onboard slot (0-2)
./target/release/origins65 brightness 50
./target/release/origins65 slot 0

# Step through every LED in row/col order
./target/release/origins65 identify --step 0.5
```

## Running Tests & Parity Checks

The integration test suite in `driver/tests/parity.rs` runs `driver/scripts/dump_reference.py` against the Python reference implementation in `tools/` and checks:

- Exact 69-key layout and CLI `effects` listing parity.
- Exact frame-by-frame, channel-by-channel `render_sync_frames` parity across 32 deterministic configurations (all deterministic effects, angles, directions, key masks, opacities, multi-layer stacks, frame-count fitting, and brightness scaling).
- Structural checks for stochastic effects (`confetti` loop closure, `sun` palette bounds, `twilight` and random colors).
- Exact byte-for-byte USB packet parity against Python for `send_direct_frame`, `write_profile` (preserving unknown bytes), `set_brightness` rounding, `select_slot` (`force_reload` slot hopping), and `upload_animation` (with and without triggers), using a mock `Transport`.

Run all tests and lints on the host (no keyboard needed):

```bash
cargo test -p origins65
cargo clippy -p origins65 --all-targets -- -D warnings
```

//! HyperX Alloy Origins Core (TKL) protocol (`tools/o65/core.py`), from NGENUITY 2.39
//! `KeyboardAlloyOriginsCore` and `CMDKBAlloyOriginsCore*`, cross-checked against OpenRGB.
//!
//! **Not verified on hardware.**
//!
//! Transport: USB `03f0:098f` (HP) or `0951:16e6` (Kingston), interface 2. usbhid binds it, so its
//! hidraw node is used directly: 64-byte output reports (report ID 0) out, 64-byte input reports in.
//! Every reply is an input report whose byte 0 echoes the command; key events (`0x73`) and game-mode
//! changes (`0x71`) arrive on the same interface.
//!
//! Command frame: `[0]=cmd [1]=profile (1..=3) or sub-index [2]=sub-command [3]=length [4..]=data`.
//! See `docs/origins-core-protocol.md` for the command list.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;

use rand::Rng;

use crate::device::{
    DeviceError, DeviceInfo, FullDeviceInfo, Notification, OnboardTrigger, PreparedSave, Profile,
    SLOTS, SlotProfileEntry, brightness_byte_to_percent, read_hidraw,
};
use crate::effects::Frame;
use crate::model::Model;

pub const INTERFACE: u8 = 2;
pub const FRAME_BYTES: usize = 380;
/// Usage the firmware reports for the Fn key.
pub const FN_USAGE: u8 = 0xE8;

const REPLY_TIMEOUT: Duration = Duration::from_millis(1000);
const PROFILE_DELAY: Duration = Duration::from_millis(50);
const SETTLE_DELAY: Duration = Duration::from_millis(1000);
/// Give up instead of waiting a second per packet on a keyboard that stopped answering.
const MAX_MISSED_REPLIES: u32 = 5;

/// Raw report I/O on the keyboard's interface 2.
pub trait HidTransport: Send {
    /// Send one 64-byte output report (report ID 0).
    fn write_report(&mut self, payload: &[u8; 64]) -> Result<(), DeviceError>;
    /// Wait up to `timeout` for one input report; `Ok(None)` on timeout.
    fn read_report(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, DeviceError>;
}

/// Location of a connected Origins Core.
#[derive(Debug, Clone)]
pub struct CoreHidraw {
    pub path: PathBuf,
    pub vid: u16,
    pub pid: u16,
    /// USB `bcdDevice`, i.e. the firmware version.
    pub bcd_device: u16,
}

/// Find the hidraw node of interface 2 of a connected Origins Core.
pub fn find_core_hidraw() -> Option<CoreHidraw> {
    let mut nodes: Vec<PathBuf> = fs::read_dir("/sys/class/hidraw")
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("hidraw"))
        })
        .collect();
    nodes.sort();
    for node in nodes {
        let Ok(uevent) = fs::read_to_string(node.join("device/uevent")) else {
            continue;
        };
        let Some(&(vid, pid)) = Model::OriginsCore
            .usb_ids()
            .iter()
            .find(|(v, p)| uevent.contains(&format!("HID_ID=0003:{v:08X}:{p:08X}")))
        else {
            continue;
        };
        let Ok(real) = fs::canonicalize(node.join("device")) else {
            continue;
        };
        let Some(intf_dir) = real.parent() else {
            continue;
        };
        let is_interface = intf_dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(&format!(":1.{INTERFACE}")));
        if !is_interface {
            continue;
        }
        let bcd_device = intf_dir
            .parent()
            .and_then(|usb| fs::read_to_string(usb.join("bcdDevice")).ok())
            .and_then(|s| u16::from_str_radix(s.trim(), 16).ok())
            .unwrap_or(0);
        let name = node.file_name()?;
        return Some(CoreHidraw {
            path: PathBuf::from("/dev").join(name),
            vid,
            pid,
            bcd_device,
        });
    }
    None
}

/// hidraw-backed transport.
pub struct HidrawTransport {
    file: File,
}

impl HidrawTransport {
    pub fn open(path: &std::path::Path) -> Result<Self, DeviceError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| DeviceError::Io(format!("failed to open {}: {e}", path.display())))?;
        Ok(Self { file })
    }
}

impl HidTransport for HidrawTransport {
    fn write_report(&mut self, payload: &[u8; 64]) -> Result<(), DeviceError> {
        let mut buf = [0u8; 65];
        buf[1..].copy_from_slice(payload);
        self.file
            .write_all(&buf)
            .map_err(|e| DeviceError::Io(format!("hidraw write failed: {e}")))
    }

    fn read_report(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, DeviceError> {
        read_hidraw(&self.file, timeout)
    }
}

/// LED index -> byte offset of its green value in the 380-byte frame buffer (red is +16, blue +32).
pub const fn frame_offset(led: usize) -> usize {
    (led / 12) * 48 + ((led / 6) & 1) * 8 + led % 6
}

/// Encode a frame into the G/R/B buffer of `KeyboardAlloyOriginsCore.SetLightings`.
pub fn encode_frame(frame: &Frame) -> [u8; FRAME_BYTES] {
    let mut buf = [0u8; FRAME_BYTES];
    for (led, &(r, g, b)) in frame.iter().enumerate() {
        let pos = frame_offset(led);
        if pos + 32 < FRAME_BYTES {
            buf[pos] = g;
            buf[pos + 16] = r;
            buf[pos + 32] = b;
        }
    }
    buf
}

/// The 7 `(index, length, data)` pieces NGENUITY splits a frame buffer into (6 x 60 + 20 bytes).
fn chunks(buf: &[u8; FRAME_BYTES]) -> impl Iterator<Item = (u8, usize, &[u8])> {
    (0..7).map(move |i| {
        let n = if i == 6 { 20 } else { 60 };
        (i as u8, n, &buf[i * 60..i * 60 + n])
    })
}

/// Bytes `[19..=24]` of the `0xA0` packet: colour type, then colours, exactly as
/// `CMDKBAlloyOriginsCoreSync` writes them (its two-colour case puts the second colour at
/// `[22..=24]`, over the first colour's blue).
pub fn trigger_color_bytes(trigger: &OnboardTrigger) -> [u8; 6] {
    let mut out = [0u8; 6];
    let c1 = trigger.colors.first().copied().unwrap_or((242, 0, 0));
    out[1] = c1.0;
    out[2] = c1.1;
    out[3] = c1.2;
    if let Some(&c2) = trigger.colors.get(1) {
        out[0] = 2;
        out[3] = c2.0;
        out[4] = c2.1;
        out[5] = c2.2;
    } else {
        out[0] = 1;
    }
    if trigger.random {
        out[0] = 4;
    }
    out
}

fn cmd_packet(cmd: u8, profile: Option<u8>, sub: u8, length: u8) -> [u8; 64] {
    let mut p = [0u8; 64];
    p[0] = cmd;
    if let Some(prof) = profile {
        p[1] = prof.clamp(1, 3);
        p[2] = sub;
    }
    p[3] = length;
    p
}

/// High-level protocol driver for the HyperX Alloy Origins Core.
pub struct OriginsCore<T: HidTransport = HidrawTransport> {
    transport: T,
    vid: u16,
    pid: u16,
    bcd_device: u16,
    pending: Vec<Vec<u8>>,
    fn_down: bool,
    missed: u32,
    reply_timeout: Duration,
    profile_delay: Duration,
    settle_delay: Duration,
}

impl OriginsCore<HidrawTransport> {
    /// Discover and open the keyboard's interface 2 hidraw node.
    pub fn open() -> Result<Self, DeviceError> {
        let found = find_core_hidraw().ok_or(DeviceError::ModelNotFound(Model::OriginsCore))?;
        let transport = HidrawTransport::open(&found.path)?;
        Ok(Self::with_transport(
            transport,
            found.vid,
            found.pid,
            found.bcd_device,
        ))
    }
}

impl<T: HidTransport> OriginsCore<T> {
    pub fn with_transport(transport: T, vid: u16, pid: u16, bcd_device: u16) -> Self {
        Self {
            transport,
            vid,
            pid,
            bcd_device,
            pending: Vec::new(),
            fn_down: false,
            missed: 0,
            reply_timeout: REPLY_TIMEOUT,
            profile_delay: PROFILE_DELAY,
            settle_delay: SETTLE_DELAY,
        }
    }

    /// Override the reply timeout and the pauses after profile switches and uploads (tests).
    pub fn set_timeouts(&mut self, reply: Duration, profile_delay: Duration, settle: Duration) {
        self.reply_timeout = reply;
        self.profile_delay = profile_delay;
        self.settle_delay = settle;
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    // ---- transport helpers --------------------------------------------------------------------

    fn reply(&mut self, cmd: u8) -> Result<Option<Vec<u8>>, DeviceError> {
        let deadline = std::time::Instant::now() + self.reply_timeout;
        loop {
            let rem = deadline.saturating_duration_since(std::time::Instant::now());
            match self.transport.read_report(rem)? {
                None => {
                    self.missed += 1;
                    if self.missed >= MAX_MISSED_REPLIES {
                        return Err(DeviceError::Protocol(format!(
                            "keyboard stopped answering (last command 0x{cmd:02x})"
                        )));
                    }
                    return Ok(None);
                }
                Some(rep) if rep.first() == Some(&cmd) => {
                    self.missed = 0;
                    return Ok(Some(rep));
                }
                Some(rep) => self.pending.push(rep),
            }
        }
    }

    /// Send a command and wait for its reply (NGENUITY carries on without one).
    fn request(&mut self, payload: &[u8; 64]) -> Result<Option<Vec<u8>>, DeviceError> {
        self.transport.write_report(payload)?;
        self.reply(payload[0])
    }

    fn query(&mut self, payload: &[u8; 64]) -> Result<Vec<u8>, DeviceError> {
        match self.request(payload)? {
            Some(rep) if rep.len() >= 64 => Ok(rep),
            Some(_) => Err(DeviceError::Protocol(format!(
                "short reply to command 0x{:02x}",
                payload[0]
            ))),
            None => Err(DeviceError::Protocol(format!(
                "no reply to command 0x{:02x}",
                payload[0]
            ))),
        }
    }

    // ---- queries ------------------------------------------------------------------------------

    pub fn device_info(&mut self) -> Result<DeviceInfo, DeviceError> {
        let rep = self.query(&cmd_packet(0x10, None, 0, 0))?;
        Ok(DeviceInfo {
            vid: format!("{:04x}", self.vid),
            pid: format!("{:04x}", self.pid),
            firmware: format!("{:x}.{:02x}", self.bcd_device >> 8, self.bcd_device & 0xFF),
            country_code: rep[56],
        })
    }

    pub fn active_slot(&mut self) -> Result<u8, DeviceError> {
        let rep = self.query(&cmd_packet(0x15, None, 0, 0))?;
        Ok(rep[4].saturating_sub(1).min(SLOTS - 1))
    }

    /// Hardware brightness byte (`0..=255`), one setting for the whole keyboard.
    pub fn hardware_brightness(&mut self) -> Result<u8, DeviceError> {
        Ok(self.query(&cmd_packet(0x27, None, 0, 0))?[4])
    }

    /// `(game_mode, disabled_combos)` of a slot; combos use the Origins 65 bit layout.
    pub fn game_mode_info(&mut self, slot: u8) -> Result<(bool, u8), DeviceError> {
        let mut p = cmd_packet(0x14, None, 0, 0);
        p[1] = slot + 1;
        let rep = self.query(&p)?;
        let combos = [(1u8, 4usize), (2, 5), (4, 6), (8, 7)]
            .into_iter()
            .filter(|&(_, i)| rep[i] == 1)
            .fold(0u8, |acc, (bit, _)| acc | bit);
        Ok((rep[8] == 1, combos))
    }

    /// The Origins Core has no profile record; this one is put together from the brightness and
    /// game-mode queries so `info` shows the same fields as on the Origins 65.
    pub fn read_profile(&mut self, slot: Option<u8>) -> Result<Profile, DeviceError> {
        let slot = match slot {
            Some(s) if s >= SLOTS => {
                return Err(DeviceError::InvalidArgument("slot must be 0..2".into()));
            }
            Some(s) => s,
            None => self.active_slot()?,
        };
        let (game_mode, combos) = self.game_mode_info(slot)?;
        let mut prof = Profile::from_raw([0u8; 64]);
        prof.set_slot(slot);
        prof.set_game_mode(game_mode);
        prof.set_disabled_combos(combos);
        prof.set_brightness(self.hardware_brightness()?);
        Ok(prof)
    }

    pub fn read_full_info(&mut self) -> Result<FullDeviceInfo, DeviceError> {
        let info = self.device_info()?;
        let current = self.read_profile(None)?;
        let mut slots = Vec::with_capacity(SLOTS as usize);
        for s in 0..SLOTS {
            match self.read_profile(Some(s)) {
                Ok(prof) => slots.push(SlotProfileEntry {
                    index: s,
                    profile: Some(prof.to_info()),
                    error: None,
                }),
                Err(e) => slots.push(SlotProfileEntry {
                    index: s,
                    profile: None,
                    error: Some(e.to_string()),
                }),
            }
        }
        Ok(FullDeviceInfo {
            model: Model::OriginsCore,
            vid: info.vid,
            pid: info.pid,
            firmware: info.firmware,
            country_code: info.country_code,
            current: current.to_info(),
            slots,
        })
    }

    // ---- settings -----------------------------------------------------------------------------

    pub fn set_brightness(&mut self, percent: i32) -> Result<Profile, DeviceError> {
        let mut p = cmd_packet(0xA7, None, 0, 1);
        // CMDKBAlloyOriginsCoreSetBrightness: (byte)(percent / 100 * 255), truncated.
        p[4] = (percent.clamp(0, 100) * 255 / 100) as u8;
        self.request(&p)?;
        self.read_profile(None)
    }

    fn set_profile(&mut self, slot: u8) -> Result<(), DeviceError> {
        let mut p = cmd_packet(0x95, None, 0, 1);
        p[4] = slot + 1;
        // NGENUITY reads a feature report here instead of waiting for a reply.
        self.transport.write_report(&p)?;
        if !self.profile_delay.is_zero() {
            thread::sleep(self.profile_delay);
        }
        Ok(())
    }

    /// Make `slot` the active onboard profile. `force_reload` hops through another slot when it is
    /// already active, as on the Origins 65.
    pub fn select_slot(&mut self, slot: u8, force_reload: bool) -> Result<(), DeviceError> {
        if slot >= SLOTS {
            return Err(DeviceError::InvalidArgument("slot must be 0..2".into()));
        }
        if force_reload && self.active_slot()? == slot {
            self.set_profile((slot + 1) % SLOTS)?;
        }
        self.set_profile(slot)
    }

    // ---- notifications ------------------------------------------------------------------------

    /// Queued and newly arrived notifications.
    pub fn poll_events(&mut self, timeout: Duration) -> Result<Vec<Notification>, DeviceError> {
        let mut reports = std::mem::take(&mut self.pending);
        let deadline = std::time::Instant::now() + timeout;
        loop {
            let rem = deadline.saturating_duration_since(std::time::Instant::now());
            match self.transport.read_report(rem)? {
                Some(rep) => reports.push(rep),
                None => break,
            }
        }
        let mut out = Vec::new();
        for rep in reports {
            if rep.len() >= 6 && rep[0] == 0x73 {
                let (usage, pressed) = (rep[4], rep[5] == 1);
                if usage == FN_USAGE {
                    self.fn_down = pressed;
                }
                out.push(Notification::KeyEvent { usage, pressed });
                // Fn+Up / Fn+Down change the brightness; NGENUITY reads it back.
                if self.fn_down && pressed && (usage == 0x52 || usage == 0x51) {
                    let level = self.hardware_brightness()?;
                    out.push(Notification::BrightnessChanged {
                        level,
                        percent: brightness_byte_to_percent(level),
                    });
                }
            } else if rep.len() >= 5 && rep[0] == 0x71 {
                out.push(Notification::GameMode(rep[4] == 1));
            }
        }
        Ok(out)
    }

    // ---- lighting -----------------------------------------------------------------------------

    /// Send one volatile live frame (7 `0xA2` packets, no reply wait). The keyboard returns to its
    /// profile if frames stop for a few seconds.
    pub fn send_direct_frame(&mut self, frame: &Frame) -> Result<(), DeviceError> {
        let buf = encode_frame(frame);
        for (i, n, data) in chunks(&buf) {
            let mut p = cmd_packet(0xA2, None, 0, n as u8);
            p[1] = i;
            p[4..4 + n].copy_from_slice(data);
            self.transport.write_report(&p)?;
        }
        Ok(())
    }

    /// `CMDKBAlloyOriginsCoreSync`: store 1..=100 frames in onboard slot `0..=2`. Leaves `slot`
    /// active (NGENUITY activates the profile before writing it). `preset_id` is the 16-byte tag
    /// NGENUITY writes with the animation (random when `None`).
    pub fn upload_animation<F>(
        &mut self,
        slot: u8,
        frames: &[Frame],
        interval: u8,
        trigger: Option<&OnboardTrigger>,
        preset_id: Option<[u8; 16]>,
        mut progress: F,
    ) -> Result<(), DeviceError>
    where
        F: FnMut(usize, usize),
    {
        if slot >= SLOTS {
            return Err(DeviceError::InvalidArgument("slot must be 0..2".into()));
        }
        let max = Model::OriginsCore.max_sync_frames();
        if frames.is_empty() || frames.len() > max {
            return Err(DeviceError::InvalidArgument(format!("1..{max} frames")));
        }
        let board = slot + 1;
        let count = frames.len() as u8;
        let total = frames.len() * 7;
        self.set_profile(slot)?;

        let mut p = cmd_packet(0x93, None, 0, 32);
        p[1] = board << 4;
        let id = preset_id.unwrap_or_else(|| rand::rng().random());
        p[4..20].copy_from_slice(&id);
        self.request(&p)?;

        let mut p = cmd_packet(0xA4, Some(board), 0, 2);
        p[4] = 0x55;
        p[5] = count;
        self.transport.write_report(&p)?;

        let mut done = 0usize;
        for (f, frame) in frames.iter().enumerate() {
            let buf = encode_frame(frame);
            for (i, n, data) in chunks(&buf) {
                let mut p = cmd_packet(0xA5, None, 0, n as u8);
                p[1] = (board << 4) | i;
                p[2] = (f + 1) as u8;
                p[4..4 + n].copy_from_slice(data);
                self.request(&p)?;
                done += 1;
                progress(done, total);
            }
        }

        let mut p = cmd_packet(0xA4, Some(board), 0, 2);
        p[4] = 0xAA;
        p[5] = count;
        self.transport.write_report(&p)?;

        let mut p = cmd_packet(0xA0, Some(board), 0, 23);
        p[4] = 9; // custom animation
        p[5] = interval;
        p[14] = count;
        if let Some(t) = trigger {
            p[16] = t.kind.onboard_trigger_code().ok_or_else(|| {
                DeviceError::InvalidArgument(format!(
                    "unsupported onboard trigger {}",
                    t.kind.name()
                ))
            })?;
            p[17] = t.speed.min(63);
            p[19..25].copy_from_slice(&trigger_color_bytes(t));
        }
        self.request(&p)?;
        if !self.settle_delay.is_zero() {
            thread::sleep(self.settle_delay); // NGENUITY lets the firmware settle
        }
        Ok(())
    }

    /// Upload, then leave the right slot showing: the new one, or the one that was active.
    pub fn save_prepared<F>(
        &mut self,
        slot: u8,
        prepared: &PreparedSave,
        activate: bool,
        progress: F,
    ) -> Result<Option<String>, DeviceError>
    where
        F: FnMut(usize, usize),
    {
        let previous = self.active_slot()?;
        self.upload_animation(
            slot,
            &prepared.frames,
            Model::OriginsCore.sync_interval(),
            prepared.trigger.as_ref(),
            None,
            progress,
        )?;
        if activate || previous == slot {
            // The upload activated `slot`; hop so the firmware reloads it.
            self.select_slot(slot, true)?;
        } else {
            self.set_profile(previous)?;
        }
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::CORE_KEYS;

    #[test]
    fn frame_offsets_match_openrgb() {
        // OpenRGB: segment = (i / 12) * 48, sector = ((i / 6) & 1) * 8, sequence = i % 6.
        assert_eq!(frame_offset(0), 0);
        assert_eq!(frame_offset(6), 8);
        assert_eq!(frame_offset(12), 48);
        assert_eq!(frame_offset(93), 347);
        // Every key fits, blue included.
        assert!(
            CORE_KEYS
                .iter()
                .all(|k| frame_offset(k.led) + 32 < FRAME_BYTES)
        );
    }

    #[test]
    fn two_colour_trigger_matches_ngenuity_overlap() {
        let t = OnboardTrigger {
            kind: crate::effects::EffectKind::Fade,
            colors: vec![(1, 2, 3), (4, 5, 6)],
            random: false,
            speed: 10,
        };
        assert_eq!(trigger_color_bytes(&t), [2, 1, 2, 4, 5, 6]);
    }
}

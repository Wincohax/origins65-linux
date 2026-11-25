//! HyperX Alloy Origins 65 USB/HID command protocol (`tools/o65/device.py`).
//!
//! Transport:
//! - Commands and lighting: 64-byte HID feature reports (report ID 0) on USB interface 3
//!   (vendor page `0xFF90`) via `nusb` control transfers (`SET_REPORT` / `GET_REPORT`).
//! - Notifications: input report 5 on the hidraw node of USB interface 1 (vendor page `0xFFFF`).
//!
//! The transport layer is abstracted behind the [`Transport`] trait so protocol framing,
//! profile read-modify-write, slot switching, live frames, and animation uploads can be
//! verified exhaustively without hardware.

use std::fmt;
use std::fs::{self, File, OpenOptions};
use std::os::fd::AsRawFd;
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::thread;
use std::time::{Duration, Instant};

use nusb::transfer::{Control, ControlType, Recipient};
use serde::{Deserialize, Serialize};

use crate::effects::{EffectKind, Frame, Layer, MAX_SYNC_FRAMES, Rgb, SYNC_FPS, SYNC_INTERVAL, render_sync_frames, scale_frame};

pub const VID: u16 = 0x03F0;
pub const PID: u16 = 0x038F;
pub const INTERFACE: u8 = 3;
pub const SLOTS: u8 = 3;
pub const TRAILER: [u8; 2] = [0xAA, 0x55];

const USB_TIMEOUT: Duration = Duration::from_millis(1000);
const DEFAULT_ACK_TIMEOUT: Duration = Duration::from_millis(500);
const DEFAULT_CMD_TIMEOUT: Duration = Duration::from_millis(2000);
const DEFAULT_POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Error returned by device discovery, USB I/O, or protocol validation.
#[derive(Debug, Clone)]
pub enum DeviceError {
    NotFound,
    Io(String),
    Protocol(String),
    InvalidArgument(String),
}

impl fmt::Display for DeviceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound => write!(f, "{VID:04x}:{PID:04x} not found"),
            Self::Io(msg) => write!(f, "{msg}"),
            Self::Protocol(msg) => write!(f, "{msg}"),
            Self::InvalidArgument(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for DeviceError {}

/// Low-level transport abstraction for USB interface 3 feature reports and interface 1 notifications.
pub trait Transport: Send {
    /// Send a 64-byte HID SET_REPORT (feature, report ID 0) to interface 3.
    fn set_feature_report(&mut self, report: &[u8; 64]) -> Result<(), DeviceError>;

    /// Read a 64-byte HID GET_REPORT (feature, report ID 0) from interface 3.
    fn get_feature_report(&mut self) -> Result<[u8; 64], DeviceError>;

    /// Whether a notification channel (hidraw interface 1) is attached.
    fn has_notifications(&self) -> bool;

    /// Wait up to `timeout` for a single notification report from interface 1.
    /// Returns `Ok(None)` on timeout or if no notification channel is attached.
    fn read_notification(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, DeviceError>;
}

/// Find the `/dev/hidrawN` node for USB interface 1 of the Alloy Origins 65 (`03f0:038f`).
pub fn find_notification_hidraw() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/hidraw").ok()?;
    let mut nodes: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("hidraw"))
        })
        .collect();
    nodes.sort();

    for node in nodes {
        let uevent = match fs::read_to_string(node.join("device/uevent")) {
            Ok(s) => s,
            Err(_) => continue,
        };
        if !uevent.contains("HID_ID=0003:000003F0:0000038F") {
            continue;
        }
        let real = match fs::canonicalize(node.join("device")) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let intf_name = real
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("");
        if intf_name.ends_with(":1.1")
            && let Some(name) = node.file_name()
        {
            return Some(PathBuf::from("/dev").join(name));
        }
    }
    None
}

/// Pure-Rust USB transport backed by `nusb` (usbfs) plus non-blocking Linux hidraw notifications.
pub struct NusbTransport {
    interface: nusb::Interface,
    notif_file: Option<File>,
}

impl NusbTransport {
    pub fn open() -> Result<Self, DeviceError> {
        let dev_info = nusb::list_devices()
            .map_err(|e| DeviceError::Io(format!("failed to enumerate USB devices: {e}")))?
            .find(|d| d.vendor_id() == VID && d.product_id() == PID)
            .ok_or(DeviceError::NotFound)?;

        let device = dev_info
            .open()
            .map_err(|e| DeviceError::Io(format!("failed to open USB device: {e}")))?;
        let interface = device
            .claim_interface(INTERFACE)
            .map_err(|e| DeviceError::Io(format!("failed to claim USB interface {INTERFACE}: {e}")))?;

        let notif_file = find_notification_hidraw().and_then(|path| {
            OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NONBLOCK)
                .open(&path)
                .ok()
        });

        Ok(Self {
            interface,
            notif_file,
        })
    }
}

impl Transport for NusbTransport {
    fn set_feature_report(&mut self, report: &[u8; 64]) -> Result<(), DeviceError> {
        let ctrl = Control {
            control_type: ControlType::Class,
            recipient: Recipient::Interface,
            request: 0x09, // HID SET_REPORT
            value: 0x0300, // Feature report, report ID 0
            index: INTERFACE as u16,
        };
        self.interface
            .control_out_blocking(ctrl, report, USB_TIMEOUT)
            .map_err(|e| DeviceError::Io(format!("USB SET_REPORT failed: {e}")))?;
        Ok(())
    }

    fn get_feature_report(&mut self) -> Result<[u8; 64], DeviceError> {
        let ctrl = Control {
            control_type: ControlType::Class,
            recipient: Recipient::Interface,
            request: 0x01, // HID GET_REPORT
            value: 0x0300, // Feature report, report ID 0
            index: INTERFACE as u16,
        };
        let mut buf = [0u8; 64];
        self.interface
            .control_in_blocking(ctrl, &mut buf, USB_TIMEOUT)
            .map_err(|e| DeviceError::Io(format!("USB GET_REPORT failed: {e}")))?;
        Ok(buf)
    }

    fn has_notifications(&self) -> bool {
        self.notif_file.is_some()
    }

    fn read_notification(&mut self, timeout: Duration) -> Result<Option<Vec<u8>>, DeviceError> {
        let Some(ref file) = self.notif_file else {
            return Ok(None);
        };
        let fd = file.as_raw_fd();
        let deadline = Instant::now() + timeout;

        loop {
            let rem = deadline.saturating_duration_since(Instant::now());
            let timeout_ms = if timeout.is_zero() {
                0
            } else {
                rem.as_millis().min(i32::MAX as u128) as i32
            };

            let mut pfd = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            let rc = unsafe { libc::poll(&mut pfd, 1, timeout_ms) };
            if rc < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::Interrupted {
                    if Instant::now() >= deadline {
                        return Ok(None);
                    }
                    continue;
                }
                return Err(DeviceError::Io(format!("hidraw poll failed: {err}")));
            }
            if rc == 0 {
                return Ok(None);
            }
            if (pfd.revents & libc::POLLIN) == 0 {
                return Ok(None);
            }

            let mut buf = [0u8; 64];
            let n = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) };
            if n < 0 {
                let err = std::io::Error::last_os_error();
                if err.kind() == std::io::ErrorKind::WouldBlock {
                    return Ok(None);
                }
                if err.kind() == std::io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(DeviceError::Io(format!("hidraw read failed: {err}")));
            }
            if n == 0 {
                return Ok(None);
            }
            return Ok(Some(buf[..n as usize].to_vec()));
        }
    }
}

/// 64-byte profile record (`ProfileAlloyOrigins65`). Unknown bytes are preserved on write.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    pub raw: [u8; 64],
}

impl Profile {
    pub fn from_raw(raw: [u8; 64]) -> Self {
        Self { raw }
    }

    pub fn slot(&self) -> u8 {
        self.raw[0]
    }

    pub fn set_slot(&mut self, v: u8) {
        self.raw[0] = v;
    }

    pub fn game_mode(&self) -> bool {
        self.raw[4] == 1
    }

    pub fn set_game_mode(&mut self, v: bool) {
        self.raw[4] = if v { 1 } else { 0 };
    }

    pub fn disabled_combos(&self) -> u8 {
        self.raw[8]
    }

    pub fn set_disabled_combos(&mut self, v: u8) {
        self.raw[8] = v;
    }

    pub fn brightness(&self) -> u8 {
        self.raw[12]
    }

    pub fn set_brightness(&mut self, v: u8) {
        self.raw[12] = v;
    }

    pub fn brightness_percent(&self) -> u8 {
        brightness_byte_to_percent(self.raw[12])
    }

    pub fn fnlock_key(&self) -> u8 {
        self.raw[15]
    }

    pub fn fnlock_color_hex(&self) -> String {
        format!(
            "{:02x}{:02x}{:02x}",
            self.raw[16], self.raw[17], self.raw[18]
        )
    }

    pub fn game_mode_blocks(&self) -> Vec<String> {
        let d = self.disabled_combos();
        [
            (1u8, "Alt+Tab"),
            (2, "Alt+F4"),
            (4, "Shift+Tab"),
            (8, "Ctrl+Esc"),
        ]
        .into_iter()
        .filter(|&(bit, _)| (d & bit) != 0)
        .map(|(_, name)| name.to_string())
        .collect()
    }

    pub fn to_info(&self) -> ProfileInfo {
        ProfileInfo {
            slot: self.slot(),
            brightness: self.brightness(),
            brightness_percent: self.brightness_percent(),
            game_mode: self.game_mode(),
            game_mode_blocks: self.game_mode_blocks(),
            fnlock_key: self.fnlock_key(),
            fnlock_color: self.fnlock_color_hex(),
        }
    }

    pub fn describe(&self) -> String {
        let blocks = self.game_mode_blocks();
        let blocks_str = if blocks.is_empty() {
            "none".to_string()
        } else {
            blocks.join("/")
        };
        format!(
            "slot={} brightness={} ({}%) game_mode={} game_mode_blocks={} fnlock_key={} fnlock_color={}",
            self.slot(),
            self.brightness(),
            self.brightness_percent(),
            if self.game_mode() { "on" } else { "off" },
            blocks_str,
            self.fnlock_key(),
            self.fnlock_color_hex(),
        )
    }
}

/// Convert raw hardware brightness byte (`0..=255`) to percentage (`0..=100`) using Python's
/// round-half-to-even (`round(b / 2.55)`).
pub fn brightness_byte_to_percent(b: u8) -> u8 {
    ((b as f64) / 2.55).round_ties_even().clamp(0.0, 100.0) as u8
}

/// Convert percentage (`0..=100`, clamped) to hardware brightness byte (`0..=255`) using Python's
/// round-half-to-even (`round(max(0, min(100, percent)) * 2.55)`).
pub fn percent_to_brightness_byte(percent: i32) -> u8 {
    let clamped = percent.clamp(0, 100) as f64;
    (clamped * 2.55).round_ties_even().clamp(0.0, 255.0) as u8
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub vid: String,
    pub pid: String,
    pub firmware: String,
    pub country_code: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProfileInfo {
    pub slot: u8,
    pub brightness: u8,
    pub brightness_percent: u8,
    pub game_mode: bool,
    pub game_mode_blocks: Vec<String>,
    pub fnlock_key: u8,
    pub fnlock_color: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SlotProfileEntry {
    pub index: u8,
    pub profile: Option<ProfileInfo>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FullDeviceInfo {
    pub vid: String,
    pub pid: String,
    pub firmware: String,
    pub country_code: u8,
    pub current: ProfileInfo,
    pub slots: Vec<SlotProfileEntry>,
}

/// Firmware key-press trigger configuration stored alongside an onboard animation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardTrigger {
    pub kind: EffectKind,
    pub colors: Vec<Rgb>,
    pub random: bool,
    pub speed: u8,
}

/// Prepared onboard save payload (rendered frames + optional firmware key-press trigger).
#[derive(Debug, Clone)]
pub struct PreparedSave {
    pub frames: Vec<Frame>,
    pub trigger: Option<OnboardTrigger>,
}

/// Validate layers for onboard storage, render the sync loop frames, and scale by `brightness_percent`.
pub fn prepare_onboard_save(
    layers: &[Layer],
    brightness_percent: f64,
) -> Result<PreparedSave, String> {
    let looped: Vec<Layer> = layers
        .iter()
        .filter(|l| !l.is_triggered())
        .cloned()
        .collect();
    let triggers: Vec<&Layer> = layers.iter().filter(|l| l.is_triggered()).collect();
    if triggers.len() > 1 {
        return Err("only one key-press effect can be stored onboard".to_string());
    }
    let trigger = if let Some(t) = triggers.first() {
        if t.kind.onboard_trigger_code().is_none() {
            return Err(format!(
                "{} can't run onboard; the firmware supports: fade, explosion, flame",
                t.kind.name()
            ));
        }
        let min_sp = t.kind.min_speed() as f64;
        let max_sp = t.kind.max_speed() as f64;
        let clamped = (t.speed as f64).clamp(min_sp, max_sp);
        let sp = ((clamped - min_sp) / (max_sp - min_sp) * 255.0) as u8;
        let colors = match &t.colors {
            Some(cs) if !cs.is_empty() => cs.clone(),
            _ => vec![t.primary_color()],
        };
        Some(OnboardTrigger {
            kind: t.kind,
            colors,
            random: t.random_color,
            speed: sp,
        })
    } else {
        None
    };

    let raw_frames = render_sync_frames(&looped, SYNC_FPS, MAX_SYNC_FRAMES);
    let scale = brightness_percent / 100.0;
    let frames: Vec<Frame> = raw_frames.iter().map(|f| scale_frame(f, scale)).collect();

    Ok(PreparedSave { frames, trigger })
}

/// Parsed notification from USB interface 1 (input report 5).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notification {
    KeyEvent { usage: u8, pressed: bool },
    GameMode(bool),
    BrightnessChanged { level: u8, percent: u8 },
    FnLock(Vec<u8>),
    Other(Vec<u8>),
}

impl Notification {
    pub fn parse(report: &[u8]) -> Option<Self> {
        if is_ack(report) {
            return None;
        }
        if report.len() >= 4 && report[0] == 5 && report[1] == 4 {
            return Some(Self::KeyEvent {
                usage: report[2],
                pressed: report[3] == 1,
            });
        }
        if report.len() >= 3 && report[0] == 5 && report[1] == 7 {
            return Some(Self::GameMode(report[2] != 0));
        }
        if report.len() >= 3 && report[0] == 5 && report[1] == 8 {
            return Some(Self::BrightnessChanged {
                level: report[2],
                percent: brightness_byte_to_percent(report[2]),
            });
        }
        if report.len() >= 2 && report[0] == 5 && report[1] == 9 {
            return Some(Self::FnLock(report.to_vec()));
        }
        Some(Self::Other(report.to_vec()))
    }
}

fn is_ack(report: &[u8]) -> bool {
    report.len() >= 2 && report[0] == 5 && report[1] == 0xAC
}

fn with_trailer(src: &[u8; 64]) -> [u8; 64] {
    let mut out = *src;
    out[62] = TRAILER[0];
    out[63] = TRAILER[1];
    out
}

/// High-level protocol driver for the HyperX Alloy Origins 65.
pub struct Origins65<T: Transport = NusbTransport> {
    transport: T,
    pending_events: Vec<Vec<u8>>,
    ack_timeout: Duration,
    cmd_timeout: Duration,
    poll_interval: Duration,
}

impl Origins65<NusbTransport> {
    /// Discover and open the real keyboard over USB (interface 3 + hidraw interface 1).
    pub fn open() -> Result<Self, DeviceError> {
        let transport = NusbTransport::open()?;
        Ok(Self::with_transport(transport))
    }
}

impl<T: Transport> Origins65<T> {
    pub fn with_transport(transport: T) -> Self {
        Self {
            transport,
            pending_events: Vec::new(),
            ack_timeout: DEFAULT_ACK_TIMEOUT,
            cmd_timeout: DEFAULT_CMD_TIMEOUT,
            poll_interval: DEFAULT_POLL_INTERVAL,
        }
    }

    pub fn set_timeouts(
        &mut self,
        ack_timeout: Duration,
        cmd_timeout: Duration,
        poll_interval: Duration,
    ) {
        self.ack_timeout = ack_timeout;
        self.cmd_timeout = cmd_timeout;
        self.poll_interval = poll_interval;
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn transport_mut(&mut self) -> &mut T {
        &mut self.transport
    }

    // ---- transport helpers --------------------------------------------------------------------

    fn set_report(&mut self, payload: &[u8; 64]) -> Result<(), DeviceError> {
        self.transport.set_feature_report(payload)
    }

    fn get_report(&mut self) -> Result<[u8; 64], DeviceError> {
        self.transport.get_feature_report()
    }

    /// Return queued + newly arrived raw notification reports (ACKs filtered out).
    pub fn read_notifications(&mut self, timeout: Duration) -> Result<Vec<Vec<u8>>, DeviceError> {
        let mut out = std::mem::take(&mut self.pending_events);
        if !self.transport.has_notifications() {
            return Ok(out);
        }
        let end = Instant::now() + timeout;
        loop {
            let rem = end.saturating_duration_since(Instant::now());
            match self.transport.read_notification(rem)? {
                None => return Ok(out),
                Some(rep) => {
                    if !is_ack(&rep) {
                        out.push(rep);
                    }
                }
            }
        }
    }

    fn wait_ack(&mut self) -> Result<bool, DeviceError> {
        if !self.transport.has_notifications() {
            thread::sleep(Duration::from_millis(10));
            return Ok(false);
        }
        let end = Instant::now() + self.ack_timeout;
        loop {
            let rem = end.saturating_duration_since(Instant::now());
            match self.transport.read_notification(rem)? {
                None => return Ok(false),
                Some(rep) => {
                    if is_ack(&rep) {
                        return Ok(true);
                    }
                    self.pending_events.push(rep);
                }
            }
        }
    }

    fn send_packet(&mut self, payload: &[u8; 64]) -> Result<(), DeviceError> {
        self.set_report(payload)?;
        let _ = self.wait_ack()?;
        Ok(())
    }

    fn command(&mut self, cmd: u8, arg: u16, arg16: bool) -> Result<[u8; 64], DeviceError> {
        let mut p = [0u8; 64];
        p[0] = 0x04;
        p[1] = cmd;
        if arg16 {
            p[8] = (arg & 0xFF) as u8;
            p[9] = ((arg >> 8) & 0xFF) as u8;
        } else {
            p[8] = (arg & 0xFF) as u8;
        }
        self.set_report(&p)?;
        let _ = self.wait_ack()?;

        // Flash operations (commit after upload) can still be busy after the ACK: poll status.
        let deadline = Instant::now() + self.cmd_timeout;
        loop {
            let resp = self.get_report()?;
            if resp[1] == cmd && resp[3] == 1 {
                return Ok(resp);
            }
            if Instant::now() > deadline {
                let hex = resp[..8]
                    .iter()
                    .map(|b| format!("{b:02x}"))
                    .collect::<Vec<_>>()
                    .join(" ");
                return Err(DeviceError::Protocol(format!(
                    "command 0x{cmd:02x} not acknowledged: {hex}"
                )));
            }
            if !self.poll_interval.is_zero() {
                thread::sleep(self.poll_interval);
            }
        }
    }

    fn commit(&mut self) -> Result<(), DeviceError> {
        self.command(0x02, 0, false)?;
        Ok(())
    }

    // ---- queries ------------------------------------------------------------------------------

    pub fn device_info(&mut self) -> Result<DeviceInfo, DeviceError> {
        self.command(0x05, 1, false)?;
        let info = self.get_report()?;
        let vid = u16::from_le_bytes([info[8], info[9]]);
        let pid = u16::from_le_bytes([info[12], info[13]]);
        Ok(DeviceInfo {
            vid: format!("{vid:04x}"),
            pid: format!("{pid:04x}"),
            firmware: format!("{:x}.{:02x}", info[18], info[17]),
            country_code: info[24],
        })
    }

    pub fn read_profile(
        &mut self,
        slot: Option<u8>,
        app_mode: bool,
    ) -> Result<Profile, DeviceError> {
        let cmd = if app_mode {
            0x66
        } else if let Some(s) = slot {
            if s >= SLOTS {
                return Err(DeviceError::InvalidArgument("slot must be 0..2".into()));
            }
            0x26 + 16 * s
        } else {
            0x56
        };
        self.command(cmd, 1, false)?;
        Ok(Profile::from_raw(self.get_report()?))
    }

    pub fn write_profile(
        &mut self,
        profile: &Profile,
        slot: Option<u8>,
        app_mode: bool,
    ) -> Result<(), DeviceError> {
        let cmd = if app_mode {
            0x67
        } else if let Some(s) = slot {
            if s >= SLOTS {
                return Err(DeviceError::InvalidArgument("slot must be 0..2".into()));
            }
            0x27 + 16 * s
        } else {
            0x57
        };
        self.command(cmd, 1, false)?;
        let pkt = with_trailer(&profile.raw);
        self.send_packet(&pkt)?;
        self.commit()
    }

    /// Read device info, current profile, and all onboard slot profiles (matching `info --json`).
    pub fn read_full_info(&mut self) -> Result<FullDeviceInfo, DeviceError> {
        let info = self.device_info()?;
        let current = self.read_profile(None, false)?;
        let mut slots = Vec::with_capacity(SLOTS as usize);
        for s in 0..SLOTS {
            match self.read_profile(Some(s), false) {
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
            vid: info.vid,
            pid: info.pid,
            firmware: info.firmware,
            country_code: info.country_code,
            current: current.to_info(),
            slots,
        })
    }

    // ---- settings -----------------------------------------------------------------------------

    /// Hardware brightness of the active profile (same field as Fn+[ / Fn+]).
    pub fn set_brightness(&mut self, percent: i32) -> Result<Profile, DeviceError> {
        let mut prof = self.read_profile(None, false)?;
        prof.set_brightness(percent_to_brightness_byte(percent));
        self.write_profile(&prof, None, false)?;
        Ok(prof)
    }

    pub fn active_slot(&mut self) -> Result<u8, DeviceError> {
        Ok(self.read_profile(None, false)?.slot())
    }

    /// Make `slot` the active onboard slot.
    ///
    /// The firmware only (re)loads a slot's animation when the active slot changes, so after
    /// re-uploading the slot that is already active, `force_reload` hops through another slot.
    pub fn select_slot(&mut self, slot: u8, force_reload: bool) -> Result<(), DeviceError> {
        if slot >= SLOTS {
            return Err(DeviceError::InvalidArgument("slot must be 0..2".into()));
        }
        let mut prof = self.read_profile(None, false)?;
        if force_reload && prof.slot() == slot {
            prof.set_slot((slot + 1) % SLOTS);
            self.write_profile(&prof, None, false)?;
        }
        prof.set_slot(slot);
        self.write_profile(&prof, None, false)
    }

    // ---- lighting -----------------------------------------------------------------------------

    /// Send one volatile live frame (5 packets of 16 x `[0x81, r, g, b]`, no ACK wait).
    pub fn send_direct_frame(&mut self, colors: &Frame) -> Result<(), DeviceError> {
        let mut init = [0u8; 64];
        init[0] = 0x04;
        init[1] = 0xF2;
        init[8] = 5;
        self.set_report(&init)?;

        for p in 0..5 {
            let mut buf = [0u8; 64];
            for i in 0..16 {
                let (r, g, b) = colors[p * 16 + i];
                buf[4 * i] = 0x81;
                buf[4 * i + 1] = r;
                buf[4 * i + 2] = g;
                buf[4 * i + 3] = b;
            }
            self.set_report(&buf)?;
        }
        Ok(())
    }

    /// Store a looping animation (1..=120 frames of `NUM_LEDS` colors) in onboard slot `0..=2`.
    pub fn upload_animation<F>(
        &mut self,
        slot: u8,
        frames: &[Frame],
        interval: u8,
        trigger: Option<&OnboardTrigger>,
        mut progress: F,
    ) -> Result<(), DeviceError>
    where
        F: FnMut(usize, usize),
    {
        if slot >= SLOTS {
            return Err(DeviceError::InvalidArgument("slot must be 0..2".into()));
        }
        if frames.is_empty() || frames.len() > MAX_SYNC_FRAMES {
            return Err(DeviceError::InvalidArgument("1..120 frames".into()));
        }
        let total = frames.len() * 5;
        self.command(0x53 + 16 * slot, total as u16, true)?;
        let mut done = 0usize;
        for frame in frames {
            for p in 0..5 {
                let mut buf = [0u8; 64];
                for i in 0..16 {
                    let (r, g, b) = frame[p * 16 + i];
                    buf[4 * i] = 0x80;
                    buf[4 * i + 1] = r;
                    buf[4 * i + 2] = g;
                    buf[4 * i + 3] = b;
                }
                self.send_packet(&buf)?;
                done += 1;
                progress(done, total);
            }
        }
        self.commit()?;

        self.command(0x23 + 16 * slot, 2, false)?;
        let mut hdr = [0u8; 64];
        hdr[0] = 8;
        hdr[59] = interval;
        hdr[61] = frames.len() as u8;
        self.send_packet(&with_trailer(&hdr))?;

        let mut trg = [0u8; 64];
        if let Some(t) = trigger {
            let code = t.kind.onboard_trigger_code().ok_or_else(|| {
                DeviceError::InvalidArgument(format!("unsupported onboard trigger {}", t.kind.name()))
            })?;
            trg[0] = code;
            let c1 = t.colors.first().copied().unwrap_or((255, 0, 0));
            let c2 = t.colors.get(1).copied().unwrap_or(c1);
            trg[1] = c1.0;
            trg[2] = c1.1;
            trg[3] = c1.2;
            trg[5] = c2.0;
            trg[6] = c2.1;
            trg[7] = c2.2;
            if t.random {
                trg[57] = 1;
            }
            trg[59] = t.speed;
        }
        self.send_packet(&with_trailer(&trg))?;
        self.commit()
    }

    /// Complete save workflow matching `tools/origins65.py save`:
    /// uploads the prepared animation, updates the slot profile's slot byte (returning a non-fatal
    /// warning if that read/write fails), and activates/reloads the slot as needed.
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
        self.upload_animation(
            slot,
            &prepared.frames,
            SYNC_INTERVAL,
            prepared.trigger.as_ref(),
            progress,
        )?;

        let profile_note = match self.read_profile(Some(slot), false) {
            Ok(mut prof) => {
                prof.set_slot(slot);
                match self.write_profile(&prof, Some(slot), false) {
                    Ok(()) => None,
                    Err(e) => Some(format!("note: could not update slot {slot} profile ({e})")),
                }
            }
            Err(e) => Some(format!("note: could not update slot {slot} profile ({e})")),
        };

        if activate || self.active_slot()? == slot {
            self.select_slot(slot, true)?;
        }

        Ok(profile_note)
    }
}

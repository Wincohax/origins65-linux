use std::sync::{Arc, LazyLock, mpsc::Sender};

use image::{ImageFormat, RgbaImage};
use ksni::blocking::TrayMethods as _;

pub const APP_ID: &str = "io.github.wincohax.Origins65";
pub const APP_TITLE: &str = "Origins 65 Lighting";

const ICON_PNG_22: &[u8] =
    include_bytes!("../../packaging/icons/22x22/io.github.wincohax.Origins65.png");
const ICON_PNG_32: &[u8] =
    include_bytes!("../../packaging/icons/32x32/io.github.wincohax.Origins65.png");
const ICON_PNG_48: &[u8] =
    include_bytes!("../../packaging/icons/48x48/io.github.wincohax.Origins65.png");
const ICON_PNG_64: &[u8] =
    include_bytes!("../../packaging/icons/64x64/io.github.wincohax.Origins65.png");

static TRAY_ICONS: LazyLock<Vec<ksni::Icon>> = LazyLock::new(|| {
    [ICON_PNG_22, ICON_PNG_32, ICON_PNG_48, ICON_PNG_64]
        .into_iter()
        .filter_map(|bytes| decode_png_to_argb_icon(bytes).ok())
        .collect()
});

static WINDOW_ICON: LazyLock<Option<Arc<RgbaImage>>> = LazyLock::new(|| {
    image::load_from_memory_with_format(ICON_PNG_64, ImageFormat::Png)
        .ok()
        .map(|img| Arc::new(img.into_rgba8()))
});

/// Commands forwarded from the system tray thread or the single-instance socket
/// listener to the GPUI main thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayCommand {
    ShowWindow { activation_token: Option<String> },
    ToggleLive,
    Quit,
    WatcherOffline { is_error: bool },
    WatcherOnline,
}

/// Converts packed RGBA8 pixels into ARGB32 network byte order (big-endian)
/// as required by the StatusNotifierItem `IconPixmap` property.
pub fn rgba_to_argb(rgba: &[u8]) -> Result<Vec<u8>, String> {
    let (chunks, remainder) = rgba.as_chunks::<4>();
    if !remainder.is_empty() {
        return Err(format!(
            "RGBA buffer length {} is not a multiple of 4",
            rgba.len()
        ));
    }
    let mut argb = Vec::with_capacity(rgba.len());
    for &[r, g, b, a] in chunks {
        argb.extend_from_slice(&[a, r, g, b]);
    }
    Ok(argb)
}

/// Decodes a PNG byte slice into a `ksni::Icon` in ARGB32 network byte order.
pub fn decode_png_to_argb_icon(bytes: &[u8]) -> Result<ksni::Icon, String> {
    let img = image::load_from_memory_with_format(bytes, ImageFormat::Png)
        .map_err(|e| format!("failed to decode embedded PNG icon: {e}"))?;
    let rgba = img.into_rgba8();
    let (width, height) = rgba.dimensions();
    let data = rgba_to_argb(rgba.as_raw())?;
    Ok(ksni::Icon {
        width: width as i32,
        height: height as i32,
        data,
    })
}

/// Returns the pre-decoded multi-resolution tray icons.
pub fn load_tray_icons() -> Vec<ksni::Icon> {
    TRAY_ICONS.clone()
}

/// Returns the decoded RGBA window icon for GPUI's `WindowOptions::icon`.
pub fn load_window_icon() -> Option<Arc<RgbaImage>> {
    WINDOW_ICON.clone()
}

pub struct Origins65Tray {
    cmd_tx: Sender<TrayCommand>,
    icons: Vec<ksni::Icon>,
    live_running: bool,
}

impl Origins65Tray {
    pub fn new(cmd_tx: Sender<TrayCommand>) -> Self {
        Self {
            cmd_tx,
            icons: load_tray_icons(),
            live_running: false,
        }
    }

    fn send(&self, cmd: TrayCommand) {
        let _ = self.cmd_tx.send(cmd);
    }
}

impl ksni::Tray for Origins65Tray {
    fn id(&self) -> String {
        APP_ID.into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn title(&self) -> String {
        APP_TITLE.into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        self.icons.clone()
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let description = if self.live_running {
            "Live lighting active"
        } else {
            "HyperX Alloy Origins lighting controller"
        };
        ksni::ToolTip {
            icon_name: String::new(),
            icon_pixmap: self.icons.clone(),
            title: APP_TITLE.into(),
            description: description.into(),
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let activation_token = ksni::current_xdg_activation_token();
        self.send(TrayCommand::ShowWindow { activation_token });
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{CheckmarkItem, MenuItem, StandardItem};

        vec![
            StandardItem {
                label: "Show window".into(),
                activate: Box::new(|this: &mut Self| {
                    let activation_token = ksni::current_xdg_activation_token();
                    this.send(TrayCommand::ShowWindow { activation_token });
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            CheckmarkItem {
                label: "Live lighting".into(),
                checked: self.live_running,
                activate: Box::new(|this: &mut Self| {
                    this.send(TrayCommand::ToggleLive);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|this: &mut Self| {
                    this.send(TrayCommand::Quit);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }

    fn watcher_offline(&self, reason: ksni::OfflineReason) -> bool {
        let is_error = matches!(reason, ksni::OfflineReason::Error(_));
        self.send(TrayCommand::WatcherOffline { is_error });
        // Keep watching so the tray re-registers if the desktop shell restarts.
        true
    }

    fn watcher_online(&self) {
        self.send(TrayCommand::WatcherOnline);
    }
}

/// Handle to the active `ksni` StatusNotifierItem service.
#[derive(Clone)]
pub struct TrayHandle {
    inner: ksni::blocking::Handle<Origins65Tray>,
}

impl TrayHandle {
    pub fn spawn(cmd_tx: Sender<TrayCommand>) -> Result<Self, ksni::Error> {
        let handle = Origins65Tray::new(cmd_tx).spawn()?;
        Ok(Self { inner: handle })
    }

    pub fn set_live_running(&self, live_running: bool) {
        let _ = self.inner.update(|tray| {
            tray.live_running = live_running;
        });
    }

    pub fn shutdown(&self) {
        if !self.inner.is_closed() {
            self.inner.shutdown().wait();
        }
    }
}

/// Pure, testable state-machine decision for tray commands so fallback and
/// lifecycle transitions can be unit-tested without D-Bus or a display server.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayLifecycleState {
    pub tray_online: bool,
    pub window_open: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayEffect {
    ShowOrFocusWindow { activation_token: Option<String> },
    ToggleLive,
    Quit,
    SetQuitOnLastWindowClosed {
        quit_on_close: bool,
        ensure_window_visible_immediately: bool,
        schedule_window_fallback_check: bool,
    },
}

impl TrayLifecycleState {
    pub fn new(tray_online: bool, window_open: bool) -> Self {
        Self {
            tray_online,
            window_open,
        }
    }

    pub fn should_quit_on_last_window_closed(&self) -> bool {
        !self.tray_online
    }

    pub fn should_open_initial_window(&self, background_requested: bool) -> bool {
        !background_requested || !self.tray_online
    }

    pub fn handle_command(&mut self, cmd: TrayCommand) -> TrayEffect {
        match cmd {
            TrayCommand::ShowWindow { activation_token } => {
                self.window_open = true;
                TrayEffect::ShowOrFocusWindow { activation_token }
            }
            TrayCommand::ToggleLive => TrayEffect::ToggleLive,
            TrayCommand::Quit => TrayEffect::Quit,
            TrayCommand::WatcherOffline { is_error } => {
                self.tray_online = false;
                let ensure_now = !self.window_open && is_error;
                let schedule_check = !self.window_open && !is_error;
                if ensure_now {
                    self.window_open = true;
                }
                TrayEffect::SetQuitOnLastWindowClosed {
                    quit_on_close: true,
                    ensure_window_visible_immediately: ensure_now,
                    schedule_window_fallback_check: schedule_check,
                }
            }
            TrayCommand::WatcherOnline => {
                self.tray_online = true;
                TrayEffect::SetQuitOnLastWindowClosed {
                    quit_on_close: false,
                    ensure_window_visible_immediately: false,
                    schedule_window_fallback_check: false,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ksni::Tray as _;
    use std::sync::mpsc;

    #[test]
    fn rgba_to_argb_converts_network_byte_order() {
        let rgba = [0x11, 0x22, 0x33, 0xFF, 0xAA, 0xBB, 0xCC, 0x80];
        let argb = rgba_to_argb(&rgba).expect("valid RGBA slice");
        assert_eq!(argb, vec![0xFF, 0x11, 0x22, 0x33, 0x80, 0xAA, 0xBB, 0xCC]);
    }

    #[test]
    fn rgba_to_argb_rejects_unaligned_buffers() {
        assert!(rgba_to_argb(&[1, 2, 3]).is_err());
    }

    #[test]
    fn embedded_png_icons_decode_to_valid_argb_pixmaps() {
        let icons = load_tray_icons();
        let dims: Vec<(i32, i32)> = icons.iter().map(|i| (i.width, i.height)).collect();
        assert_eq!(dims, vec![(22, 22), (32, 32), (48, 48), (64, 64)]);

        for icon in &icons {
            assert_eq!(icon.data.len(), (icon.width * icon.height * 4) as usize);
            // Verify the center pixel has non-zero alpha (first byte of ARGB32).
            let cx = (icon.width / 2) as usize;
            let cy = (icon.height / 2) as usize;
            let idx = (cy * icon.width as usize + cx) * 4;
            assert!(
                icon.data[idx] > 200,
                "expected opaque center alpha for {}x{}, got {}",
                icon.width,
                icon.height,
                icon.data[idx]
            );
        }

        let win_icon = load_window_icon().expect("window icon PNG should decode");
        assert_eq!(win_icon.dimensions(), (64, 64));
    }

    #[test]
    fn tray_menu_and_callbacks_emit_expected_commands() {
        let (tx, rx) = mpsc::channel();
        let mut tray = Origins65Tray::new(tx);
        assert_eq!(tray.id(), APP_ID);
        assert_eq!(tray.title(), APP_TITLE);
        assert!(!tray.icon_pixmap().is_empty());

        // Left-click activate sends ShowWindow.
        tray.activate(10, 20);
        assert_eq!(
            rx.try_recv().unwrap(),
            TrayCommand::ShowWindow {
                activation_token: None
            }
        );

        // Verify menu structure and live checkmark mirror.
        tray.live_running = true;
        assert!(tray.tool_tip().description.contains("Live lighting"));
        let items = tray.menu();
        assert_eq!(items.len(), 5);

        match &items[0] {
            ksni::MenuItem::Standard(item) => {
                assert_eq!(item.label, "Show window");
                (item.activate)(&mut tray);
            }
            other => panic!("expected Show window StandardItem, got {other:?}"),
        }
        assert_eq!(
            rx.try_recv().unwrap(),
            TrayCommand::ShowWindow {
                activation_token: None
            }
        );

        assert!(matches!(items[1], ksni::MenuItem::Separator));

        match &items[2] {
            ksni::MenuItem::Checkmark(item) => {
                assert_eq!(item.label, "Live lighting");
                assert!(item.checked);
                (item.activate)(&mut tray);
            }
            other => panic!("expected Live lighting CheckmarkItem, got {other:?}"),
        }
        assert_eq!(rx.try_recv().unwrap(), TrayCommand::ToggleLive);

        assert!(matches!(items[3], ksni::MenuItem::Separator));

        match &items[4] {
            ksni::MenuItem::Standard(item) => {
                assert_eq!(item.label, "Quit");
                (item.activate)(&mut tray);
            }
            other => panic!("expected Quit StandardItem, got {other:?}"),
        }
        assert_eq!(rx.try_recv().unwrap(), TrayCommand::Quit);

        // Watcher offline/online notifications.
        assert!(tray.watcher_offline(ksni::OfflineReason::No));
        assert_eq!(
            rx.try_recv().unwrap(),
            TrayCommand::WatcherOffline { is_error: false }
        );
        tray.watcher_online();
        assert_eq!(rx.try_recv().unwrap(), TrayCommand::WatcherOnline);
    }

    #[test]
    fn lifecycle_state_handles_background_and_offline_fallback() {
        // With tray online, --background starts hidden and closing window does not quit.
        let mut state = TrayLifecycleState::new(true, false);
        assert!(!state.should_open_initial_window(true));
        assert!(state.should_open_initial_window(false));
        assert!(!state.should_quit_on_last_window_closed());

        // Without tray host, --background still opens the window and closing quits.
        let no_tray = TrayLifecycleState::new(false, false);
        assert!(no_tray.should_open_initial_window(true));
        assert!(no_tray.should_quit_on_last_window_closed());

        // Soft watcher offline while windowless schedules a fallback check and enables quit-on-close.
        let eff = state.handle_command(TrayCommand::WatcherOffline { is_error: false });
        assert_eq!(
            eff,
            TrayEffect::SetQuitOnLastWindowClosed {
                quit_on_close: true,
                ensure_window_visible_immediately: false,
                schedule_window_fallback_check: true,
            }
        );
        assert!(state.should_quit_on_last_window_closed());

        // Watcher back online restores explicit quit mode.
        let eff = state.handle_command(TrayCommand::WatcherOnline);
        assert_eq!(
            eff,
            TrayEffect::SetQuitOnLastWindowClosed {
                quit_on_close: false,
                ensure_window_visible_immediately: false,
                schedule_window_fallback_check: false,
            }
        );
        assert!(!state.should_quit_on_last_window_closed());

        // Hard watcher error while windowless immediately forces window open.
        let eff = state.handle_command(TrayCommand::WatcherOffline { is_error: true });
        assert_eq!(
            eff,
            TrayEffect::SetQuitOnLastWindowClosed {
                quit_on_close: true,
                ensure_window_visible_immediately: true,
                schedule_window_fallback_check: false,
            }
        );
        assert!(state.window_open);

        // ShowWindow carries through the activation token.
        let eff = state.handle_command(TrayCommand::ShowWindow {
            activation_token: Some("tok-123".into()),
        });
        assert_eq!(
            eff,
            TrayEffect::ShowOrFocusWindow {
                activation_token: Some("tok-123".into())
            }
        );
        assert_eq!(
            state.handle_command(TrayCommand::ToggleLive),
            TrayEffect::ToggleLive
        );
        assert_eq!(state.handle_command(TrayCommand::Quit), TrayEffect::Quit);
    }
}

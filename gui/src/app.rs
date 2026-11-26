use std::collections::BTreeSet;
use std::sync::{Arc, Mutex};

use gpui::{
    Context, Div, FocusHandle, FontWeight, IntoElement, KeyDownEvent, MouseButton, Render,
    SharedString, Stateful, Task, Window, div, prelude::*, px, relative, rgb, rgba,
};

use crate::driver::{DriverClient, DriverCommand, DriverEvent};
use crate::layout::{
    KEYBOARD_ROWS, PRESET_ALPHAS, PRESET_ARROWS, PRESET_MODIFIERS, PRESET_NAV, PRESET_NUMBERS,
    PRESET_WASD, all_key_names,
};
use crate::model::{
    ColorMode, DeviceInfo, EffectKind, LayerConfig, RgbColor, SWATCH_COLORS, SwipeDirection,
    validate_onboard,
};
use crate::slider::{SliderId, SliderPhase, render_slider};

// Palette tokens (dark hardware studio aesthetic)
const BG_APP: u32 = 0x0e1015;
const BG_PANEL: u32 = 0x161922;
const BG_CARD: u32 = 0x1d212d;
const BG_CARD_HOVER: u32 = 0x252a38;
const BG_KEY_OFF: u32 = 0x1a1e28;
const BORDER_SUBTLE: u32 = 0x2a3040;
const BORDER_STRONG: u32 = 0x3b4358;
const TEXT_PRIMARY: u32 = 0xf1f5f9;
const TEXT_SECONDARY: u32 = 0x94a3b8;
const TEXT_MUTED: u32 = 0x64748b;
const ACCENT: u32 = 0xe11d48;
const ACCENT_HOVER: u32 = 0xf43f5e;
const ACCENT_BLUE: u32 = 0x3b82f6;
const GREEN: u32 = 0x10b981;
const AMBER: u32 = 0xf59e0b;
const RED: u32 = 0xef4444;

pub struct Origins65App {
    driver: DriverClient,
    _event_pump: Task<()>,
    on_live_state_changed: Option<Arc<dyn Fn(bool) + Send + Sync>>,

    // Device state
    device_info: Option<DeviceInfo>,
    hw_brightness: u8,
    busy_message: Option<String>,
    error_banner: Option<(String, String)>,
    status_notice: Option<String>,

    // Layer stack
    layers: Vec<LayerConfig>,
    next_layer_id: u64,
    selected_layer_idx: usize,
    selected_color_idx: usize,

    // Live preview state
    live_running: bool,
    live_brightness: u8,
    live_fps: u32,

    // Save to keyboard state
    save_slot: u8,
    save_brightness: u8,
    save_activate: bool,
    save_progress: Option<(u8, String)>,

    // Interactive widget state
    dragging_slider: Option<SliderId>,
    hex_editing: bool,
    hex_buffer: String,
    hex_focus: FocusHandle,
}

impl Origins65App {
    pub fn new(
        on_live_state_changed: Option<Arc<dyn Fn(bool) + Send + Sync>>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (driver, evt_rx) = DriverClient::spawn();
        driver.send(DriverCommand::FetchInfo);

        let rx = Arc::new(Mutex::new(evt_rx));
        let event_pump = cx.spawn(async move |this, cx| {
            loop {
                let rx_clone = Arc::clone(&rx);
                let next_event = cx
                    .background_spawn(async move {
                        let guard = rx_clone.lock().ok()?;
                        guard.recv().ok()
                    })
                    .await;
                let Some(event) = next_event else {
                    break;
                };
                let ok = this.update(cx, |app, cx| {
                    app.handle_driver_event(event, cx);
                });
                if ok.is_err() {
                    break;
                }
            }
        });

        let mut base = LayerConfig::new(1, EffectKind::Wave);
        base.speed = 3000;
        let mut trig = LayerConfig::new(2, EffectKind::Explosion);
        trig.random_color = true;
        trig.speed = 1000;

        Self {
            driver,
            _event_pump: event_pump,
            on_live_state_changed,
            device_info: None,
            hw_brightness: 100,
            busy_message: Some("Reading keyboard info...".into()),
            error_banner: None,
            status_notice: None,
            layers: vec![trig, base],
            next_layer_id: 3,
            selected_layer_idx: 0,
            selected_color_idx: 0,
            live_running: false,
            live_brightness: 100,
            live_fps: 30,
            save_slot: 0,
            save_brightness: 100,
            save_activate: true,
            save_progress: None,
            dragging_slider: None,
            hex_editing: false,
            hex_buffer: String::new(),
            hex_focus: cx.focus_handle(),
        }
    }

    fn set_live_running(&mut self, running: bool, cx: &mut Context<Self>) {
        if self.live_running == running {
            return;
        }
        self.live_running = running;
        if let Some(cb) = self.on_live_state_changed.as_ref().map(Arc::clone) {
            cx.background_spawn(async move {
                cb(running);
            })
            .detach();
        }
    }

    pub fn toggle_live_from_tray(&mut self, cx: &mut Context<Self>) {
        self.toggle_live(cx);
    }

    pub fn clear_transient_window_state(&mut self) {
        self.dragging_slider = None;
        self.hex_editing = false;
    }

    pub fn shutdown(&mut self) {
        self.driver.shutdown();
    }

    fn handle_driver_event(&mut self, event: DriverEvent, cx: &mut Context<Self>) {
        match event {
            DriverEvent::Busy(msg) => {
                self.busy_message = msg;
            }
            DriverEvent::InfoLoaded(Ok(info)) => {
                self.hw_brightness = info.current.brightness_percent;
                self.save_slot = info.current.slot.min(2);
                self.device_info = Some(info);
                self.error_banner = None;
            }
            DriverEvent::InfoLoaded(Err(err)) => {
                self.device_info = None;
                self.error_banner = Some(("Failed to read keyboard".into(), err));
            }
            DriverEvent::LiveState { running, note } => {
                self.set_live_running(running, cx);
                if let Some(n) = note {
                    self.status_notice = Some(n);
                }
            }
            DriverEvent::LiveHardwareBrightness(pct) => {
                self.hw_brightness = pct;
            }
            DriverEvent::LiveExitedError(err) => {
                self.set_live_running(false, cx);
                self.error_banner = Some(("Live preview stopped with an error".into(), err));
            }
            DriverEvent::BrightnessApplied(Ok(pct)) => {
                self.hw_brightness = pct;
                self.error_banner = None;
                self.status_notice = Some(format!("Hardware brightness set to {pct}%."));
            }
            DriverEvent::BrightnessApplied(Err(err)) => {
                self.error_banner = Some(("Failed to set hardware brightness".into(), err));
            }
            DriverEvent::SlotSelected(Ok(slot)) => {
                self.error_banner = None;
                self.status_notice = Some(format!("Switched active onboard slot to Slot {slot}."));
            }
            DriverEvent::SlotSelected(Err(err)) => {
                self.error_banner = Some(("Failed to switch onboard slot".into(), err));
            }
            DriverEvent::SaveProgress {
                slot: _,
                percent,
                status,
            } => {
                self.save_progress = Some((percent, status));
            }
            DriverEvent::SaveFinished(Ok(summary)) => {
                self.save_progress = None;
                self.error_banner = None;
                self.status_notice = Some(summary);
            }
            DriverEvent::SaveFinished(Err(err)) => {
                self.save_progress = None;
                self.error_banner = Some(("Failed to save to keyboard".into(), err));
            }
        }
        cx.notify();
    }

    fn layer_specs(&self) -> Vec<String> {
        self.layers.iter().map(LayerConfig::to_cli_spec).collect()
    }

    fn restart_live_if_running(&mut self) {
        if !self.live_running {
            return;
        }
        let specs = self.layer_specs();
        if specs.is_empty() {
            self.driver.send(DriverCommand::StopLive);
            return;
        }
        self.driver.send(DriverCommand::StartLive {
            specs,
            fps: self.live_fps,
            brightness: self.live_brightness,
        });
    }

    fn toggle_live(&mut self, cx: &mut Context<Self>) {
        if self.live_running {
            self.driver.send(DriverCommand::StopLive);
            self.status_notice = Some("Live preview stopped (keyboard reverted to onboard lighting).".into());
        } else {
            let specs = self.layer_specs();
            if specs.is_empty() {
                self.error_banner = Some((
                    "Cannot start live preview".into(),
                    "Add at least one layer first.".into(),
                ));
            } else {
                self.error_banner = None;
                self.status_notice = None;
                self.driver.send(DriverCommand::StartLive {
                    specs,
                    fps: self.live_fps,
                    brightness: self.live_brightness,
                });
            }
        }
        cx.notify();
    }

    fn start_save(&mut self, cx: &mut Context<Self>) {
        let validation = validate_onboard(&self.layers);
        if !validation.is_valid() {
            self.error_banner = Some((
                "Onboard validation failed".into(),
                validation.errors.join("\n"),
            ));
            cx.notify();
            return;
        }
        self.error_banner = None;
        self.save_progress = Some((0, format!("Rendering frames for slot {}...", self.save_slot)));
        self.driver.send(DriverCommand::Save {
            slot: self.save_slot,
            specs: self.layer_specs(),
            brightness: self.save_brightness,
            activate: self.save_activate,
        });
        cx.notify();
    }

    fn selected_layer(&self) -> Option<&LayerConfig> {
        self.layers.get(self.selected_layer_idx)
    }

    fn selected_layer_mut(&mut self) -> Option<&mut LayerConfig> {
        self.layers.get_mut(self.selected_layer_idx)
    }

    fn active_edit_color(&self) -> RgbColor {
        let Some(layer) = self.selected_layer() else {
            return RgbColor::new(242, 0, 0);
        };
        match layer.effect.color_mode() {
            ColorMode::List | ColorMode::PairOrRandom => layer
                .colors
                .get(self.selected_color_idx)
                .copied()
                .or_else(|| layer.colors.first().copied())
                .unwrap_or(layer.color),
            _ => layer.color,
        }
    }

    fn set_active_edit_color(&mut self, c: RgbColor, commit: bool) {
        let idx = self.selected_color_idx;
        if let Some(layer) = self.selected_layer_mut() {
            match layer.effect.color_mode() {
                ColorMode::List | ColorMode::PairOrRandom => {
                    layer.random_color = false;
                    if let Some(slot) = layer.colors.get_mut(idx) {
                        *slot = c;
                    } else if let Some(first) = layer.colors.first_mut() {
                        *first = c;
                    } else {
                        layer.colors.push(c);
                    }
                }
                ColorMode::Single | ColorMode::SingleOrRandom => {
                    layer.random_color = false;
                    layer.color = c;
                }
                ColorMode::None => {}
            }
        }
        if commit {
            self.restart_live_if_running();
        }
    }

    fn add_layer(&mut self, effect: EffectKind, cx: &mut Context<Self>) {
        let id = self.next_layer_id;
        self.next_layer_id += 1;
        let layer = LayerConfig::new(id, effect);
        self.layers.insert(0, layer);
        self.selected_layer_idx = 0;
        self.selected_color_idx = 0;
        self.hex_editing = false;
        self.restart_live_if_running();
        cx.notify();
    }

    fn duplicate_layer(&mut self, idx: usize, cx: &mut Context<Self>) {
        if let Some(orig) = self.layers.get(idx).cloned() {
            let mut copy = orig;
            copy.id = self.next_layer_id;
            self.next_layer_id += 1;
            self.layers.insert(idx + 1, copy);
            self.selected_layer_idx = idx + 1;
            self.selected_color_idx = 0;
            self.hex_editing = false;
            self.restart_live_if_running();
            cx.notify();
        }
    }

    fn remove_layer(&mut self, idx: usize, cx: &mut Context<Self>) {
        if idx < self.layers.len() {
            self.layers.remove(idx);
            if self.layers.is_empty() {
                self.selected_layer_idx = 0;
            } else if self.selected_layer_idx >= self.layers.len() {
                self.selected_layer_idx = self.layers.len() - 1;
            }
            self.selected_color_idx = 0;
            self.hex_editing = false;
            self.restart_live_if_running();
            cx.notify();
        }
    }

    fn move_layer(&mut self, idx: usize, delta: isize, cx: &mut Context<Self>) {
        let target = idx as isize + delta;
        if target < 0 || target as usize >= self.layers.len() {
            return;
        }
        let target = target as usize;
        self.layers.swap(idx, target);
        if self.selected_layer_idx == idx {
            self.selected_layer_idx = target;
        } else if self.selected_layer_idx == target {
            self.selected_layer_idx = idx;
        }
        self.restart_live_if_running();
        cx.notify();
    }

    fn toggle_key(&mut self, key_name: &'static str, cx: &mut Context<Self>) {
        if let Some(layer) = self.selected_layer_mut() {
            if layer.keys.contains(key_name) {
                layer.keys.remove(key_name);
            } else {
                layer.keys.insert(key_name.to_string());
            }
        }
        self.restart_live_if_running();
        cx.notify();
    }

    fn set_keys_preset(&mut self, keys: Option<&[&str]>, cx: &mut Context<Self>) {
        if let Some(layer) = self.selected_layer_mut() {
            match keys {
                None => layer.keys.clear(),
                Some(list) => {
                    layer.keys = list.iter().map(|s| (*s).to_string()).collect();
                }
            }
        }
        self.restart_live_if_running();
        cx.notify();
    }

    fn invert_keys(&mut self, cx: &mut Context<Self>) {
        if let Some(layer) = self.selected_layer_mut() {
            if layer.keys.is_empty() {
                return;
            }
            let next: BTreeSet<String> = all_key_names()
                .into_iter()
                .filter(|k| !layer.keys.contains(*k))
                .map(|k| k.to_string())
                .collect();
            layer.keys = next;
        }
        self.restart_live_if_running();
        cx.notify();
    }

    fn handle_hex_key_down(
        &mut self,
        event: &KeyDownEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.hex_editing {
            return;
        }
        let k = event.keystroke.key.as_str();
        match k {
            "escape" => {
                self.hex_editing = false;
                cx.notify();
            }
            "enter" => {
                if let Some(c) = RgbColor::from_hex(&self.hex_buffer) {
                    self.set_active_edit_color(c, true);
                }
                self.hex_editing = false;
                cx.notify();
            }
            "backspace" => {
                self.hex_buffer.pop();
                cx.notify();
            }
            _ => {
                if let Some(ch_str) = &event.keystroke.key_char {
                    for ch in ch_str.chars() {
                        if ch.is_ascii_hexdigit() && self.hex_buffer.len() < 6 {
                            self.hex_buffer.push(ch.to_ascii_uppercase());
                            if self.hex_buffer.len() == 6
                                && let Some(c) = RgbColor::from_hex(&self.hex_buffer)
                            {
                                self.set_active_edit_color(c, true);
                            }
                        }
                    }
                    cx.notify();
                }
            }
        }
    }

    fn command_preview(&self) -> String {
        let specs = self.layer_specs();
        if specs.is_empty() {
            return "(no layers configured)".into();
        }
        format!(
            "origins65 live {} --fps {} --brightness {}",
            specs.join(" "),
            self.live_fps,
            self.live_brightness
        )
    }

    // ---- UI Builders -------------------------------------------------------------------------

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let is_connected = self.device_info.is_some();
        let fw_badge = match &self.device_info {
            Some(info) => format!(
                "Connected  {}:{}  FW {}",
                info.vid, info.pid, info.firmware
            ),
            None => "Keyboard not detected".into(),
        };

        let live_btn_bg = if self.live_running { GREEN } else { ACCENT };
        let live_btn_hover = if self.live_running { 0x059669 } else { ACCENT_HOVER };
        let live_label = if self.live_running {
            "Stop Live Preview"
        } else {
            "Start Live Preview"
        };

        let entity = cx.entity();
        let live_frac = self.live_brightness as f32 / 100.0;
        let is_drag_live = self.dragging_slider == Some(SliderId::LiveBrightness);

        div()
            .w_full()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_3()
            .px_4()
            .py_3()
            .bg(rgb(BG_PANEL))
            .border_b_1()
            .border_color(rgb(BORDER_SUBTLE))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_3()
                    .child(
                        div()
                            .w(px(10.0))
                            .h(px(10.0))
                            .rounded_full()
                            .bg(rgb(if is_connected { GREEN } else { RED })),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_base()
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child("HyperX Alloy Origins 65"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(if is_connected {
                                        TEXT_SECONDARY
                                    } else {
                                        AMBER
                                    }))
                                    .child(fw_badge),
                            ),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_3()
                    // Software brightness slider
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_3()
                            .py_1p5()
                            .rounded_md()
                            .bg(rgb(BG_CARD))
                            .border_1()
                            .border_color(rgb(BORDER_SUBTLE))
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child("Live Brightness"),
                            )
                            .child(
                                div().w(px(104.0)).child(render_slider(
                                    live_frac,
                                    rgb(GREEN),
                                    is_drag_live,
                                    true,
                                    move |frac, phase, _w, cx| {
                                        let pct = (frac * 100.0).round().clamp(0.0, 100.0) as u8;
                                        entity.update(cx, |app, cx| {
                                            app.live_brightness = pct;
                                            match phase {
                                                SliderPhase::Start | SliderPhase::Move => {
                                                    app.dragging_slider =
                                                        Some(SliderId::LiveBrightness);
                                                }
                                                SliderPhase::Commit => {
                                                    app.dragging_slider = None;
                                                    app.restart_live_if_running();
                                                }
                                            }
                                            cx.notify();
                                        });
                                    },
                                )),
                            )
                            .child(
                                div()
                                    .w(px(34.0))
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(format!("{}%", self.live_brightness)),
                            ),
                    )
                    // FPS pills
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .p_1()
                            .rounded_md()
                            .bg(rgb(BG_CARD))
                            .border_1()
                            .border_color(rgb(BORDER_SUBTLE))
                            .children([20u32, 30, 60].into_iter().map(|fps| {
                                let active = self.live_fps == fps;
                                div()
                                    .id(("fps", fps as usize))
                                    .px_2()
                                    .py_0p5()
                                    .rounded_sm()
                                    .text_xs()
                                    .cursor_pointer()
                                    .bg(rgb(if active { BORDER_STRONG } else { BG_CARD }))
                                    .text_color(rgb(if active {
                                        TEXT_PRIMARY
                                    } else {
                                        TEXT_MUTED
                                    }))
                                    .hover(|s| s.bg(rgb(BG_CARD_HOVER)))
                                    .child(format!("{fps} fps"))
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        if app.live_fps != fps {
                                            app.live_fps = fps;
                                            app.restart_live_if_running();
                                            cx.notify();
                                        }
                                    }))
                            })),
                    )
                    // Live toggle button
                    .child(
                        div()
                            .id("toggle-live")
                            .flex()
                            .items_center()
                            .gap_2()
                            .px_4()
                            .py_2()
                            .rounded_md()
                            .bg(rgb(live_btn_bg))
                            .hover(|s| s.bg(rgb(live_btn_hover)))
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xffffff))
                            .cursor_pointer()
                            .child(
                                div()
                                    .w(px(8.0))
                                    .h(px(8.0))
                                    .rounded_full()
                                    .bg(rgb(if self.live_running { 0xd1fae5 } else { 0xffe4e6 })),
                            )
                            .child(live_label)
                            .on_click(cx.listener(|app, _, _, cx| app.toggle_live(cx))),
                    ),
            )
    }

    fn render_banners(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let mut container = div().w_full().flex().flex_col();

        if let Some((title, detail)) = &self.error_banner {
            container = container.child(
                div()
                    .w_full()
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap_3()
                    .px_4()
                    .py_2p5()
                    .bg(rgba(0xef444424))
                    .border_b_1()
                    .border_color(rgba(0xef444488))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .flex_1()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(0xfca5a5))
                                    .child(title.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(0xfecaca))
                                    .child(detail.clone()),
                            ),
                    )
                    .child(
                        small_button("dismiss-error", "Dismiss", BG_CARD, TEXT_PRIMARY)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.error_banner = None;
                                cx.notify();
                            })),
                    ),
            );
        }

        if let Some(msg) = &self.busy_message {
            container = container.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_4()
                    .py_1p5()
                    .bg(rgba(0x3b82f620))
                    .border_b_1()
                    .border_color(rgba(0x3b82f655))
                    .child(div().w(px(8.0)).h(px(8.0)).rounded_full().bg(rgb(ACCENT_BLUE)))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xbfdbfe))
                            .child(msg.clone()),
                    ),
            );
        } else if let Some(notice) = &self.status_notice {
            container = container.child(
                div()
                    .w_full()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_4()
                    .py_1p5()
                    .bg(rgba(0x10b9811c))
                    .border_b_1()
                    .border_color(rgba(0x10b98144))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0xa7f3d0))
                            .child(notice.clone()),
                    )
                    .child(
                        div()
                            .id("dismiss-notice")
                            .text_xs()
                            .text_color(rgb(TEXT_SECONDARY))
                            .cursor_pointer()
                            .hover(|s| s.text_color(rgb(TEXT_PRIMARY)))
                            .child("✕")
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.status_notice = None;
                                cx.notify();
                            })),
                    ),
            );
        }

        container
    }

    fn render_device_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let busy = self.busy_message.is_some();
        let connected = self.device_info.is_some();
        let active_slot = self.device_info.as_ref().map(|d| d.current.slot);
        let entity = cx.entity();
        let hw_frac = self.hw_brightness as f32 / 100.0;
        let is_drag_hw = self.dragging_slider == Some(SliderId::HwBrightness);

        let mut panel = panel_box("DEVICE")
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(match &self.device_info {
                        Some(info) => div()
                            .text_xs()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child(format!(
                                "FW {}  •  Country {}  •  Game Mode {}",
                                info.firmware,
                                info.country_code,
                                if info.current.game_mode { "ON" } else { "Off" }
                            )),
                        None => div()
                            .text_xs()
                            .text_color(rgb(AMBER))
                            .child("No keyboard info loaded"),
                    })
                    .child(
                        small_button("refresh-info", "Refresh", BG_CARD, TEXT_PRIMARY).on_click(
                            cx.listener(|app, _, _, cx| {
                                app.error_banner = None;
                                app.driver.send(DriverCommand::FetchInfo);
                                cx.notify();
                            }),
                        ),
                    ),
            )
            // Hardware brightness slider
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2p5()
                    .rounded_md()
                    .bg(rgb(BG_CARD))
                    .border_1()
                    .border_color(rgb(BORDER_SUBTLE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child("Hardware Brightness (Active Profile)"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(format!("{}%", self.hw_brightness)),
                            ),
                    )
                    .child(render_slider(
                        hw_frac,
                        rgb(ACCENT),
                        is_drag_hw,
                        connected && !busy,
                        move |frac, phase, _w, cx| {
                            let pct = (frac * 100.0).round().clamp(0.0, 100.0) as u8;
                            entity.update(cx, |app, cx| {
                                app.hw_brightness = pct;
                                match phase {
                                    SliderPhase::Start | SliderPhase::Move => {
                                        app.dragging_slider = Some(SliderId::HwBrightness);
                                    }
                                    SliderPhase::Commit => {
                                        app.dragging_slider = None;
                                        app.driver.send(DriverCommand::SetBrightness { percent: pct });
                                    }
                                }
                                cx.notify();
                            });
                        },
                    )),
            )
            // Active slot switcher & per-slot summaries
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(TEXT_SECONDARY))
                            .child("Onboard Slots (click to activate)"),
                    )
                    .child(div().flex().gap_1p5().children((0u8..3).map(|slot_idx| {
                        let is_active = active_slot == Some(slot_idx);
                        let slot_entry = self
                            .device_info
                            .as_ref()
                            .and_then(|d| d.slots.iter().find(|s| s.index == slot_idx));
                        let detail = match slot_entry {
                            Some(s) => match (&s.profile, &s.error) {
                                (Some(p), _) => format!(
                                    "{}% • GM {}",
                                    p.brightness_percent,
                                    if p.game_mode { "On" } else { "Off" }
                                ),
                                (None, Some(_)) => "Unreadable".into(),
                                _ => "—".into(),
                            },
                            None => "—".into(),
                        };

                        div()
                            .id(("slot-btn", slot_idx as usize))
                            .flex_1()
                            .flex()
                            .flex_col()
                            .gap_0p5()
                            .p_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(if is_active { ACCENT } else { BORDER_SUBTLE }))
                            .bg(if is_active {
                                rgba(0xe11d4822)
                            } else {
                                rgba(0x1d212dff)
                            })
                            .when(connected && !busy, |d| {
                                d.cursor_pointer().hover(|s| s.bg(rgb(BG_CARD_HOVER)))
                            })
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(TEXT_PRIMARY))
                                            .child(format!("Slot {slot_idx}")),
                                    )
                                    .when(is_active, |d| {
                                        d.child(
                                            div()
                                                .w(px(6.0))
                                                .h(px(6.0))
                                                .rounded_full()
                                                .bg(rgb(ACCENT)),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_MUTED))
                                    .child(detail),
                            )
                            .on_click(cx.listener(move |app, _, _, cx| {
                                if app.device_info.is_some() && app.busy_message.is_none() {
                                    app.driver.send(DriverCommand::SelectSlot { slot: slot_idx });
                                    cx.notify();
                                }
                            }))
                    }))),
            );

        if let Some(info) = &self.device_info
            && !info.current.game_mode_blocks.is_empty()
        {
            panel = panel.child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(format!(
                        "Game mode blocks: {}",
                        info.current.game_mode_blocks.join(", ")
                    )),
            );
        }

        panel
    }

    fn render_layers_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let len = self.layers.len();

        panel_box("LAYERS (TOP TO BOTTOM)")
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap_1p5()
                    .child(
                        small_button("add-looped", "+ Looped Layer", ACCENT, 0xffffff)
                            .hover(|s| s.bg(rgb(ACCENT_HOVER)))
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.add_layer(EffectKind::Solid, cx);
                            })),
                    )
                    .child(
                        small_button("add-trigger", "+ Key-Press", BG_CARD, TEXT_PRIMARY)
                            .on_click(cx.listener(|app, _, _, cx| {
                                app.add_layer(EffectKind::Fade, cx);
                            })),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .when(self.layers.is_empty(), |d| {
                        d.child(
                            div()
                                .p_3()
                                .rounded_md()
                                .bg(rgb(BG_CARD))
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child("No layers yet. Add a looped or key-press layer above."),
                        )
                    })
                    .children(self.layers.iter().enumerate().map(|(idx, layer)| {
                        let selected = idx == self.selected_layer_idx;
                        let swatch = layer.preview_color();
                        let keys_badge = if layer.keys.is_empty() {
                            "All keys".to_string()
                        } else {
                            format!("{} keys", layer.keys.len())
                        };
                        let kind_badge = if layer.effect.is_triggered() {
                            if layer.effect.onboard_ok() {
                                "Key-press"
                            } else {
                                "Live only"
                            }
                        } else {
                            "Looped"
                        };

                        div()
                            .id(("layer-row", layer.id as usize))
                            .flex()
                            .items_center()
                            .justify_between()
                            .gap_2()
                            .px_2p5()
                            .py_2()
                            .rounded_md()
                            .border_1()
                            .border_color(rgb(if selected { ACCENT } else { BORDER_SUBTLE }))
                            .bg(if selected {
                                rgba(0xe11d481e)
                            } else {
                                rgba(0x1d212dff)
                            })
                            .cursor_pointer()
                            .hover(|s| s.bg(rgb(BG_CARD_HOVER)))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.selected_layer_idx = idx;
                                app.selected_color_idx = 0;
                                app.hex_editing = false;
                                cx.notify();
                            }))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .min_w_0()
                                    .child(
                                        div()
                                            .w(px(14.0))
                                            .h(px(14.0))
                                            .rounded_sm()
                                            .bg(rgb(swatch.to_u32()))
                                            .border_1()
                                            .border_color(rgba(0xffffff44)),
                                    )
                                    .child(
                                        div()
                                            .flex()
                                            .flex_col()
                                            .min_w_0()
                                            .child(
                                                div()
                                                    .flex()
                                                    .items_center()
                                                    .gap_1p5()
                                                    .child(
                                                        div()
                                                            .text_xs()
                                                            .font_weight(FontWeight::SEMIBOLD)
                                                            .text_color(rgb(TEXT_PRIMARY))
                                                            .child(format!(
                                                                "{}. {}",
                                                                idx + 1,
                                                                layer.effect.label()
                                                            )),
                                                    )
                                                    .child(
                                                        div()
                                                            .px_1()
                                                            .rounded_sm()
                                                            .text_xs()
                                                            .bg(rgba(if layer.effect.onboard_ok() {
                                                                0x3b82f624
                                                            } else {
                                                                0xf59e0b2a
                                                            }))
                                                            .text_color(rgb(
                                                                if layer.effect.onboard_ok() {
                                                                    0x93c5fd
                                                                } else {
                                                                    0xfcd34d
                                                                },
                                                            ))
                                                            .child(kind_badge),
                                                    ),
                                            )
                                            .child(
                                                div()
                                                    .text_xs()
                                                    .text_color(rgb(TEXT_MUTED))
                                                    .child(format!(
                                                        "{} • {}% op",
                                                        keys_badge, layer.opacity
                                                    )),
                                            ),
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .child(icon_btn(
                                        ("up", layer.id as usize),
                                        "↑",
                                        idx > 0,
                                        cx.listener(move |app, _, _, cx| {
                                            app.move_layer(idx, -1, cx);
                                        }),
                                    ))
                                    .child(icon_btn(
                                        ("down", layer.id as usize),
                                        "↓",
                                        idx + 1 < len,
                                        cx.listener(move |app, _, _, cx| {
                                            app.move_layer(idx, 1, cx);
                                        }),
                                    ))
                                    .child(icon_btn(
                                        ("dup", layer.id as usize),
                                        "+",
                                        true,
                                        cx.listener(move |app, _, _, cx| {
                                            app.duplicate_layer(idx, cx);
                                        }),
                                    ))
                                    .child(icon_btn(
                                        ("del", layer.id as usize),
                                        "✕",
                                        true,
                                        cx.listener(move |app, _, _, cx| {
                                            app.remove_layer(idx, cx);
                                        }),
                                    )),
                            )
                    })),
            )
    }

    fn render_save_panel(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let validation = validate_onboard(&self.layers);
        let busy = self.busy_message.is_some();
        let can_save = validation.is_valid() && !busy;
        let entity = cx.entity();
        let save_frac = self.save_brightness as f32 / 100.0;
        let is_drag_save = self.dragging_slider == Some(SliderId::SaveBrightness);

        let mut panel = panel_box("SAVE TO ONBOARD MEMORY")
            // Target slot picker
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child("Target Slot"),
                    )
                    .child(div().flex().gap_1().children((0u8..3).map(|s| {
                        let active = self.save_slot == s;
                        div()
                            .id(("save-slot", s as usize))
                            .px_2p5()
                            .py_1()
                            .rounded_md()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .border_1()
                            .border_color(rgb(if active { ACCENT } else { BORDER_SUBTLE }))
                            .bg(rgb(if active { ACCENT } else { BG_CARD }))
                            .text_color(rgb(if active { 0xffffff } else { TEXT_SECONDARY }))
                            .cursor_pointer()
                            .hover(|st| st.bg(rgb(if active { ACCENT_HOVER } else { BG_CARD_HOVER })))
                            .child(format!("Slot {s}"))
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.save_slot = s;
                                cx.notify();
                            }))
                    }))),
            )
            // Save brightness slider
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child("Rendered Brightness"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(format!("{}%", self.save_brightness)),
                            ),
                    )
                    .child(render_slider(
                        save_frac,
                        rgb(ACCENT_BLUE),
                        is_drag_save,
                        !busy,
                        move |frac, phase, _w, cx| {
                            let pct = (frac * 100.0).round().clamp(0.0, 100.0) as u8;
                            entity.update(cx, |app, cx| {
                                app.save_brightness = pct;
                                match phase {
                                    SliderPhase::Start | SliderPhase::Move => {
                                        app.dragging_slider = Some(SliderId::SaveBrightness);
                                    }
                                    SliderPhase::Commit => {
                                        app.dragging_slider = None;
                                    }
                                }
                                cx.notify();
                            });
                        },
                    )),
            )
            // Activate after save toggle
            .child(
                div()
                    .id("toggle-activate")
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .child(
                        div()
                            .w(px(16.0))
                            .h(px(16.0))
                            .rounded_sm()
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_1()
                            .border_color(rgb(if self.save_activate {
                                ACCENT_BLUE
                            } else {
                                BORDER_STRONG
                            }))
                            .bg(rgb(if self.save_activate {
                                ACCENT_BLUE
                            } else {
                                BG_CARD
                            }))
                            .text_xs()
                            .text_color(rgb(0xffffff))
                            .child(if self.save_activate { "✓" } else { "" }),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child("Activate slot after upload"),
                    )
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.save_activate = !app.save_activate;
                        cx.notify();
                    })),
            );

        // Validation messages
        if !validation.errors.is_empty() {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_2()
                    .rounded_md()
                    .bg(rgba(0xef444420))
                    .border_1()
                    .border_color(rgba(0xef444466))
                    .children(validation.errors.iter().map(|e| {
                        div()
                            .text_xs()
                            .text_color(rgb(0xfca5a5))
                            .child(format!("• {e}"))
                    })),
            );
        } else {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_0p5()
                    .p_2()
                    .rounded_md()
                    .bg(rgba(0x10b98118))
                    .border_1()
                    .border_color(rgba(0x10b98144))
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(0x6ee7b7))
                            .child("Valid for onboard storage (≤120 frames, ≤1 trigger)."),
                    )
                    .children(validation.notes.iter().map(|n| {
                        div()
                            .text_xs()
                            .text_color(rgb(0xfde68a))
                            .child(format!("Note: {n}"))
                    })),
            );
        }

        // Upload progress bar
        if let Some((pct, status)) = &self.save_progress {
            panel = panel.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child(status.clone()),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(format!("{pct}%")),
                            ),
                    )
                    .child(
                        div()
                            .w_full()
                            .h(px(8.0))
                            .rounded_full()
                            .bg(rgb(BG_CARD))
                            .overflow_hidden()
                            .child(
                                div()
                                    .h_full()
                                    .w(relative(*pct as f32 / 100.0))
                                    .rounded_full()
                                    .bg(rgb(ACCENT_BLUE)),
                            ),
                    ),
            );
        }

        // Save CTA button
        panel = panel.child(
            div()
                .id("save-to-kb")
                .w_full()
                .py_2()
                .rounded_md()
                .flex()
                .items_center()
                .justify_center()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .bg(rgb(if can_save { ACCENT_BLUE } else { BG_CARD }))
                .text_color(rgb(if can_save { 0xffffff } else { TEXT_MUTED }))
                .border_1()
                .border_color(rgb(if can_save { ACCENT_BLUE } else { BORDER_SUBTLE }))
                .when(can_save, |d| {
                    d.cursor_pointer().hover(|s| s.bg(rgb(0x2563eb)))
                })
                .child(format!("Save to Onboard Slot {}", self.save_slot))
                .on_click(cx.listener(move |app, _, _, cx| {
                    if can_save {
                        app.start_save(cx);
                    }
                })),
        );

        panel
    }

    fn render_layer_inspector(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(layer) = self.selected_layer().cloned() else {
            return panel_box("LAYER INSPECTOR").child(
                div()
                    .p_4()
                    .text_sm()
                    .text_color(rgb(TEXT_MUTED))
                    .child("Select or add a layer to edit its effect, colors, and keys."),
            );
        };

        let entity = cx.entity();
        let min_s = layer.effect.min_speed();
        let max_s = layer.effect.max_speed();
        let speed_span = (max_s - min_s).max(1) as f32;
        let speed_frac = ((layer.speed.clamp(min_s, max_s) - min_s) as f32) / speed_span;
        let opacity_frac = layer.opacity as f32 / 100.0;
        let angle_frac = layer.angle as f32 / 360.0;

        let is_drag_speed = self.dragging_slider == Some(SliderId::LayerSpeed);
        let is_drag_opacity = self.dragging_slider == Some(SliderId::LayerOpacity);
        let is_drag_angle = self.dragging_slider == Some(SliderId::LayerAngle);

        let effect_pill = |eff: EffectKind, cx: &mut Context<Self>| {
            let active = layer.effect == eff;
            let live_only = !eff.onboard_ok();
            div()
                .id(("eff", eff as usize))
                .px_2p5()
                .py_1p5()
                .rounded_md()
                .border_1()
                .border_color(rgb(if active { ACCENT } else { BORDER_SUBTLE }))
                .bg(if active {
                    rgba(0xe11d482a)
                } else {
                    rgba(0x1d212dff)
                })
                .cursor_pointer()
                .hover(|s| s.bg(rgb(BG_CARD_HOVER)))
                .flex()
                .items_center()
                .gap_1p5()
                .child(
                    div()
                        .text_xs()
                        .font_weight(if active {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(rgb(if active { TEXT_PRIMARY } else { TEXT_SECONDARY }))
                        .child(eff.label()),
                )
                .when(live_only, |d| {
                    d.child(
                        div()
                            .px_1()
                            .rounded_sm()
                            .bg(rgba(0xf59e0b2e))
                            .text_xs()
                            .text_color(rgb(0xfcd34d))
                            .child("live"),
                    )
                })
                .on_click(cx.listener(move |app, _, _, cx| {
                    if let Some(l) = app.selected_layer_mut() {
                        l.set_effect(eff);
                    }
                    app.selected_color_idx = 0;
                    app.hex_editing = false;
                    app.restart_live_if_running();
                    cx.notify();
                }))
        };

        let mut params = div().flex().flex_wrap().gap_3().w_full();

        // Speed slider (for all animated effects)
        if layer.effect.uses_speed() {
            let e_speed = entity.clone();
            params = params.child(
                div()
                    .flex_1()
                    .min_w(px(220.0))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(BG_CARD))
                    .border_1()
                    .border_color(rgb(BORDER_SUBTLE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child(format!("Speed ({min_s}–{max_s} ms)")),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
                                    .child(
                                        div()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(TEXT_PRIMARY))
                                            .child(format!("{} ms", layer.speed)),
                                    )
                                    .child(
                                        div()
                                            .id("reset-speed")
                                            .text_xs()
                                            .text_color(rgb(TEXT_MUTED))
                                            .cursor_pointer()
                                            .hover(|s| s.text_color(rgb(TEXT_PRIMARY)))
                                            .child("Reset")
                                            .on_click(cx.listener(|app, _, _, cx| {
                                                if let Some(l) = app.selected_layer_mut() {
                                                    l.speed = l.effect.default_speed();
                                                }
                                                app.restart_live_if_running();
                                                cx.notify();
                                            })),
                                    ),
                            ),
                    )
                    .child(render_slider(
                        speed_frac,
                        rgb(ACCENT),
                        is_drag_speed,
                        true,
                        move |frac, phase, _w, cx| {
                            let val = (min_s as f32 + frac * speed_span).round() as u32;
                            e_speed.update(cx, |app, cx| {
                                if let Some(l) = app.selected_layer_mut() {
                                    l.speed = val.clamp(min_s, max_s);
                                }
                                match phase {
                                    SliderPhase::Start | SliderPhase::Move => {
                                        app.dragging_slider = Some(SliderId::LayerSpeed);
                                    }
                                    SliderPhase::Commit => {
                                        app.dragging_slider = None;
                                        app.restart_live_if_running();
                                    }
                                }
                                cx.notify();
                            });
                        },
                    )),
            );
        }

        // Opacity slider
        {
            let e_op = entity.clone();
            params = params.child(
                div()
                    .flex_1()
                    .min_w(px(180.0))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(BG_CARD))
                    .border_1()
                    .border_color(rgb(BORDER_SUBTLE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child("Layer Opacity"),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(TEXT_PRIMARY))
                                    .child(format!("{}%", layer.opacity)),
                            ),
                    )
                    .child(render_slider(
                        opacity_frac,
                        rgb(ACCENT_BLUE),
                        is_drag_opacity,
                        true,
                        move |frac, phase, _w, cx| {
                            let val = (frac * 100.0).round().clamp(0.0, 100.0) as u8;
                            e_op.update(cx, |app, cx| {
                                if let Some(l) = app.selected_layer_mut() {
                                    l.opacity = val;
                                }
                                match phase {
                                    SliderPhase::Start | SliderPhase::Move => {
                                        app.dragging_slider = Some(SliderId::LayerOpacity);
                                    }
                                    SliderPhase::Commit => {
                                        app.dragging_slider = None;
                                        app.restart_live_if_running();
                                    }
                                }
                                cx.notify();
                            });
                        },
                    )),
            );
        }

        // Wave angle slider
        if layer.effect.has_angle() {
            let e_ang = entity.clone();
            params = params.child(
                div()
                    .flex_1()
                    .min_w(px(220.0))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(BG_CARD))
                    .border_1()
                    .border_color(rgb(BORDER_SUBTLE))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(
                                div()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(TEXT_SECONDARY))
                                    .child("Wave Angle"),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_1()
                                    .children([0u16, 90, 180, 270].into_iter().map(|deg| {
                                        let active = layer.angle == deg;
                                        div()
                                            .id(("ang", deg as usize))
                                            .px_1p5()
                                            .py_0p5()
                                            .rounded_sm()
                                            .text_xs()
                                            .bg(rgb(if active {
                                                BORDER_STRONG
                                            } else {
                                                BG_PANEL
                                            }))
                                            .text_color(rgb(if active {
                                                TEXT_PRIMARY
                                            } else {
                                                TEXT_MUTED
                                            }))
                                            .cursor_pointer()
                                            .child(format!("{deg}°"))
                                            .on_click(cx.listener(move |app, _, _, cx| {
                                                if let Some(l) = app.selected_layer_mut() {
                                                    l.angle = deg;
                                                }
                                                app.restart_live_if_running();
                                                cx.notify();
                                            }))
                                    }))
                                    .child(
                                        div()
                                            .ml_1()
                                            .text_xs()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(TEXT_PRIMARY))
                                            .child(format!("{}°", layer.angle)),
                                    ),
                            ),
                    )
                    .child(render_slider(
                        angle_frac,
                        rgb(0xa855f7),
                        is_drag_angle,
                        true,
                        move |frac, phase, _w, cx| {
                            let val = (frac * 360.0).round().clamp(0.0, 360.0) as u16;
                            e_ang.update(cx, |app, cx| {
                                if let Some(l) = app.selected_layer_mut() {
                                    l.angle = val;
                                }
                                match phase {
                                    SliderPhase::Start | SliderPhase::Move => {
                                        app.dragging_slider = Some(SliderId::LayerAngle);
                                    }
                                    SliderPhase::Commit => {
                                        app.dragging_slider = None;
                                        app.restart_live_if_running();
                                    }
                                }
                                cx.notify();
                            });
                        },
                    )),
            );
        }

        // Swipe direction picker
        if layer.effect.has_direction() {
            params = params.child(
                div()
                    .flex_1()
                    .min_w(px(220.0))
                    .flex()
                    .flex_col()
                    .gap_1p5()
                    .p_3()
                    .rounded_md()
                    .bg(rgb(BG_CARD))
                    .border_1()
                    .border_color(rgb(BORDER_SUBTLE))
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(TEXT_SECONDARY))
                            .child("Swipe Direction"),
                    )
                    .child(
                        div()
                            .flex()
                            .gap_1p5()
                            .children(SwipeDirection::ALL.into_iter().map(|dir| {
                                let active = layer.direction == dir;
                                div()
                                    .id(("dir", dir as usize))
                                    .flex_1()
                                    .py_1()
                                    .rounded_md()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_xs()
                                    .font_weight(FontWeight::MEDIUM)
                                    .border_1()
                                    .border_color(rgb(if active { ACCENT } else { BORDER_SUBTLE }))
                                    .bg(rgb(if active { ACCENT } else { BG_PANEL }))
                                    .text_color(rgb(if active {
                                        0xffffff
                                    } else {
                                        TEXT_SECONDARY
                                    }))
                                    .cursor_pointer()
                                    .child(dir.label())
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        if let Some(l) = app.selected_layer_mut() {
                                            l.direction = dir;
                                        }
                                        app.restart_live_if_running();
                                        cx.notify();
                                    }))
                            })),
                    ),
            );
        }

        panel_box(&format!(
            "LAYER {} INSPECTOR — {}",
            self.selected_layer_idx + 1,
            layer.effect.label().to_uppercase()
        ))
        // Effect selection
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .w(px(72.0))
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child("Looped"),
                        )
                        .children(EffectKind::LOOPED.into_iter().map(|e| effect_pill(e, cx))),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .items_center()
                        .gap_1p5()
                        .child(
                            div()
                                .w(px(72.0))
                                .text_xs()
                                .text_color(rgb(TEXT_MUTED))
                                .child("Key-Press"),
                        )
                        .children(EffectKind::TRIGGERED.into_iter().map(|e| effect_pill(e, cx))),
                ),
        )
        // Sliders & effect-specific options
        .child(params)
        // Color editor
        .child(self.render_color_editor(&layer, cx))
    }

    fn render_color_editor(&self, layer: &LayerConfig, cx: &mut Context<Self>) -> impl IntoElement {
        let mode = layer.effect.color_mode();
        if mode == ColorMode::None {
            return div()
                .p_3()
                .rounded_md()
                .bg(rgb(BG_CARD))
                .border_1()
                .border_color(rgb(BORDER_SUBTLE))
                .text_xs()
                .text_color(rgb(TEXT_SECONDARY))
                .child(match layer.effect {
                    EffectKind::Sun => "Sun uses built-in warm solar color gradients.",
                    EffectKind::Confetti => "Confetti assigns random colors to every key automatically.",
                    _ => "This effect uses built-in colors.",
                });
        }

        let supports_random = matches!(mode, ColorMode::SingleOrRandom | ColorMode::PairOrRandom);
        let is_list = matches!(mode, ColorMode::List | ColorMode::PairOrRandom);
        let active_color = self.active_edit_color();
        let entity = cx.entity();

        let mut box_el = div()
            .flex()
            .flex_col()
            .gap_3()
            .p_3()
            .rounded_md()
            .bg(rgb(BG_CARD))
            .border_1()
            .border_color(rgb(BORDER_SUBTLE));

        // Header row: label + random toggle + hex input
        let mut top_row = div()
            .flex()
            .flex_wrap()
            .items_center()
            .justify_between()
            .gap_2()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(TEXT_SECONDARY))
                    .child(match mode {
                        ColorMode::List => "Color Gradient / List",
                        ColorMode::PairOrRandom => "Swipe Colors (2 colors or Random)",
                        _ => "Layer Color",
                    }),
            );

        let mut right_controls = div().flex().items_center().gap_2();

        if supports_random {
            let is_rand = layer.random_color;
            right_controls = right_controls.child(
                div()
                    .id("toggle-random-color")
                    .px_2p5()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(rgb(if is_rand { 0xa855f7 } else { BORDER_SUBTLE }))
                    .bg(if is_rand {
                        rgba(0xa855f730)
                    } else {
                        rgba(0x161922ff)
                    })
                    .text_xs()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(if is_rand { 0xe9d5ff } else { TEXT_SECONDARY }))
                    .cursor_pointer()
                    .child(if is_rand { "Random: ON" } else { "Random: Off" })
                    .on_click(cx.listener(|app, _, _, cx| {
                        if let Some(l) = app.selected_layer_mut() {
                            l.random_color = !l.random_color;
                        }
                        app.hex_editing = false;
                        app.restart_live_if_running();
                        cx.notify();
                    })),
            );
        }

        // Editable Hex box (when not random)
        if !layer.random_color {
            let hex_display = if self.hex_editing {
                format!("#{}|", self.hex_buffer)
            } else {
                active_color.to_hex_upper()
            };
            right_controls = right_controls.child(
                div()
                    .id("hex-input")
                    .track_focus(&self.hex_focus)
                    .flex()
                    .items_center()
                    .gap_2()
                    .px_2p5()
                    .py_1()
                    .rounded_md()
                    .bg(rgb(BG_PANEL))
                    .border_1()
                    .border_color(rgb(if self.hex_editing {
                        ACCENT_BLUE
                    } else {
                        BORDER_STRONG
                    }))
                    .cursor_pointer()
                    .on_click(cx.listener(|app, _, window, cx| {
                        app.hex_editing = true;
                        app.hex_buffer = app.active_edit_color().to_hex().to_uppercase();
                        window.focus(&app.hex_focus, cx);
                        cx.notify();
                    }))
                    .on_mouse_down_out(cx.listener(|app, _, _, cx| {
                        if app.hex_editing {
                            if let Some(c) = RgbColor::from_hex(&app.hex_buffer) {
                                app.set_active_edit_color(c, true);
                            }
                            app.hex_editing = false;
                            cx.notify();
                        }
                    }))
                    .on_key_down(cx.listener(Self::handle_hex_key_down))
                    .child(
                        div()
                            .w(px(14.0))
                            .h(px(14.0))
                            .rounded_sm()
                            .bg(rgb(active_color.to_u32()))
                            .border_1()
                            .border_color(rgba(0xffffff44)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(TEXT_PRIMARY))
                            .child(hex_display),
                    ),
            );
        }

        top_row = top_row.child(right_controls);
        box_el = box_el.child(top_row);

        if layer.random_color {
            return box_el.child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child("A fresh random RGB color is chosen each cycle / key press."),
            );
        }

        // Color list bar (Wave / Cycle / Swipe)
        if is_list {
            let max_items = if mode == ColorMode::PairOrRandom { 2 } else { 12 };
            let can_add = layer.colors.len() < max_items;
            let can_remove = layer.colors.len() > if mode == ColorMode::PairOrRandom { 2 } else { 1 };
            let sel_idx = self.selected_color_idx.min(layer.colors.len().saturating_sub(1));

            box_el = box_el.child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .gap_1p5()
                    .children(layer.colors.iter().enumerate().map(|(i, c)| {
                        let sel = i == sel_idx;
                        div()
                            .id(("color-stop", i))
                            .px_2()
                            .py_1()
                            .rounded_md()
                            .flex()
                            .items_center()
                            .gap_1p5()
                            .border_2()
                            .border_color(rgb(if sel { 0xffffff } else { BORDER_SUBTLE }))
                            .bg(rgb(BG_PANEL))
                            .cursor_pointer()
                            .child(
                                div()
                                    .w(px(16.0))
                                    .h(px(16.0))
                                    .rounded_sm()
                                    .bg(rgb(c.to_u32())),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(if sel { TEXT_PRIMARY } else { TEXT_MUTED }))
                                    .child(format!("{}", i + 1)),
                            )
                            .on_click(cx.listener(move |app, _, _, cx| {
                                app.selected_color_idx = i;
                                app.hex_editing = false;
                                cx.notify();
                            }))
                    }))
                    .when(can_add, |d| {
                        d.child(
                            small_button("add-stop", "+ Add", BG_PANEL, TEXT_PRIMARY).on_click(
                                cx.listener(|app, _, _, cx| {
                                    if let Some(l) = app.selected_layer_mut() {
                                        let next_c = RgbColor::new(0, 200, 255);
                                        l.colors.push(next_c);
                                        app.selected_color_idx = l.colors.len() - 1;
                                    }
                                    app.restart_live_if_running();
                                    cx.notify();
                                }),
                            ),
                        )
                    })
                    .when(can_remove, |d| {
                        d.child(
                            small_button("rem-stop", "− Remove", BG_PANEL, 0xfca5a5).on_click(
                                cx.listener(move |app, _, _, cx| {
                                    if let Some(l) = app.selected_layer_mut()
                                        && l.colors.len() > 1
                                        && sel_idx < l.colors.len()
                                    {
                                        l.colors.remove(sel_idx);
                                        app.selected_color_idx = sel_idx.min(l.colors.len() - 1);
                                    }
                                    app.restart_live_if_running();
                                    cx.notify();
                                }),
                            ),
                        )
                    })
                    .child(
                        small_button("reset-stops", "Reset Default", BG_PANEL, TEXT_SECONDARY)
                            .on_click(cx.listener(|app, _, _, cx| {
                                if let Some(l) = app.selected_layer_mut() {
                                    l.colors = l.effect.default_colors();
                                    app.selected_color_idx = 0;
                                }
                                app.restart_live_if_running();
                                cx.notify();
                            })),
                    ),
            );
        }

        // Swatches row
        box_el = box_el.child(
            div()
                .flex()
                .flex_wrap()
                .gap_1p5()
                .children(SWATCH_COLORS.iter().enumerate().map(|(i, (_name, c))| {
                    let is_match = active_color == *c;
                    let col = *c;
                    div()
                        .id(("swatch", i))
                        .w(px(22.0))
                        .h(px(22.0))
                        .rounded_md()
                        .bg(rgb(col.to_u32()))
                        .border_2()
                        .border_color(rgb(if is_match { 0xffffff } else { BORDER_SUBTLE }))
                        .cursor_pointer()
                        .hover(|s| s.border_color(rgb(0xd1d5db)))
                        .on_click(cx.listener(move |app, _, _, cx| {
                            app.hex_editing = false;
                            app.set_active_edit_color(col, true);
                            cx.notify();
                        }))
                })),
        );

        // RGB sliders
        let channels: [(&'static str, u8, u32, SliderId); 3] = [
            ("R", active_color.r, 0xef4444, SliderId::ColorR),
            ("G", active_color.g, 0x22c55e, SliderId::ColorG),
            ("B", active_color.b, 0x3b82f6, SliderId::ColorB),
        ];

        box_el = box_el.child(div().flex().flex_wrap().gap_3().children(
            channels.into_iter().map(|(label, val, tint, sid)| {
                let e_ch = entity.clone();
                let is_drag = self.dragging_slider == Some(sid);
                div()
                    .flex_1()
                    .min_w(px(140.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .w(px(14.0))
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(tint))
                            .child(label),
                    )
                    .child(div().flex_1().child(render_slider(
                        val as f32 / 255.0,
                        rgb(tint),
                        is_drag,
                        true,
                        move |frac, phase, _w, cx| {
                            let byte = (frac * 255.0).round().clamp(0.0, 255.0) as u8;
                            e_ch.update(cx, |app, cx| {
                                let mut c = app.active_edit_color();
                                match sid {
                                    SliderId::ColorR => c.r = byte,
                                    SliderId::ColorG => c.g = byte,
                                    SliderId::ColorB => c.b = byte,
                                    _ => {}
                                }
                                app.hex_editing = false;
                                let commit = matches!(phase, SliderPhase::Commit);
                                app.set_active_edit_color(c, commit);
                                match phase {
                                    SliderPhase::Start | SliderPhase::Move => {
                                        app.dragging_slider = Some(sid);
                                    }
                                    SliderPhase::Commit => {
                                        app.dragging_slider = None;
                                    }
                                }
                                cx.notify();
                            });
                        },
                    )))
                    .child(
                        div()
                            .w(px(28.0))
                            .text_xs()
                            .text_color(rgb(TEXT_SECONDARY))
                            .child(format!("{val}")),
                    )
            }),
        ));

        box_el
    }

    fn render_keyboard_picker(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(layer) = self.selected_layer().cloned() else {
            return panel_box("KEYBOARD KEY MASK").child(
                div()
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child("Select a layer to pick keys."),
            );
        };

        let all_selected = layer.keys.is_empty();
        let tint = layer.preview_color();
        let key_unit = 44.0_f32;
        let gap_px = 4.0_f32;

        let summary = if all_selected {
            "All 69 LEDs active (no key restriction). Click any key to restrict this layer.".into()
        } else {
            format!(
                "{} / 69 LEDs selected. Click keys to toggle, or click \"All Keys\" to clear restriction.",
                layer.keys.len()
            )
        };

        let preset_btn = |id: &'static str,
                          label: &'static str,
                          keys: Option<&'static [&'static str]>,
                          cx: &mut Context<Self>| {
            small_button(id, label, BG_CARD, TEXT_PRIMARY).on_click(cx.listener(
                move |app, _, _, cx| {
                    app.set_keys_preset(keys, cx);
                },
            ))
        };

        let mut kb_rows = div()
            .flex()
            .flex_col()
            .gap(px(gap_px))
            .p_3()
            .rounded_lg()
            .bg(rgb(0x0b0d12))
            .border_1()
            .border_color(rgb(BORDER_SUBTLE));

        for (r_idx, row) in KEYBOARD_ROWS.iter().enumerate() {
            let mut row_div = div()
                .id(("kb-row", r_idx))
                .flex()
                .items_center()
                .gap(px(gap_px));

            for (k_idx, key) in row.iter().enumerate() {
                if key.gap_before > 0.0 {
                    row_div = row_div.child(div().w(px(key.gap_before * key_unit)));
                }

                let is_lit = all_selected || layer.keys.contains(key.name);
                let is_explicit = !all_selected && layer.keys.contains(key.name);
                let w_px = key.width * key_unit + (key.width - 1.0).max(0.0) * gap_px;

                let (bg_col, border_col, text_col) = if is_explicit {
                    (
                        rgba(tint.to_rgba_u32(0xd8)),
                        rgb(0xffffff),
                        rgb(if tint.is_light() { 0x0f172a } else { 0xffffff }),
                    )
                } else if all_selected {
                    (
                        rgba(tint.to_rgba_u32(0x3a)),
                        rgba(tint.to_rgba_u32(0x99)),
                        rgb(TEXT_PRIMARY),
                    )
                } else {
                    (rgba(BG_KEY_OFF << 8 | 0xff), rgb(BORDER_SUBTLE), rgb(TEXT_MUTED))
                };

                let key_name = key.name;
                row_div = row_div.child(
                    div()
                        .id(("key", r_idx * 32 + k_idx))
                        .w(px(w_px))
                        .h(px(40.0))
                        .rounded_md()
                        .bg(bg_col)
                        .border_1()
                        .border_color(border_col)
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .font_weight(if is_lit {
                            FontWeight::SEMIBOLD
                        } else {
                            FontWeight::NORMAL
                        })
                        .text_color(text_col)
                        .cursor_pointer()
                        .hover(|s| s.border_color(rgb(0xffffff)))
                        .child(key.label)
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(move |app, _, _, cx| {
                                app.toggle_key(key_name, cx);
                            }),
                        ),
                );
            }
            kb_rows = kb_rows.child(row_div);
        }

        panel_box("KEYBOARD KEY MASK (HYPERX ALLOY ORIGINS 65 — US 67-KEY)")
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(div().text_xs().text_color(rgb(TEXT_SECONDARY)).child(summary))
                    .child(
                        div()
                            .flex()
                            .flex_wrap()
                            .gap_1()
                            .child(preset_btn("preset-all", "All Keys", None, cx))
                            .child(preset_btn("preset-wasd", "WASD", Some(PRESET_WASD), cx))
                            .child(preset_btn("preset-arrows", "Arrows", Some(PRESET_ARROWS), cx))
                            .child(preset_btn("preset-num", "Numbers", Some(PRESET_NUMBERS), cx))
                            .child(preset_btn("preset-alpha", "Alphas", Some(PRESET_ALPHAS), cx))
                            .child(preset_btn("preset-mod", "Modifiers", Some(PRESET_MODIFIERS), cx))
                            .child(preset_btn("preset-nav", "Nav", Some(PRESET_NAV), cx))
                            .child(
                                small_button("preset-invert", "Invert", BG_CARD, TEXT_PRIMARY)
                                    .on_click(cx.listener(|app, _, _, cx| app.invert_keys(cx))),
                            ),
                    ),
            )
            .child(
                div()
                    .id("kb-scroll")
                    .w_full()
                    .overflow_x_scroll()
                    .child(kb_rows),
            )
            .child(
                div()
                    .w_full()
                    .px_2p5()
                    .py_1p5()
                    .rounded_md()
                    .bg(rgb(0x0b0d12))
                    .border_1()
                    .border_color(rgb(BORDER_SUBTLE))
                    .text_xs()
                    .text_color(rgb(TEXT_MUTED))
                    .child(self.command_preview()),
            )
    }
}

impl Drop for Origins65App {
    fn drop(&mut self) {
        self.driver.shutdown();
    }
}

impl Render for Origins65App {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .flex()
            .flex_col()
            .bg(rgb(BG_APP))
            .text_color(rgb(TEXT_PRIMARY))
            .child(self.render_header(cx))
            .child(self.render_banners(cx))
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .gap_4()
                    .p_4()
                    // Left sidebar: Device + Layers + Save
                    .child(
                        div()
                            .id("left-col")
                            .w(px(330.0))
                            .flex_shrink_0()
                            .h_full()
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(self.render_device_panel(cx))
                            .child(self.render_layers_panel(cx))
                            .child(self.render_save_panel(cx)),
                    )
                    // Right main area: Layer Inspector + Interactive Keyboard
                    .child(
                        div()
                            .id("right-col")
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .overflow_y_scroll()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(self.render_layer_inspector(cx))
                            .child(self.render_keyboard_picker(cx)),
                    ),
            )
    }
}

fn panel_box(title: &str) -> Div {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap_2p5()
        .p_3()
        .rounded_lg()
        .bg(rgb(BG_PANEL))
        .border_1()
        .border_color(rgb(BORDER_SUBTLE))
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(TEXT_MUTED))
                .child(SharedString::from(title.to_string())),
        )
}

fn small_button(id: &'static str, label: &'static str, bg: u32, fg: u32) -> Stateful<Div> {
    div()
        .id(id)
        .px_2p5()
        .py_1()
        .rounded_md()
        .bg(rgb(bg))
        .border_1()
        .border_color(rgb(BORDER_SUBTLE))
        .text_xs()
        .font_weight(FontWeight::MEDIUM)
        .text_color(rgb(fg))
        .cursor_pointer()
        .hover(|s| s.bg(rgb(BG_CARD_HOVER)))
        .child(label)
}

fn icon_btn(
    id: (&'static str, usize),
    glyph: &'static str,
    enabled: bool,
    handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(22.0))
        .h(px(22.0))
        .rounded_sm()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .bg(rgb(BG_PANEL))
        .border_1()
        .border_color(rgb(BORDER_SUBTLE))
        .text_color(rgb(if enabled { TEXT_PRIMARY } else { 0x3f4759 }))
        .when(enabled, |d| {
            d.cursor_pointer()
                .hover(|s| s.bg(rgb(BORDER_STRONG)))
                .on_click(handler)
        })
        .child(glyph)
}

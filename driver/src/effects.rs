//! Port of the NGENUITY 2.39 keyboard effect engine (`tools/o65/effects.py`).
//!
//! Timing follows `EffectImplBase.RunEffect`: every effect has a cycle length `speed` (ms) and a
//! `TimePosition` (`tp`) in `[0, 1]`; `tp` hits `1.0` on the frame a cycle ends. In sync mode
//! (rendering frames for onboard storage) `tp` is derived from the frame number instead of the
//! wall clock.
//!
//! Layers are composited like `EffectEngine.CompositeEffects`: layer 0 is on top, colors carry
//! alpha, lower layers show through until a key is opaque, and the rest is blended over black.

use std::collections::{HashMap, HashSet};
use std::f64::consts::PI;
use std::time::Instant;

use rand::Rng;

use crate::layout::{Key, NUM_LEDS};
use crate::model::Model;

pub type Rgb = (u8, u8, u8);
pub type Rgba = (u8, u8, u8, f64);
pub type Frame = [Rgb; NUM_LEDS];

/// Origins 65 onboard playback rate; see [`Model::sync_fps`] for other keyboards.
pub const SYNC_FPS: u32 = Model::Origins65.sync_fps();
pub const SYNC_INTERVAL: u8 = Model::Origins65.sync_interval();
pub const MAX_SYNC_FRAMES: usize = Model::Origins65.max_sync_frames();

pub const RGB_RAINBOW: [Rgb; 10] = [
    (255, 0, 0),
    (255, 151, 0),
    (206, 255, 0),
    (50, 255, 0),
    (0, 255, 103),
    (0, 255, 255),
    (0, 104, 255),
    (43, 0, 255),
    (205, 0, 255),
    (255, 0, 158),
];

pub const CYCLE_COLORS: [Rgb; 5] = [
    (255, 0, 0),
    (255, 255, 0),
    (0, 255, 0),
    (0, 255, 255),
    (0, 0, 255),
];

pub const THEME_COLOR: Rgb = (242, 0, 0);

pub const NAMED_COLORS: &[(&str, Rgb)] = &[
    ("red", (255, 0, 0)),
    ("green", (0, 255, 0)),
    ("blue", (0, 0, 255)),
    ("white", (255, 255, 255)),
    ("black", (0, 0, 0)),
    ("off", (0, 0, 0)),
    ("yellow", (255, 200, 0)),
    ("cyan", (0, 255, 255)),
    ("magenta", (255, 0, 255)),
    ("purple", (128, 0, 255)),
    ("orange", (255, 80, 0)),
    ("pink", (255, 60, 120)),
];

pub fn blank_frame() -> Frame {
    [(0, 0, 0); NUM_LEDS]
}

pub fn scale_frame(frame: &Frame, k: f64) -> Frame {
    let mut out = [(0, 0, 0); NUM_LEDS];
    for (dst, &(r, g, b)) in out.iter_mut().zip(frame.iter()) {
        *dst = (
            (r as f64 * k) as u8,
            (g as f64 * k) as u8,
            (b as f64 * k) as u8,
        );
    }
    out
}

pub fn rand_color() -> Rgb {
    let mut rng = rand::rng();
    (
        rng.random_range(0..=255),
        rng.random_range(0..=255),
        rng.random_range(0..=255),
    )
}

/// Color.GetGradientColor: truncating byte cast matching Python's `int(a + t * (b - a)) & 0xFF`.
#[inline]
pub fn lerp(c1: Rgb, c2: Rgb, t: f64) -> Rgb {
    let ch = |a: u8, b: u8| -> u8 {
        let v = (a as f64 + t * (b as f64 - a as f64)) as i64;
        (v & 0xFF) as u8
    };
    (ch(c1.0, c2.0), ch(c1.1, c2.1), ch(c1.2, c2.2))
}

#[inline]
pub fn with_alpha(c: Rgb, a: f64) -> Rgba {
    (c.0, c.1, c.2, a.clamp(0.0, 1.0))
}

/// Parse a named color or `#RRGGBB` / `RRGGBB` hex string.
pub fn parse_color(s: &str) -> Result<Rgb, String> {
    let lower = s.trim().to_ascii_lowercase();
    if let Some(&(_, rgb)) = NAMED_COLORS.iter().find(|&&(name, _)| name == lower) {
        return Ok(rgb);
    }
    let hex = lower.trim_start_matches('#');
    if hex.len() != 6 {
        return Err(format!("bad color '{hex}'"));
    }
    let r = u8::from_str_radix(&hex[0..2], 16).map_err(|_| format!("bad color '{hex}'"))?;
    let g = u8::from_str_radix(&hex[2..4], 16).map_err(|_| format!("bad color '{hex}'"))?;
    let b = u8::from_str_radix(&hex[4..6], 16).map_err(|_| format!("bad color '{hex}'"))?;
    Ok((r, g, b))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    Solid,
    Breathing,
    Cycle,
    Wave,
    Swipe,
    Confetti,
    Sun,
    Twilight,
    Fade,
    Explosion,
    Flame,
    Implosion,
}

impl EffectKind {
    pub const ALL: [EffectKind; 12] = [
        Self::Solid,
        Self::Breathing,
        Self::Cycle,
        Self::Wave,
        Self::Swipe,
        Self::Confetti,
        Self::Sun,
        Self::Twilight,
        Self::Fade,
        Self::Explosion,
        Self::Flame,
        Self::Implosion,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Solid => "solid",
            Self::Breathing => "breathing",
            Self::Cycle => "cycle",
            Self::Wave => "wave",
            Self::Swipe => "swipe",
            Self::Confetti => "confetti",
            Self::Sun => "sun",
            Self::Twilight => "twilight",
            Self::Fade => "fade",
            Self::Explosion => "explosion",
            Self::Flame => "flame",
            Self::Implosion => "implosion",
        }
    }

    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.name() == name)
    }

    pub const fn min_speed(self) -> u32 {
        match self {
            Self::Solid => 500,
            Self::Breathing => 500,
            Self::Cycle => 500,
            Self::Wave => 1000,
            Self::Swipe => 500,
            Self::Confetti => 500,
            Self::Sun => 500,
            Self::Twilight => 300,
            Self::Fade => 1000,
            Self::Explosion => 600,
            Self::Flame => 1000,
            Self::Implosion => 5,
        }
    }

    pub const fn max_speed(self) -> u32 {
        match self {
            Self::Solid => 3000,
            Self::Breathing => 4000,
            Self::Cycle => 4000,
            Self::Wave => 6000,
            Self::Swipe => 3000,
            Self::Confetti => 3000,
            Self::Sun => 1375,
            Self::Twilight => 1000,
            Self::Fade => 5000,
            Self::Explosion => 10000,
            Self::Flame => 12000,
            Self::Implosion => 50,
        }
    }

    pub const fn default_speed(self) -> u32 {
        match self {
            Self::Solid => 1000,
            Self::Breathing => 2250,
            Self::Cycle => 2250,
            Self::Wave => 3000,
            Self::Swipe => 2000,
            Self::Confetti => 1000,
            Self::Sun => 1000,
            Self::Twilight => 1000,
            Self::Fade => 2000,
            Self::Explosion => 1000,
            Self::Flame => 2000,
            Self::Implosion => 20,
        }
    }

    pub const fn is_triggered(self) -> bool {
        matches!(
            self,
            Self::Fade | Self::Explosion | Self::Flame | Self::Implosion
        )
    }

    pub const fn onboard_trigger_code(self) -> Option<u8> {
        match self {
            Self::Fade => Some(4),
            Self::Explosion => Some(5),
            Self::Flame => Some(6),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    Left,
    Right,
    Up,
    Down,
}

impl SwipeDirection {
    pub fn from_name(s: &str) -> Self {
        match s {
            "left" => Self::Left,
            "up" => Self::Up,
            "down" => Self::Down,
            _ => Self::Right,
        }
    }
}

#[derive(Debug, Clone)]
pub struct LayerParams {
    pub kind: EffectKind,
    pub speed: Option<u32>,
    pub color: Option<Rgb>,
    pub colors: Option<Vec<Rgb>>,
    pub keys: Option<HashSet<String>>,
    pub opacity: i32,
    pub random_color: bool,
    pub angle: f64,
    pub direction: SwipeDirection,
    pub model: Model,
}

impl LayerParams {
    pub fn new(kind: EffectKind) -> Self {
        Self::for_model(kind, Model::Origins65)
    }

    pub fn for_model(kind: EffectKind, model: Model) -> Self {
        Self {
            model,
            kind,
            speed: None,
            color: None,
            colors: None,
            keys: None,
            opacity: 100,
            random_color: false,
            angle: 180.0,
            direction: SwipeDirection::Right,
        }
    }
}

/// Parse a CLI layer specification such as `"wave:speed=4000,angle=90"` or
/// `"solid:color=ff0000,keys=w+a+s+d"`.
///
/// Returns `(Layer, Option<warning_note>)` where `warning_note` is populated if `speed` lies
/// outside NGENUITY's standard speed range for that effect. Keys and speeds are those of the
/// Origins 65; use [`parse_layer_spec_for`] for another keyboard.
pub fn parse_layer_spec(spec: &str) -> Result<(Layer, Option<String>), String> {
    parse_layer_spec_for(spec, Model::Origins65)
}

/// [`parse_layer_spec`] for a given keyboard model (key names and speed ranges differ).
pub fn parse_layer_spec_for(spec: &str, model: Model) -> Result<(Layer, Option<String>), String> {
    let (name_raw, rest) = match spec.split_once(':') {
        Some((n, r)) => (n, r),
        None => (spec, ""),
    };
    let name = name_raw.to_ascii_lowercase();
    let kind = EffectKind::from_name(&name)
        .ok_or_else(|| format!("unknown effect '{name}'; run `origins65 effects`"))?;

    let mut params = LayerParams::for_model(kind, model);
    let mut explicit_speed: Option<i64> = None;

    for item in rest.split(',').filter(|s| !s.is_empty()) {
        let (k_raw, v) = match item.split_once('=') {
            Some((k, v)) => (k, v),
            None => (item, ""),
        };
        let k = k_raw.trim().to_ascii_lowercase();
        match k.as_str() {
            "color" => {
                if v.eq_ignore_ascii_case("random") {
                    params.random_color = true;
                } else {
                    params.color = Some(parse_color(v)?);
                }
            }
            "colors" => {
                let mut cols = Vec::new();
                for part in v.split('/') {
                    cols.push(parse_color(part)?);
                }
                params.colors = Some(cols);
            }
            "keys" => {
                let names: Vec<String> = v.split('+').map(|n| n.to_ascii_lowercase()).collect();
                let bad: Vec<&str> = names
                    .iter()
                    .filter(|n| model.key_by_name(n).is_none())
                    .map(String::as_str)
                    .collect();
                if !bad.is_empty() {
                    let formatted = bad
                        .iter()
                        .map(|b| format!("'{b}'"))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return Err(format!("unknown keys [{formatted}]; run `origins65 keys`"));
                }
                params.keys = Some(names.into_iter().collect());
            }
            "speed" => {
                let val: i64 = v
                    .trim()
                    .parse()
                    .map_err(|_| format!("invalid integer for speed: '{v}'"))?;
                explicit_speed = Some(val);
                if val > 0 {
                    params.speed = Some(val as u32);
                } else {
                    params.speed = None;
                }
            }
            "opacity" => {
                let val: i32 = v
                    .trim()
                    .parse()
                    .map_err(|_| format!("invalid integer for opacity: '{v}'"))?;
                params.opacity = val;
            }
            "angle" => {
                let val: f64 = v
                    .trim()
                    .parse()
                    .map_err(|_| format!("invalid float for angle: '{v}'"))?;
                params.angle = val;
            }
            "direction" => {
                params.direction = SwipeDirection::from_name(&v.to_ascii_lowercase());
            }
            _ => {
                return Err(format!("unknown option '{k}' in '{spec}'"));
            }
        }
    }

    let (min_speed, max_speed, _) = model.speed_range(kind);
    let warning = explicit_speed.and_then(|sp| {
        if sp < min_speed as i64 || sp > max_speed as i64 {
            Some(format!(
                "note: {} speed {} is outside NGENUITY's range {}..{} ms",
                kind.name(),
                sp,
                min_speed,
                max_speed
            ))
        } else {
            None
        }
    });

    Ok((Layer::from_params(params), warning))
}

const SUN_THEMES: &[&[(Rgb, f64)]] = &[
    &[((255, 178, 0), 0.0), ((255, 97, 0), 0.5)],
    &[
        ((255, 178, 0), 0.0),
        ((255, 97, 0), 0.25),
        ((255, 20, 0), 0.5),
        ((255, 183, 0), 0.75),
    ],
    &[
        ((255, 130, 0), 0.0),
        ((255, 153, 0), 0.2),
        ((255, 161, 0), 0.4),
        ((255, 168, 0), 0.6),
        ((255, 97, 0), 0.8),
    ],
];

const FLAME_LEVELS: &[(usize, &[(i32, i32)])] = &[
    (1, &[(-1, -1), (-2, -2)]),
    (2, &[(0, -1), (1, -1), (1, -2), (2, -3)]),
    (3, &[(0, -2), (1, -3), (1, -4)]),
    (4, &[(0, -3), (-1, -3), (-2, -4)]),
    (5, &[(0, -4)]),
    (6, &[(-1, -5)]),
    (7, &[(-1, -6)]),
];

#[derive(Debug, Clone)]
enum EffectState {
    Solid,
    Breathing {
        color: Rgb,
    },
    Cycle,
    Wave {
        angle: f64,
    },
    Swipe {
        direction: SwipeDirection,
        state: u8,
        c1: Rgb,
        c2: Rgb,
    },
    Confetti {
        state: usize,
        items: Vec<(usize, Rgb, Rgb)>,
        first: Vec<(usize, Rgb)>,
    },
    Sun {
        state: Vec<(usize, usize)>,
    },
    Twilight {
        active: HashMap<usize, (Rgb, u32, bool)>,
        inited: bool,
        cycles: usize,
    },
    TriggerTemplate,
    FadeInstance {
        key: &'static Key,
        color: Rgb,
        done: bool,
    },
    ExplosionInstance {
        key: &'static Key,
        color: Rgb,
        done: bool,
    },
    FlameInstance {
        key: &'static Key,
        color: Rgb,
        done: bool,
        level: usize,
        lit: HashMap<usize, Instant>,
        last_step: Instant,
        state: i32,
    },
    ImplosionInstance {
        key: &'static Key,
        color: Rgb,
        done: bool,
        start: Instant,
        ticks_fade: f64,
        radius: i32,
        hits: HashMap<usize, f64>,
        last_shrink: f64,
    },
}

#[derive(Debug, Clone)]
pub struct Layer {
    pub model: Model,
    pub kind: EffectKind,
    pub speed: u32,
    pub colors: Option<Vec<Rgb>>,
    pub random_color: bool,
    pub keys: Vec<&'static Key>,
    pub opacity: i32,
    pub sync: bool,
    pub fps: u32,
    pub fpc: usize,
    pub ref_fpc: f64,
    pub cycle_count: usize,
    timing: Instant,
    frame: usize,
    tp: f64,
    state: EffectState,
}

impl Layer {
    pub fn from_params(params: LayerParams) -> Self {
        let speed = match params.speed {
            Some(s) if s > 0 => s,
            _ => params.model.default_speed(params.kind),
        };
        let colors = match (params.colors, params.color) {
            (Some(cs), _) if !cs.is_empty() => Some(cs),
            (_, Some(c)) => Some(vec![c]),
            _ => None,
        };
        let keys: Vec<&'static Key> = params
            .model
            .keys()
            .iter()
            .filter(|k| match &params.keys {
                None => true,
                Some(set) => set.contains(k.name),
            })
            .collect();

        let initial_state = match params.kind {
            EffectKind::Solid => EffectState::Solid,
            EffectKind::Breathing => EffectState::Breathing { color: THEME_COLOR },
            EffectKind::Cycle => EffectState::Cycle,
            EffectKind::Wave => EffectState::Wave {
                angle: params.angle,
            },
            EffectKind::Swipe => EffectState::Swipe {
                direction: params.direction,
                state: 0,
                c1: (255, 0, 0),
                c2: (0, 255, 0),
            },
            EffectKind::Confetti => EffectState::Confetti {
                state: 0,
                items: Vec::new(),
                first: Vec::new(),
            },
            EffectKind::Sun => EffectState::Sun { state: Vec::new() },
            EffectKind::Twilight => EffectState::Twilight {
                active: HashMap::new(),
                inited: false,
                cycles: 0,
            },
            EffectKind::Fade | EffectKind::Explosion | EffectKind::Flame | EffectKind::Implosion => {
                EffectState::TriggerTemplate
            }
        };

        let mut layer = Self {
            model: params.model,
            kind: params.kind,
            speed,
            colors,
            random_color: params.random_color,
            keys,
            opacity: params.opacity,
            sync: false,
            fps: SYNC_FPS,
            fpc: 1,
            ref_fpc: 1.0,
            cycle_count: 1,
            timing: Instant::now(),
            frame: 0,
            tp: 0.0,
            state: initial_state,
        };
        layer.reset();
        layer
    }

    pub fn primary_color(&self) -> Rgb {
        self.colors
            .as_ref()
            .and_then(|cs| cs.first().copied())
            .unwrap_or(THEME_COLOR)
    }

    pub fn is_triggered(&self) -> bool {
        self.kind.is_triggered()
    }

    pub fn is_done(&self) -> bool {
        match &self.state {
            EffectState::FadeInstance { done, .. }
            | EffectState::ExplosionInstance { done, .. }
            | EffectState::FlameInstance { done, .. }
            | EffectState::ImplosionInstance { done, .. } => *done,
            _ => false,
        }
    }

    pub fn reset(&mut self) {
        self.timing = Instant::now();
        self.frame = 0;
        self.tp = 0.0;
        self.setup();
    }

    fn setup(&mut self) {
        let primary = self.primary_color();
        match &mut self.state {
            EffectState::Solid | EffectState::Wave { .. } | EffectState::TriggerTemplate => {}
            EffectState::Breathing { color } => {
                *color = if self.random_color {
                    rand_color()
                } else {
                    primary
                };
                self.cycle_count = 1;
            }
            EffectState::Cycle => {
                self.cycle_count = match &self.colors {
                    Some(cs) if cs.len() == 1 => 1,
                    _ => 2,
                };
            }
            EffectState::Swipe { state, c1, c2, .. } => {
                self.cycle_count = 2;
                *state = 0;
                if self.random_color {
                    *c1 = rand_color();
                    *c2 = rand_color();
                } else {
                    let (a, b) = swipe_pair(self.colors.as_deref());
                    *c1 = a;
                    *c2 = b;
                }
            }
            EffectState::Confetti {
                state,
                items,
                first,
            } => {
                self.cycle_count = 4;
                *state = 0;
                items.clear();
                first.clear();
            }
            EffectState::Sun { state } => {
                self.cycle_count = 4;
                let mut rng = rand::rng();
                *state = self
                    .keys
                    .iter()
                    .map(|k| (k.led, rng.random_range(0..3)))
                    .collect();
            }
            EffectState::Twilight {
                active,
                inited,
                cycles,
            } => {
                self.cycle_count = 8;
                active.clear();
                *inited = false;
                *cycles = 0;
            }
            EffectState::FadeInstance { done, .. } | EffectState::ExplosionInstance { done, .. } => {
                *done = false;
            }
            EffectState::FlameInstance {
                key,
                done,
                level,
                lit,
                last_step,
                state,
                ..
            } => {
                let now = Instant::now();
                *done = false;
                *level = 0;
                lit.clear();
                lit.insert(key.led, now);
                *last_step = now;
                *state = key.row as i32;
            }
            EffectState::ImplosionInstance {
                key,
                done,
                start,
                ticks_fade,
                radius,
                hits,
                last_shrink,
                ..
            } => {
                *done = false;
                *start = Instant::now();
                *ticks_fade = (self.speed as f64 / 5.0) / (20.0 / 5.0) * 100.0;
                *radius = (key.col as i32).max(self.model.max_col() as i32 - key.col as i32);
                hits.clear();
                *last_shrink = 0.0;
            }
        }
    }

    pub fn configure_sync(&mut self, fps: u32) {
        self.sync = true;
        self.fps = fps;
        self.reset();
        if self.kind == EffectKind::Solid {
            self.fpc = 1;
            self.ref_fpc = 1.0;
        } else {
            self.fpc = (((fps as f64) * (self.speed as f64) / 1000.0) as usize).max(1);
            self.ref_fpc = self.fpc as f64;
        }
    }

    pub fn spawn_trigger(&self, key: &'static Key) -> Layer {
        let color = if self.random_color {
            rand_color()
        } else {
            self.primary_color()
        };
        let now = Instant::now();
        let state = match self.kind {
            EffectKind::Fade => EffectState::FadeInstance {
                key,
                color,
                done: false,
            },
            EffectKind::Explosion => EffectState::ExplosionInstance {
                key,
                color,
                done: false,
            },
            EffectKind::Flame => {
                let mut lit = HashMap::new();
                lit.insert(key.led, now);
                EffectState::FlameInstance {
                    key,
                    color,
                    done: false,
                    level: 0,
                    lit,
                    last_step: now,
                    state: key.row as i32,
                }
            }
            EffectKind::Implosion => EffectState::ImplosionInstance {
                key,
                color,
                done: false,
                start: now,
                ticks_fade: (self.speed as f64 / 5.0) / (20.0 / 5.0) * 100.0,
                radius: (key.col as i32).max(self.model.max_col() as i32 - key.col as i32),
                hits: HashMap::new(),
                last_shrink: 0.0,
            },
            _ => EffectState::TriggerTemplate,
        };
        let mut inst = Layer {
            model: self.model,
            kind: self.kind,
            speed: self.speed,
            colors: self.colors.clone(),
            random_color: self.random_color,
            keys: self.keys.clone(),
            opacity: self.opacity,
            sync: false,
            fps: self.fps,
            fpc: self.fpc,
            ref_fpc: self.ref_fpc,
            cycle_count: self.cycle_count,
            timing: now,
            frame: 0,
            tp: 0.0,
            state,
        };
        inst.reset();
        inst
    }

    fn advance(&mut self) {
        if self.sync {
            if self.fpc == 1 {
                self.tp = 1.0;
            } else {
                let n = ((self.frame as f64) % self.ref_fpc) as usize;
                self.tp = (n as f64) / ((self.fpc - 1) as f64);
                if self.tp > 1.0 {
                    self.tp = 1.0;
                }
            }
            self.frame += 1;
        } else {
            let now = Instant::now();
            let elapsed = ((now - self.timing).as_secs_f64() * 1000.0).min(self.speed as f64);
            self.tp = elapsed / (self.speed as f64);
            if self.tp >= 1.0 {
                self.tp = 1.0;
                self.timing = Instant::now();
            }
        }

        if let EffectState::Twilight { inited, .. } = &self.state
            && !*inited
            && !self.sync
        {
            self.tp = 0.0;
        }
    }

    pub fn run(&mut self) -> Vec<(usize, Rgba)> {
        if !matches!(self.state, EffectState::ImplosionInstance { .. }) {
            self.advance();
        }
        self.render()
    }

    fn render(&mut self) -> Vec<(usize, Rgba)> {
        let tp = self.tp;
        let sync = self.sync;
        let random_color = self.random_color;
        let primary = self.primary_color();
        let cycle_count = self.cycle_count;
        let speed = self.speed;

        match &mut self.state {
            EffectState::Solid => self
                .keys
                .iter()
                .map(|k| (k.led, with_alpha(primary, 1.0)))
                .collect(),

            EffectState::Breathing { color } => {
                let n = if tp <= 0.5 { tp } else { 1.0 - tp };
                let a = (n * PI).sin();
                let c = *color;
                let out = self
                    .keys
                    .iter()
                    .map(|k| (k.led, with_alpha(c, a)))
                    .collect();
                if tp >= 1.0 && random_color {
                    *color = rand_color();
                }
                out
            }

            EffectState::Cycle => {
                let base: &[Rgb] = match &self.colors {
                    Some(cs) if !cs.is_empty() => cs.as_slice(),
                    _ => &CYCLE_COLORS,
                };
                let mut stops = Vec::with_capacity(base.len() + 1);
                stops.extend_from_slice(base);
                stops.push(base[0]);
                let n = stops.len() - 1;
                let t = if tp > 1.0 { tp - 1.0 } else { tp };
                let mut c = stops[0];
                for j in 0..n {
                    let p0 = (j as f64) / (n as f64);
                    let p1 = ((j + 1) as f64) / (n as f64);
                    if p0 <= t && t <= p1 {
                        c = lerp(stops[j], stops[j + 1], (t - p0) / (p1 - p0));
                    }
                }
                self.keys
                    .iter()
                    .map(|k| (k.led, with_alpha(c, 1.0)))
                    .collect()
            }

            EffectState::Wave { angle } => {
                let mut cols: Vec<Rgb> = match &self.colors {
                    Some(cs) if !cs.is_empty() => cs.clone(),
                    _ => RGB_RAINBOW.to_vec(),
                };
                if cols.len() < 2 {
                    cols.push(cols[0]);
                }
                let len = cols.len();
                let stops: Vec<(f64, Rgb)> = cols
                    .iter()
                    .enumerate()
                    .map(|(i, &c)| (0.5 / (len as f64) + (i as f64) / (len as f64), c))
                    .collect();
                let mut width = (self.model.max_col() as i32) + 1;
                width += width / ((stops.len() as i32) - 1);
                let ang = *angle / 180.0 * PI;
                let cx = (width / 2) as f64;
                let cy = 3.0_f64;
                let width_f = width as f64;
                let first_p = stops[0].0;
                let last_p = stops[stops.len() - 1].0;
                let mut w = 1.0 - last_p + first_p;
                if w != 0.0 {
                    w = 1.0 - first_p;
                }
                let wrap = lerp(stops[stops.len() - 1].1, stops[0].1, w);
                let cos_a = ang.cos();
                let sin_a = ang.sin();
                let mut out = Vec::with_capacity(self.keys.len());
                for k in &self.keys {
                    let mut x =
                        (k.col as f64 - cx) * cos_a - (k.row as f64 - cy) * sin_a + cx;
                    x += width_f * tp;
                    if x > width_f {
                        x -= width_f;
                    }
                    x /= width_f;
                    let mut color = wrap;
                    for (i, &(p, c)) in stops.iter().enumerate() {
                        if i == 0 && x < p {
                            color = lerp(wrap, c, x / p);
                            break;
                        }
                        if i == stops.len() - 1 && x > p {
                            color = lerp(c, wrap, (x - p) / (1.0 - p));
                            break;
                        }
                        if i < stops.len() - 1 && p <= x && x <= stops[i + 1].0 {
                            color = lerp(c, stops[i + 1].1, (x - p) / (stops[i + 1].0 - p));
                            break;
                        }
                    }
                    out.push((k.led, with_alpha(color, 1.0)));
                }
                out
            }

            EffectState::Swipe {
                direction,
                state,
                c1,
                c2,
            } => {
                let mut out = Vec::with_capacity(self.keys.len());
                for k in &self.keys {
                    let inside = match direction {
                        SwipeDirection::Up | SwipeDirection::Down => {
                            let n = (self.model.max_row() as i32) + 1;
                            let step = (tp * (n as f64)) as i32;
                            let edge = if *direction == SwipeDirection::Up {
                                n - step
                            } else {
                                step
                            };
                            (k.row as i32) < edge
                        }
                        SwipeDirection::Left | SwipeDirection::Right => {
                            let n = (self.model.max_col() as i32) + 1;
                            let step = (tp * (n as f64)) as i32;
                            let edge = if *direction == SwipeDirection::Left {
                                n - step
                            } else {
                                step
                            };
                            (k.col as i32) < edge
                        }
                    };
                    out.push((k.led, with_alpha(if inside { *c2 } else { *c1 }, 1.0)));
                }
                if tp >= 1.0 {
                    if random_color && !sync {
                        *c1 = *c2;
                        *c2 = rand_color();
                    } else {
                        *state ^= 1;
                        let (a, b) = swipe_pair(self.colors.as_deref());
                        if *state == 0 {
                            *c1 = a;
                            *c2 = b;
                        } else {
                            *c1 = b;
                            *c2 = a;
                        }
                    }
                }
                out
            }

            EffectState::Confetti {
                state,
                items,
                first,
            } => {
                if *state == 0 {
                    *state = 1;
                    items.clear();
                    first.clear();
                    for k in &self.keys {
                        let a = rand_color();
                        let b = rand_color();
                        items.push((k.led, a, b));
                        first.push((k.led, a));
                    }
                }
                let out: Vec<(usize, Rgba)> = items
                    .iter()
                    .map(|&(led, f, t)| (led, with_alpha(lerp(f, t, tp), 1.0)))
                    .collect();
                if tp >= 1.0 && (!sync || *state < cycle_count) {
                    for (idx, (_, f, t)) in items.iter_mut().enumerate() {
                        *f = *t;
                        *t = if sync && *state == 3 {
                            first[idx].1
                        } else {
                            rand_color()
                        };
                    }
                    *state += 1;
                }
                out
            }

            EffectState::Sun { state } => {
                let mut out = Vec::with_capacity(self.keys.len());
                for &(led, theme_idx) in state.iter() {
                    let theme = SUN_THEMES[theme_idx];
                    for (i, &(c, p)) in theme.iter().enumerate() {
                        let (c2, p2) = if i + 1 < theme.len() {
                            theme[i + 1]
                        } else {
                            (theme[0].0, 1.0)
                        };
                        if p <= tp && tp <= p2 {
                            out.push((led, with_alpha(lerp(c, c2, (tp - p) / (p2 - p)), 1.0)));
                            break;
                        }
                    }
                }
                out
            }

            EffectState::Twilight {
                active,
                inited,
                cycles,
            } => {
                let spawn_some = |active: &mut HashMap<usize, (Rgb, u32, bool)>,
                                  keys: &[&'static Key]| {
                    let mut free: Vec<usize> = keys
                        .iter()
                        .map(|k| k.led)
                        .filter(|led| !active.contains_key(led))
                        .collect();
                    let mut rng = rand::rng();
                    let count = rng.random_range(1..=3).min(free.len());
                    for _ in 0..count {
                        let idx = rng.random_range(0..free.len());
                        let led = free.remove(idx);
                        let c = if random_color { rand_color() } else { primary };
                        active.insert(led, (c, 0, true));
                    }
                };

                if !*inited {
                    *inited = true;
                    spawn_some(active, &self.keys);
                }
                let mut out = Vec::new();
                for (&led, item) in active.iter_mut() {
                    if !item.2 {
                        continue;
                    }
                    let a = if item.1 == 0 {
                        if tp < 1.0 { tp } else { 1.0 }
                    } else if item.1 == 3 {
                        1.0 - tp
                    } else if item.1 >= 4 {
                        item.2 = false;
                        continue;
                    } else {
                        1.0
                    };
                    out.push((led, with_alpha(item.0, a)));
                    if tp == 1.0 {
                        item.1 += 1;
                    }
                }
                if tp == 1.0 {
                    *cycles += 1;
                    if !sync || *cycles <= cycle_count.saturating_sub(4) {
                        spawn_some(active, &self.keys);
                    }
                }
                if self
                    .keys
                    .iter()
                    .all(|k| active.get(&k.led).is_some_and(|v| !v.2))
                {
                    active.clear();
                }
                out
            }

            EffectState::TriggerTemplate => Vec::new(),

            EffectState::FadeInstance { key, color, done } => {
                let n = if tp <= 0.5 { tp } else { 1.0 - tp };
                if tp >= 1.0 {
                    *done = true;
                }
                vec![(key.led, with_alpha(*color, n / 0.5))]
            }

            EffectState::ExplosionInstance { key, color, done } => {
                let mut out = Vec::new();
                if !self.keys.is_empty() {
                    let min_x = self.keys.iter().map(|k| k.col as i32).min().unwrap_or(0);
                    let max_x = self.keys.iter().map(|k| k.col as i32).max().unwrap_or(0);
                    let min_y = self.keys.iter().map(|k| k.row as i32).min().unwrap_or(0);
                    let max_y = self.keys.iter().map(|k| k.row as i32).max().unwrap_or(0);
                    let max_r = (((min_x - max_x).pow(2) + (min_y - max_y).pow(2)) as f64).sqrt()
                        as i32
                        + 11;
                    let r = tp * (max_r as f64);
                    for k in &self.keys {
                        let d = (k.col as f64 - key.col as f64)
                            .hypot(k.row as f64 - key.row as f64);
                        if d > r || r - d > 10.0 {
                            continue;
                        }
                        out.push((k.led, with_alpha(*color, (10.0 - (r - d)) / 10.0)));
                    }
                }
                if tp >= 1.0 {
                    *done = true;
                }
                out
            }

            EffectState::FlameInstance {
                key,
                color,
                done,
                level,
                lit,
                last_step,
                state,
            } => {
                let now = Instant::now();
                if (now - *last_step).as_secs_f64() * 1000.0 >= (speed as f64) / 50.0 {
                    *level += 1;
                    *last_step = now;
                    if *state >= 0
                        && let Some(&(_, offsets)) =
                            FLAME_LEVELS.iter().find(|&&(lvl, _)| lvl == *level)
                    {
                        for &(dx, dy) in offsets {
                            for k in &self.keys {
                                if k.col as i32 == key.col as i32 + dx
                                    && k.row as i32 == key.row as i32 + dy
                                {
                                    lit.insert(k.led, now);
                                }
                            }
                        }
                    }
                    if *level == 3 {
                        *state += 1;
                    }
                    *state -= 1;
                }
                let mut out = Vec::new();
                for (&led, &ts) in lit.iter() {
                    let n = (now - ts).as_secs_f64() * 1000.0 / (speed as f64) * 2.0;
                    if n <= 1.0 {
                        out.push((led, with_alpha(*color, 1.0 - n)));
                    }
                }
                if tp >= 1.0 {
                    *done = true;
                }
                out
            }

            EffectState::ImplosionInstance {
                key,
                color,
                done,
                start,
                ticks_fade,
                radius,
                hits,
                last_shrink,
            } => {
                let tick = (Instant::now() - *start).as_secs_f64() / 0.005;
                while *last_shrink + *ticks_fade * 0.1 <= tick && *radius >= 0 {
                    let hit_tick = *last_shrink + *ticks_fade * 0.1;
                    for k in &self.keys {
                        let dist = (k.col as f64 - key.col as f64)
                            .hypot(k.row as f64 - key.row as f64)
                            as i32;
                        if dist == *radius {
                            hits.insert(k.led, hit_tick);
                        }
                    }
                    *radius -= 1;
                    *last_shrink = hit_tick;
                }
                let mut out = Vec::new();
                for (&led, &t0) in hits.iter() {
                    let a = 1.0 - (tick - t0) / *ticks_fade;
                    if a > 0.0 {
                        out.push((led, with_alpha(*color, a)));
                    }
                }
                if *radius < 0 && out.is_empty() {
                    *done = true;
                }
                out
            }
        }
    }
}

fn swipe_pair(colors: Option<&[Rgb]>) -> (Rgb, Rgb) {
    match colors {
        Some(cs) if cs.len() > 1 => (cs[0], cs[1]),
        Some(cs) if cs.len() == 1 => (cs[0], (0, 0, 0)),
        _ => ((255, 0, 0), (0, 255, 0)),
    }
}

/// `Color.Blend`: top over bottom, both `(r, g, b, a)`.
#[inline]
pub fn blend_over(top: Rgba, bottom: Rgba) -> Rgba {
    let a1 = top.3;
    let a2 = bottom.3;
    let r = (top.0 as f64 * a1 + bottom.0 as f64 * (1.0 - a1)) as u8;
    let g = (top.1 as f64 * a1 + bottom.1 as f64 * (1.0 - a1)) as u8;
    let b = (top.2 as f64 * a1 + bottom.2 as f64 * (1.0 - a1)) as u8;
    (r, g, b, 1.0 - (1.0 - a1) * (1.0 - a2))
}

/// Composite layer outputs (`layer_results[0]` is the top layer) into `NUM_LEDS` RGB values.
pub fn composite(layer_results: &[Vec<(usize, Rgba)>], opacities: &[i32]) -> Frame {
    let mut stacks: [Vec<Rgba>; NUM_LEDS] = std::array::from_fn(|_| Vec::new());
    for (res, &op) in layer_results.iter().zip(opacities.iter()) {
        for &(led, mut c) in res {
            if led >= NUM_LEDS {
                continue;
            }
            if op != 100 {
                c.3 = c.3 * (op as f64) / 100.0;
            }
            stacks[led].push(c);
        }
    }
    let mut frame = blank_frame();
    for (led, cs) in stacks.iter().enumerate() {
        if cs.is_empty() {
            continue;
        }
        let mut c = cs[0];
        if c.3 < 1.0 {
            for &below in &cs[1..] {
                c = blend_over(c, below);
                if c.3 >= 1.0 {
                    break;
                }
            }
            if c.3 < 1.0 {
                c = blend_over(c, (0, 0, 0, 0.0));
            }
        }
        frame[led] = (c.0, c.1, c.2);
    }
    frame
}

/// Live renderer: looped layers plus per-keypress trigger instances on top.
pub struct Engine {
    pub layers: Vec<Layer>,
    pub triggers: Vec<Layer>,
    pub active: Vec<Layer>,
}

impl Engine {
    pub fn new(layers: Vec<Layer>) -> Self {
        let mut looped = Vec::new();
        let mut triggers = Vec::new();
        for l in layers {
            if l.is_triggered() {
                triggers.push(l);
            } else {
                looped.push(l);
            }
        }
        Self {
            layers: looped,
            triggers,
            active: Vec::new(),
        }
    }

    pub fn key_pressed(&mut self, key: &'static Key) {
        for t in &self.triggers {
            self.active.insert(0, t.spawn_trigger(key));
        }
    }

    pub fn frame(&mut self) -> Frame {
        let mut results = Vec::with_capacity(self.active.len() + self.layers.len());
        let mut ops = Vec::with_capacity(self.active.len() + self.layers.len());

        for inst in &mut self.active {
            results.push(inst.run());
            ops.push(100);
        }
        self.active.retain(|inst| !inst.is_done());

        // NGENUITY runs lower layers first; index 0 stays on top.
        let mut looped_results: Vec<Vec<(usize, Rgba)>> =
            self.layers.iter_mut().rev().map(|l| l.run()).collect();
        looped_results.reverse();
        results.extend(looped_results);
        ops.extend(self.layers.iter().map(|l| l.opacity));

        composite(&results, &ops)
    }
}

/// `EffectEngine.Sync`: render a seamless loop of `<= max_frames` frames for onboard storage.
pub fn render_sync_frames(layers: &[Layer], fps: u32, max_frames: usize) -> Vec<Frame> {
    let mut looped: Vec<Layer> = layers
        .iter()
        .filter(|l| !l.is_triggered())
        .cloned()
        .collect();
    if looped.is_empty() {
        return vec![blank_frame()];
    }
    for l in &mut looped {
        l.configure_sync(fps);
    }
    let mut total = looped
        .iter()
        .map(|l| l.fpc * l.cycle_count)
        .max()
        .unwrap_or(1)
        .min(max_frames);

    let fits = looped.iter().all(|l| {
        let ratio = (total as f64) / ((l.fpc * l.cycle_count) as f64);
        !(1.2 < ratio && ratio < 2.0)
    });
    if !fits {
        let mut best = 999usize;
        for l in &looped {
            let n = max_frames / (l.fpc * l.cycle_count);
            if n < best {
                best = n;
                total = if n > 0 {
                    n * l.fpc * l.cycle_count
                } else {
                    max_frames
                };
            }
        }
    }
    for l in &mut looped {
        let n = total / (l.fpc * l.cycle_count);
        if let Some(per_repeat) = total.checked_div(n) {
            l.fpc = (per_repeat / l.cycle_count).max(1);
            l.ref_fpc = (total as f64) / ((n * l.cycle_count) as f64);
        } else {
            l.fpc = (total / l.cycle_count).max(1);
            l.ref_fpc = (total as f64) / (l.cycle_count as f64);
        }
    }
    let opacities: Vec<i32> = looped.iter().map(|l| l.opacity).collect();
    let mut frames = Vec::with_capacity(total);
    for _ in 0..total {
        let mut results: Vec<Vec<(usize, Rgba)>> =
            looped.iter_mut().rev().map(|l| l.run()).collect();
        results.reverse();
        frames.push(composite(&results, &opacities));
    }
    frames
}

use std::collections::BTreeSet;

pub use origins65::device::FullDeviceInfo as DeviceInfo;
use origins65::model::Model;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RgbColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl RgbColor {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b }
    }

    pub fn to_hex(self) -> String {
        format!("{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    pub fn to_hex_upper(self) -> String {
        format!("#{:02X}{:02X}{:02X}", self.r, self.g, self.b)
    }

    pub fn from_hex(s: &str) -> Option<Self> {
        let s = s.trim().trim_start_matches('#');
        if s.len() != 6 {
            return None;
        }
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(Self { r, g, b })
    }

    pub fn to_u32(self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    pub fn to_rgba_u32(self, alpha: u8) -> u32 {
        ((self.r as u32) << 24)
            | ((self.g as u32) << 16)
            | ((self.b as u32) << 8)
            | (alpha as u32)
    }

    pub fn is_light(self) -> bool {
        let luma = 0.299 * (self.r as f32) + 0.587 * (self.g as f32) + 0.114 * (self.b as f32);
        luma > 155.0
    }
}

pub const SWATCH_COLORS: &[(&str, RgbColor)] = &[
    ("HyperX Red", RgbColor::new(242, 0, 0)),
    ("Crimson", RgbColor::new(255, 25, 80)),
    ("Orange", RgbColor::new(255, 80, 0)),
    ("Amber", RgbColor::new(255, 165, 0)),
    ("Yellow", RgbColor::new(255, 220, 0)),
    ("Lime", RgbColor::new(100, 255, 0)),
    ("Green", RgbColor::new(0, 255, 64)),
    ("Teal", RgbColor::new(0, 220, 180)),
    ("Cyan", RgbColor::new(0, 255, 255)),
    ("Sky", RgbColor::new(0, 140, 255)),
    ("Blue", RgbColor::new(0, 48, 255)),
    ("Indigo", RgbColor::new(90, 40, 255)),
    ("Purple", RgbColor::new(150, 0, 255)),
    ("Magenta", RgbColor::new(255, 0, 255)),
    ("Pink", RgbColor::new(255, 60, 140)),
    ("White", RgbColor::new(255, 255, 255)),
    ("Warm White", RgbColor::new(255, 224, 180)),
    ("Off", RgbColor::new(0, 0, 0)),
];

pub const RGB_RAINBOW: &[RgbColor] = &[
    RgbColor::new(255, 0, 0),
    RgbColor::new(255, 151, 0),
    RgbColor::new(206, 255, 0),
    RgbColor::new(50, 255, 0),
    RgbColor::new(0, 255, 103),
    RgbColor::new(0, 255, 255),
    RgbColor::new(0, 104, 255),
    RgbColor::new(43, 0, 255),
    RgbColor::new(205, 0, 255),
    RgbColor::new(255, 0, 158),
];

pub const CYCLE_COLORS: &[RgbColor] = &[
    RgbColor::new(255, 0, 0),
    RgbColor::new(255, 255, 0),
    RgbColor::new(0, 255, 0),
    RgbColor::new(0, 255, 255),
    RgbColor::new(0, 0, 255),
];

pub const SWIPE_DEFAULT_COLORS: &[RgbColor] = &[
    RgbColor::new(255, 0, 0),
    RgbColor::new(0, 255, 0),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorMode {
    /// Fixed/built-in colors (confetti, sun)
    None,
    /// Single color only (solid)
    Single,
    /// Single color or random (breathing, twilight, fade, explosion, flame, implosion)
    SingleOrRandom,
    /// Ordered gradient / color list (cycle, wave)
    List,
    /// Two colors or random (swipe)
    PairOrRandom,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwipeDirection {
    Left,
    Right,
    Up,
    Down,
}

impl SwipeDirection {
    pub const ALL: [SwipeDirection; 4] = [
        SwipeDirection::Left,
        SwipeDirection::Right,
        SwipeDirection::Up,
        SwipeDirection::Down,
    ];

    pub fn cli_name(self) -> &'static str {
        match self {
            Self::Left => "left",
            Self::Right => "right",
            Self::Up => "up",
            Self::Down => "down",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Up => "Up",
            Self::Down => "Down",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EffectKind {
    // Looped
    Solid,
    Breathing,
    Cycle,
    Wave,
    Swipe,
    Confetti,
    Sun,
    Twilight,
    // Key-press
    Fade,
    Explosion,
    Flame,
    Implosion,
}

impl EffectKind {
    pub const LOOPED: [EffectKind; 8] = [
        Self::Solid,
        Self::Breathing,
        Self::Cycle,
        Self::Wave,
        Self::Swipe,
        Self::Confetti,
        Self::Sun,
        Self::Twilight,
    ];

    pub const TRIGGERED: [EffectKind; 4] = [
        Self::Fade,
        Self::Explosion,
        Self::Flame,
        Self::Implosion,
    ];

    pub fn cli_name(self) -> &'static str {
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

    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Breathing => "Breathing",
            Self::Cycle => "Cycle",
            Self::Wave => "Wave",
            Self::Swipe => "Swipe",
            Self::Confetti => "Confetti",
            Self::Sun => "Sun",
            Self::Twilight => "Twilight",
            Self::Fade => "Fade",
            Self::Explosion => "Explosion",
            Self::Flame => "Flame",
            Self::Implosion => "Implosion",
        }
    }

    pub fn is_triggered(self) -> bool {
        matches!(self, Self::Fade | Self::Explosion | Self::Flame | Self::Implosion)
    }

    pub fn onboard_ok(self) -> bool {
        !matches!(self, Self::Implosion)
    }

    pub fn uses_speed(self) -> bool {
        !matches!(self, Self::Solid)
    }

    fn driver_kind(self) -> origins65::effects::EffectKind {
        origins65::effects::EffectKind::from_name(self.cli_name()).expect("same effect names")
    }

    /// NGENUITY's speed range on `model`: `(min, max, default)` ms.
    pub fn speed_range(self, model: Model) -> (u32, u32, u32) {
        model.speed_range(self.driver_kind())
    }

    pub fn default_speed(self, model: Model) -> u32 {
        self.speed_range(model).2
    }

    pub fn color_mode(self) -> ColorMode {
        match self {
            Self::Solid => ColorMode::Single,
            Self::Breathing | Self::Twilight | Self::Fade | Self::Explosion | Self::Flame | Self::Implosion => {
                ColorMode::SingleOrRandom
            }
            Self::Cycle | Self::Wave => ColorMode::List,
            Self::Swipe => ColorMode::PairOrRandom,
            Self::Confetti | Self::Sun => ColorMode::None,
        }
    }

    pub fn default_colors(self) -> Vec<RgbColor> {
        match self {
            Self::Wave => RGB_RAINBOW.to_vec(),
            Self::Cycle => CYCLE_COLORS.to_vec(),
            Self::Swipe => SWIPE_DEFAULT_COLORS.to_vec(),
            _ => vec![RgbColor::new(242, 0, 0)],
        }
    }

    pub fn has_angle(self) -> bool {
        matches!(self, Self::Wave)
    }

    pub fn has_direction(self) -> bool {
        matches!(self, Self::Swipe)
    }
}

#[derive(Debug, Clone)]
pub struct LayerConfig {
    pub id: u64,
    pub effect: EffectKind,
    pub speed: u32,
    pub color: RgbColor,
    pub colors: Vec<RgbColor>,
    pub random_color: bool,
    pub opacity: u8,
    pub angle: u16,
    pub direction: SwipeDirection,
    /// Empty means all keys.
    pub keys: BTreeSet<String>,
    /// Keyboard the layer is edited for (speed ranges and key names differ).
    pub model: Model,
}

impl LayerConfig {
    pub fn new(id: u64, effect: EffectKind) -> Self {
        Self::for_model(id, effect, Model::Origins65)
    }

    pub fn for_model(id: u64, effect: EffectKind, model: Model) -> Self {
        Self {
            model,
            id,
            effect,
            speed: effect.default_speed(model),
            color: RgbColor::new(242, 0, 0),
            colors: effect.default_colors(),
            random_color: false,
            opacity: 100,
            angle: 180,
            direction: SwipeDirection::Right,
            keys: BTreeSet::new(),
        }
    }

    pub fn set_effect(&mut self, effect: EffectKind) {
        if self.effect == effect {
            return;
        }
        let prev_mode = self.effect.color_mode();
        self.effect = effect;
        self.speed = effect.default_speed(self.model);
        match effect.color_mode() {
            ColorMode::List => {
                if prev_mode != ColorMode::List || self.colors.is_empty() {
                    self.colors = effect.default_colors();
                }
                self.random_color = false;
            }
            ColorMode::PairOrRandom => {
                if self.colors.len() < 2 {
                    self.colors = effect.default_colors();
                } else {
                    self.colors.truncate(2);
                }
            }
            ColorMode::Single => {
                self.random_color = false;
            }
            ColorMode::SingleOrRandom | ColorMode::None => {}
        }
    }

    /// Move the layer to another keyboard: keep the speed if it is still in range and drop key
    /// names that keyboard does not have.
    pub fn set_model(&mut self, model: Model) {
        if self.model == model {
            return;
        }
        let was_default = self.speed == self.effect.default_speed(self.model);
        self.model = model;
        let (min, max, default) = self.effect.speed_range(model);
        self.speed = if was_default { default } else { self.speed.clamp(min, max) };
        self.keys.retain(|name| model.key_by_name(name).is_some());
    }

    pub fn preview_color(&self) -> RgbColor {
        match self.effect.color_mode() {
            ColorMode::None => match self.effect {
                EffectKind::Sun => RgbColor::new(255, 130, 0),
                EffectKind::Confetti => RgbColor::new(168, 85, 247),
                _ => self.color,
            },
            ColorMode::Single => self.color,
            ColorMode::SingleOrRandom => {
                if self.random_color {
                    RgbColor::new(192, 132, 252)
                } else {
                    self.color
                }
            }
            ColorMode::List | ColorMode::PairOrRandom => {
                if self.random_color {
                    RgbColor::new(192, 132, 252)
                } else {
                    self.colors.first().copied().unwrap_or(self.color)
                }
            }
        }
    }

    pub fn to_cli_spec(&self) -> String {
        let mut opts: Vec<String> = Vec::new();

        if self.effect.uses_speed() {
            let (min, max, default) = self.effect.speed_range(self.model);
            let clamped = self.speed.clamp(min, max);
            if clamped != default {
                opts.push(format!("speed={clamped}"));
            }
        }

        match self.effect.color_mode() {
            ColorMode::None => {}
            ColorMode::Single => {
                opts.push(format!("color={}", self.color.to_hex()));
            }
            ColorMode::SingleOrRandom => {
                if self.random_color {
                    opts.push("color=random".to_string());
                } else {
                    opts.push(format!("color={}", self.color.to_hex()));
                }
            }
            ColorMode::List => {
                if !self.colors.is_empty() {
                    let list = self
                        .colors
                        .iter()
                        .map(|c| c.to_hex())
                        .collect::<Vec<_>>()
                        .join("/");
                    opts.push(format!("colors={list}"));
                }
            }
            ColorMode::PairOrRandom => {
                if self.random_color {
                    opts.push("color=random".to_string());
                } else if !self.colors.is_empty() {
                    let list = self
                        .colors
                        .iter()
                        .take(2)
                        .map(|c| c.to_hex())
                        .collect::<Vec<_>>()
                        .join("/");
                    opts.push(format!("colors={list}"));
                }
            }
        }

        if self.effect.has_angle() && self.angle != 180 {
            opts.push(format!("angle={}", self.angle));
        }
        if self.effect.has_direction() && self.direction != SwipeDirection::Right {
            opts.push(format!("direction={}", self.direction.cli_name()));
        }
        if self.opacity < 100 {
            opts.push(format!("opacity={}", self.opacity));
        }
        if !self.keys.is_empty() {
            let keys = self.keys.iter().cloned().collect::<Vec<_>>().join("+");
            opts.push(format!("keys={keys}"));
        }

        if opts.is_empty() {
            self.effect.cli_name().to_string()
        } else {
            format!("{}:{}", self.effect.cli_name(), opts.join(","))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OnboardValidation {
    pub errors: Vec<String>,
    pub notes: Vec<String>,
}

impl OnboardValidation {
    pub fn is_valid(&self) -> bool {
        self.errors.is_empty()
    }
}

pub fn validate_onboard(layers: &[LayerConfig]) -> OnboardValidation {
    let mut errors = Vec::new();
    let mut notes = Vec::new();

    if layers.is_empty() {
        errors.push("Add at least one layer before saving.".to_string());
        return OnboardValidation { errors, notes };
    }

    let triggers: Vec<&LayerConfig> = layers.iter().filter(|l| l.effect.is_triggered()).collect();
    if triggers.len() > 1 {
        errors.push(format!(
            "Only one key-press effect can be saved onboard (currently {}).",
            triggers.len()
        ));
    }

    for trig in &triggers {
        if !trig.effect.onboard_ok() {
            errors.push(format!(
                "{} is live-only; onboard firmware supports fade, explosion, or flame.",
                trig.effect.label()
            ));
        }
        if !trig.keys.is_empty() {
            notes.push(format!(
                "{}: onboard firmware ignores per-key masks on key-press effects.",
                trig.effect.label()
            ));
        }
    }

    let looped_count = layers.len() - triggers.len();
    if looped_count == 0 {
        notes.push("No looped base layer: unpressed keys will be off onboard.".to_string());
    }

    OnboardValidation { errors, notes }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_roundtrip() {
        let c = RgbColor::from_hex("#1a2b3c").unwrap();
        assert_eq!(c, RgbColor::new(0x1a, 0x2b, 0x3c));
        assert_eq!(c.to_hex(), "1a2b3c");
        assert!(RgbColor::from_hex("12345").is_none());
        assert!(RgbColor::from_hex("zzzzzz").is_none());
    }

    #[test]
    fn layer_spec_formatting() {
        let mut solid = LayerConfig::new(1, EffectKind::Solid);
        solid.color = RgbColor::new(255, 0, 0);
        solid.keys.insert("w".into());
        solid.keys.insert("a".into());
        solid.opacity = 75;
        assert_eq!(solid.to_cli_spec(), "solid:color=ff0000,opacity=75,keys=a+w");

        let mut wave = LayerConfig::new(2, EffectKind::Wave);
        wave.speed = 4000;
        wave.angle = 90;
        wave.colors = vec![RgbColor::new(255, 0, 0), RgbColor::new(0, 0, 255)];
        assert_eq!(wave.to_cli_spec(), "wave:speed=4000,colors=ff0000/0000ff,angle=90");

        let mut exp = LayerConfig::new(3, EffectKind::Explosion);
        exp.random_color = true;
        assert_eq!(exp.to_cli_spec(), "explosion:color=random");
    }

    #[test]
    fn layers_follow_the_keyboard_model() {
        let mut wave = LayerConfig::new(1, EffectKind::Wave);
        wave.speed = 8000; // clamped to the Origins 65 maximum when formatted
        assert!(wave.to_cli_spec().starts_with("wave:speed=6000"));
        wave.set_model(Model::OriginsCore);
        assert_eq!(wave.speed, 8000);
        assert!(wave.to_cli_spec().starts_with("wave:speed=8000"));

        let mut flame = LayerConfig::new(2, EffectKind::Flame);
        flame.keys.insert("w".into());
        flame.keys.insert("lspace".into()); // Origins 65 only
        flame.set_model(Model::OriginsCore);
        assert_eq!(flame.speed, 1000); // default follows the model
        assert_eq!(flame.keys.iter().collect::<Vec<_>>(), ["w"]);
    }

    #[test]
    fn onboard_validation_rules() {
        let wave = LayerConfig::new(1, EffectKind::Wave);
        let fade = LayerConfig::new(2, EffectKind::Fade);
        let flame = LayerConfig::new(3, EffectKind::Flame);
        let implosion = LayerConfig::new(4, EffectKind::Implosion);

        assert!(validate_onboard(&[wave.clone(), fade.clone()]).is_valid());
        assert!(!validate_onboard(&[]).is_valid());
        assert!(!validate_onboard(&[wave.clone(), implosion]).is_valid());
        assert!(!validate_onboard(&[wave, fade, flame]).is_valid());
    }
}

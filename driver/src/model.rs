//! Supported keyboard models (`tools/o65/models.py`): key layout, onboard-storage limits and
//! NGENUITY's per-model effect speed ranges.
//!
//! Values come from NGENUITY 2.39 (`NGenuity2Helper.exe`): the device classes' `MaxSyncFrameCount`
//! and `FramePerSecond`, and the model-specific branches in the effects' `SetupDefaultValues`.

use serde::{Deserialize, Serialize};

use crate::effects::EffectKind;
use crate::layout::{CORE_KEYS, KEYS, Key};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub enum Model {
    /// HyperX Alloy Origins 65 (`03f0:038f`).
    #[default]
    #[serde(rename = "origins65")]
    Origins65,
    /// HyperX Alloy Origins Core, the tenkeyless (`03f0:098f`, `0951:16e6`).
    #[serde(rename = "origins-core")]
    OriginsCore,
}

impl Model {
    pub const ALL: [Model; 2] = [Self::Origins65, Self::OriginsCore];

    /// Stable identifier used by `info --json` and `--model`.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Origins65 => "origins65",
            Self::OriginsCore => "origins-core",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    pub const fn name(self) -> &'static str {
        match self {
            Self::Origins65 => "HyperX Alloy Origins 65",
            Self::OriginsCore => "HyperX Alloy Origins Core",
        }
    }

    /// `(vid, pid)` pairs: HP-era and Kingston-era HyperX.
    pub const fn usb_ids(self) -> &'static [(u16, u16)] {
        match self {
            Self::Origins65 => &[(0x03F0, 0x038F)],
            Self::OriginsCore => &[(0x03F0, 0x098F), (0x0951, 0x16E6)],
        }
    }

    pub const fn keys(self) -> &'static [Key] {
        match self {
            Self::Origins65 => KEYS,
            Self::OriginsCore => CORE_KEYS,
        }
    }

    pub fn max_col(self) -> u8 {
        self.keys().iter().map(|k| k.col).max().unwrap_or(0)
    }

    pub fn max_row(self) -> u8 {
        self.keys().iter().map(|k| k.row).max().unwrap_or(0)
    }

    pub fn key_by_name(self, name: &str) -> Option<&'static Key> {
        self.keys().iter().find(|k| k.name == name)
    }

    pub fn key_by_usage(self, usage: u8) -> Option<&'static Key> {
        self.keys().iter().find(|k| k.usage == Some(usage))
    }

    /// `FramePerSecond`: playback rate of onboard animations.
    pub const fn sync_fps(self) -> u32 {
        match self {
            Self::Origins65 => 20,
            Self::OriginsCore => 25,
        }
    }

    /// Frame interval byte NGENUITY stores with an onboard animation.
    pub const fn sync_interval(self) -> u8 {
        (1000 / self.sync_fps() - 16) as u8
    }

    /// `MaxSyncFrameCount`.
    pub const fn max_sync_frames(self) -> usize {
        match self {
            Self::Origins65 => 120,
            Self::OriginsCore => 100,
        }
    }

    /// The onboard key-press speed byte is `(speed - min) / (max - min) * trigger_speed_max`.
    pub const fn trigger_speed_max(self) -> f64 {
        match self {
            Self::Origins65 => 255.0,
            Self::OriginsCore => 63.0,
        }
    }

    /// `(min, max, default)` speed in ms of an effect on this keyboard. [`EffectKind`]'s own
    /// ranges are the Origins 65 ones.
    pub const fn speed_range(self, kind: EffectKind) -> (u32, u32, u32) {
        match (self, kind) {
            // Only the Origins 60/65 lower the wave maximum to 6000.
            (Self::OriginsCore, EffectKind::Wave) => (1000, 9000, 3000),
            (Self::OriginsCore, EffectKind::Explosion) => (500, 3000, 1000),
            (Self::OriginsCore, EffectKind::Flame) => (800, 1500, 1000),
            _ => (kind.min_speed(), kind.max_speed(), kind.default_speed()),
        }
    }

    pub const fn min_speed(self, kind: EffectKind) -> u32 {
        self.speed_range(kind).0
    }

    pub const fn max_speed(self, kind: EffectKind) -> u32 {
        self.speed_range(kind).1
    }

    pub const fn default_speed(self, kind: EffectKind) -> u32 {
        self.speed_range(kind).2
    }
}

impl std::fmt::Display for Model {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn model_constants() {
        assert_eq!(Model::Origins65.sync_interval(), 34);
        assert_eq!(Model::OriginsCore.sync_interval(), 24);
        assert_eq!(Model::Origins65.max_col(), 14);
        assert_eq!(Model::OriginsCore.max_col(), 16);
        assert_eq!(Model::OriginsCore.max_row(), 5);
        assert_eq!(
            Model::OriginsCore.key_by_usage(0xE8).map(|k| k.name),
            Some("fn")
        );
        assert_eq!(Model::from_id("origins-core"), Some(Model::OriginsCore));
        assert_eq!(
            Model::Origins65.speed_range(EffectKind::Wave),
            (1000, 6000, 3000)
        );
    }
}

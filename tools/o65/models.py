"""Supported keyboard models: key layout, onboard-storage limits and NGENUITY's per-model effect speeds.

Values come from NGENUITY 2.39 (NGenuity2Helper.exe): the device classes' MaxSyncFrameCount and
FramePerSecond, and the model-specific branches in the effects' SetupDefaultValues.
"""
from dataclasses import dataclass
from functools import cached_property

from . import layout


@dataclass(frozen=True, eq=False)
class Model:
    id: str                    # stable identifier used in `info --json`
    name: str
    usb_ids: tuple             # (vid, pid) pairs: HP and Kingston-era HyperX
    keys: tuple
    sync_fps: int              # FramePerSecond: playback rate of onboard animations
    max_sync_frames: int       # MaxSyncFrameCount
    trigger_speed_max: int     # onboard key-press speed byte is (speed - min) / (max - min) * this
    speed_overrides: dict      # effect name -> (min, max, default) ms

    @cached_property
    def by_name(self):
        return {k.name: k for k in self.keys}

    @cached_property
    def by_usage(self):
        return {k.usage: k for k in self.keys if k.usage is not None}

    @cached_property
    def max_col(self):
        return max(k.col for k in self.keys)

    @cached_property
    def max_row(self):
        return max(k.row for k in self.keys)

    @property
    def sync_interval(self):
        return 1000 // self.sync_fps - 16  # value NGENUITY stores as the frame interval

    def speed_range(self, effect_cls):
        """(min, max, default) speed in ms of an effect class on this keyboard."""
        return self.speed_overrides.get(
            effect_cls.name, (effect_cls.min_speed, effect_cls.max_speed, effect_cls.default_speed))


# The effect classes' own ranges are the Origins 65 ones.
ORIGINS65 = Model(
    id="origins65",
    name="HyperX Alloy Origins 65",
    usb_ids=((0x03F0, 0x038F),),
    keys=tuple(layout.KEYS),
    sync_fps=20,
    max_sync_frames=120,
    trigger_speed_max=255,
    speed_overrides={},
)

ORIGINS_CORE = Model(
    id="origins-core",
    name="HyperX Alloy Origins Core",
    usb_ids=((0x03F0, 0x098F), (0x0951, 0x16E6)),
    keys=tuple(layout.CORE_KEYS),
    sync_fps=25,
    max_sync_frames=100,
    trigger_speed_max=63,
    speed_overrides={
        "wave": (1000, 9000, 3000),        # only the 60/65 lower the maximum to 6000
        "explosion": (500, 3000, 1000),
        "flame": (800, 1500, 1000),
    },
)

MODELS = (ORIGINS65, ORIGINS_CORE)
BY_ID = {m.id: m for m in MODELS}

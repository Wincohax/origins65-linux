"""Key layouts of the supported keyboards.

* Alloy Origins 65, US 67-key (NGENUITY KeyboardAlloyOrigins65.Setup67Keyboard). LED slot on the wire =
  LEDMatrix.Y * 6 + LEDMatrix.X. Keys without a usage (the two outer space-bar LEDs) are lit but never
  reported as pressed.
* Alloy Origins Core (TKL), US 87-key (KeyboardAlloyOriginsCore.Setup87Keyboard). LEDMatrix.X is 0..5 or
  8..13 (two six-LED "sectors" per 48-byte segment), so the LED index is
  (LEDMatrix.Y * 2 + X // 8) * 6 + X % 8 -- the same numbering OpenRGB uses for this keyboard.

Each key: name, HID usage reported in key notifications, grid row/column used by the effect engine,
and LED index.
"""
from dataclasses import dataclass


@dataclass(frozen=True)
class Key:
    name: str
    usage: int | None
    row: int
    col: int
    led: int


def _k(name, usage, row, col, ledx, ledy):
    return Key(name, usage, row, col, ledy * 6 + ledx)


KEYS = [
    _k("esc", 41, 0, 0, 1, 0), _k("1", 30, 0, 1, 2, 0), _k("2", 31, 0, 2, 3, 0), _k("3", 32, 0, 3, 4, 0),
    _k("4", 33, 0, 4, 5, 0), _k("5", 34, 0, 5, 0, 1), _k("6", 35, 0, 6, 1, 1), _k("7", 36, 0, 7, 2, 1),
    _k("8", 37, 0, 8, 3, 1), _k("9", 38, 0, 9, 4, 1), _k("0", 39, 0, 10, 5, 1), _k("minus", 45, 0, 11, 0, 2),
    _k("equals", 46, 0, 12, 1, 2), _k("backspace", 42, 0, 13, 3, 2), _k("home", 74, 0, 14, 3, 11),
    _k("tab", 43, 1, 0, 4, 2), _k("q", 20, 1, 1, 5, 2), _k("w", 26, 1, 2, 0, 3), _k("e", 8, 1, 3, 1, 3),
    _k("r", 21, 1, 4, 2, 3), _k("t", 23, 1, 5, 3, 3), _k("y", 28, 1, 6, 4, 3), _k("u", 24, 1, 7, 5, 3),
    _k("i", 12, 1, 8, 0, 4), _k("o", 18, 1, 9, 1, 4), _k("p", 19, 1, 10, 2, 4), _k("lbracket", 47, 1, 11, 3, 4),
    _k("rbracket", 48, 1, 12, 4, 4), _k("backslash", 49, 1, 13, 5, 4), _k("delete", 76, 1, 14, 4, 11),
    _k("caps", 57, 2, 0, 0, 5), _k("a", 4, 2, 1, 1, 5), _k("s", 22, 2, 2, 2, 5), _k("d", 7, 2, 3, 3, 5),
    _k("f", 9, 2, 4, 4, 5), _k("g", 10, 2, 5, 5, 5), _k("h", 11, 2, 6, 0, 6), _k("j", 13, 2, 7, 1, 6),
    _k("k", 14, 2, 8, 2, 6), _k("l", 15, 2, 9, 3, 6), _k("semicolon", 51, 2, 10, 4, 6), _k("quote", 52, 2, 11, 5, 6),
    _k("enter", 40, 2, 12, 1, 7), _k("pgup", 75, 2, 14, 5, 11),
    _k("lshift", 225, 3, 0, 2, 7), _k("z", 29, 3, 2, 4, 7), _k("x", 27, 3, 3, 5, 7), _k("c", 6, 3, 4, 0, 8),
    _k("v", 25, 3, 5, 1, 8), _k("b", 5, 3, 6, 2, 8), _k("n", 17, 3, 7, 3, 8), _k("m", 16, 3, 8, 4, 8),
    _k("comma", 54, 3, 9, 5, 8), _k("period", 55, 3, 10, 0, 9), _k("slash", 56, 3, 11, 1, 9),
    _k("rshift", 229, 3, 12, 3, 9), _k("up", 82, 3, 13, 1, 12), _k("pgdn", 78, 3, 14, 0, 12),
    _k("lctrl", 224, 4, 0, 4, 9), _k("lwin", 227, 4, 1, 5, 9), _k("lalt", 226, 4, 2, 0, 10),
    _k("lspace", None, 4, 5, 1, 10), _k("space", 44, 4, 6, 2, 10), _k("rspace", None, 4, 7, 3, 10),
    _k("ralt", 230, 4, 10, 4, 10), _k("fn", 164, 4, 11, 2, 11),
    _k("left", 80, 4, 12, 2, 12), _k("down", 81, 4, 13, 3, 12), _k("right", 79, 4, 14, 4, 12),
]

# Frame size shared by every model: the Origins 65 sends the first 80 slots (5 packets x 16), the
# Origins Core 94 of its 96 (380-byte G/R/B segments).
NUM_LEDS = 96
ORIGINS65_WIRE_LEDS = 80
BY_NAME = {k.name: k for k in KEYS}
BY_USAGE = {k.usage: k for k in KEYS if k.usage is not None}
MAX_COL = max(k.col for k in KEYS)
MAX_ROW = max(k.row for k in KEYS)


def _c(name, usage, row, col, ledx, ledy):
    return Key(name, usage, row, col, (ledy * 2 + ledx // 8) * 6 + ledx % 8)


# The firmware reports Fn as usage 0xE8 (232), see KeyboardAlloyOriginsCore.Device_InputReportReceived.
CORE_KEYS = [
    _c("esc", 41, 0, 0, 0, 0), _c("f1", 58, 0, 2, 1, 0), _c("f2", 59, 0, 3, 2, 0), _c("f3", 60, 0, 4, 3, 0),
    _c("f4", 61, 0, 5, 4, 0), _c("f5", 62, 0, 6, 5, 0), _c("f6", 63, 0, 7, 8, 0), _c("f7", 64, 0, 8, 9, 0),
    _c("f8", 65, 0, 9, 0, 4), _c("f9", 66, 0, 10, 1, 4), _c("f10", 67, 0, 11, 2, 4),
    _c("f11", 68, 0, 12, 3, 4), _c("f12", 69, 0, 13, 4, 4), _c("print", 70, 0, 14, 5, 4),
    _c("scroll", 71, 0, 15, 8, 4), _c("pause", 72, 0, 16, 9, 4),
    _c("grave", 53, 1, 0, 10, 0), _c("1", 30, 1, 1, 11, 0), _c("2", 31, 1, 2, 12, 0), _c("3", 32, 1, 3, 13, 0),
    _c("4", 33, 1, 4, 0, 1), _c("5", 34, 1, 5, 1, 1), _c("6", 35, 1, 6, 2, 1), _c("7", 36, 1, 7, 3, 1),
    _c("8", 37, 1, 8, 4, 1), _c("9", 38, 1, 9, 10, 4), _c("0", 39, 1, 10, 11, 4),
    _c("minus", 45, 1, 11, 12, 4), _c("equals", 46, 1, 12, 13, 4), _c("backspace", 42, 1, 13, 0, 5),
    _c("insert", 73, 1, 14, 1, 5), _c("home", 74, 1, 15, 2, 5), _c("pgup", 75, 1, 16, 3, 5),
    _c("tab", 43, 2, 0, 5, 1), _c("q", 20, 2, 1, 8, 1), _c("w", 26, 2, 2, 9, 1), _c("e", 8, 2, 3, 10, 1),
    _c("r", 21, 2, 4, 11, 1), _c("t", 23, 2, 5, 12, 1), _c("y", 28, 2, 6, 13, 1), _c("u", 24, 2, 7, 0, 2),
    _c("i", 12, 2, 8, 4, 5), _c("o", 18, 2, 9, 5, 5), _c("p", 19, 2, 10, 8, 5),
    _c("lbracket", 47, 2, 11, 9, 5), _c("rbracket", 48, 2, 12, 10, 5), _c("backslash", 49, 2, 13, 11, 5),
    _c("delete", 76, 2, 14, 12, 5), _c("end", 77, 2, 15, 13, 5), _c("pgdn", 78, 2, 16, 0, 6),
    _c("caps", 57, 3, 0, 1, 2), _c("a", 4, 3, 1, 2, 2), _c("s", 22, 3, 2, 3, 2), _c("d", 7, 3, 3, 4, 2),
    _c("f", 9, 3, 4, 5, 2), _c("g", 10, 3, 5, 8, 2), _c("h", 11, 3, 6, 9, 2), _c("j", 13, 3, 7, 10, 2),
    _c("k", 14, 3, 8, 1, 6), _c("l", 15, 3, 9, 2, 6), _c("semicolon", 51, 3, 10, 3, 6),
    _c("quote", 52, 3, 11, 4, 6), _c("enter", 40, 3, 12, 8, 6),
    _c("lshift", 225, 4, 0, 11, 2), _c("z", 29, 4, 2, 13, 2), _c("x", 27, 4, 3, 0, 3), _c("c", 6, 4, 4, 1, 3),
    _c("v", 25, 4, 5, 2, 3), _c("b", 5, 4, 6, 3, 3), _c("n", 17, 4, 7, 4, 3), _c("m", 16, 4, 8, 9, 6),
    _c("comma", 54, 4, 9, 10, 6), _c("period", 55, 4, 10, 11, 6), _c("slash", 56, 4, 11, 12, 6),
    _c("rshift", 229, 4, 13, 0, 7), _c("up", 82, 4, 15, 1, 7),
    _c("lctrl", 224, 5, 0, 5, 3), _c("lwin", 227, 5, 1, 8, 3), _c("lalt", 226, 5, 2, 9, 3),
    _c("space", 44, 5, 6, 11, 3), _c("ralt", 230, 5, 10, 2, 7), _c("fn", 232, 5, 11, 3, 7),
    _c("menu", 101, 5, 12, 4, 7), _c("rctrl", 228, 5, 13, 5, 7), _c("left", 80, 5, 14, 8, 7),
    _c("down", 81, 5, 15, 9, 7), _c("right", 79, 5, 16, 10, 7),
]

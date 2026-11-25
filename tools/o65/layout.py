"""US 67-key layout of the Alloy Origins 65 (NGENUITY KeyboardAlloyOrigins65.Setup67Keyboard).

Each key: name, HID usage reported in notification 05 04 <usage> <state>, grid row/column used by
the effect engine, and LED slot (= LEDMatrix.Y * 6 + LEDMatrix.X) used on the wire.
Keys without a usage (the two outer space-bar LEDs) are lit but never reported as pressed.
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

NUM_LEDS = 80  # slots on the wire (5 packets x 16)
BY_NAME = {k.name: k for k in KEYS}
BY_USAGE = {k.usage: k for k in KEYS if k.usage is not None}
MAX_COL = max(k.col for k in KEYS)
MAX_ROW = max(k.row for k in KEYS)

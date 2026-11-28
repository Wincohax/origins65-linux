//! Key layouts of the supported keyboards (`tools/o65/layout.py`).
//!
//! - [`KEYS`]: Alloy Origins 65, US 67-key (`NGENUITY KeyboardAlloyOrigins65.Setup67Keyboard`). Wire
//!   LED index = `LEDMatrix.Y * 6 + LEDMatrix.X`. Keys without a usage (`lspace`, `rspace`) are lit on
//!   the wire but never reported as pressed.
//! - [`CORE_KEYS`]: Alloy Origins Core (TKL), US 87-key (`KeyboardAlloyOriginsCore.Setup87Keyboard`).
//!   `LEDMatrix.X` is `0..=5` or `8..=13` (two six-LED sectors per 48-byte segment), so the LED index is
//!   `(LEDMatrix.Y * 2 + X / 8) * 6 + X % 8`, the numbering OpenRGB uses for this keyboard.
//!
//! Each entry provides the key name, HID usage reported in key notifications, grid row/column used
//! by the effect engine, and LED index.

/// Frame size shared by every model: the Origins 65 sends the first [`ORIGINS65_WIRE_LEDS`] slots,
/// the Origins Core 94 of the 96 (380-byte G/R/B segments).
pub const NUM_LEDS: usize = 96;

/// LED slots the Origins 65 transmits (5 packets x 16 LEDs).
pub const ORIGINS65_WIRE_LEDS: usize = 80;

/// Maximum grid column index across the Origins 65 keys.
pub const MAX_COL: u8 = 14;

/// Maximum grid row index across the Origins 65 keys.
pub const MAX_ROW: u8 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Key {
    pub name: &'static str,
    pub usage: Option<u8>,
    pub row: u8,
    pub col: u8,
    pub led: usize,
}

const fn k(
    name: &'static str,
    usage: Option<u8>,
    row: u8,
    col: u8,
    ledx: usize,
    ledy: usize,
) -> Key {
    Key {
        name,
        usage,
        row,
        col,
        led: ledy * 6 + ledx,
    }
}

/// All 69 LED entries (67 physical keys + 2 outer spacebar LEDs) in canonical order.
pub const KEYS: &[Key] = &[
    // Row 0
    k("esc", Some(41), 0, 0, 1, 0),
    k("1", Some(30), 0, 1, 2, 0),
    k("2", Some(31), 0, 2, 3, 0),
    k("3", Some(32), 0, 3, 4, 0),
    k("4", Some(33), 0, 4, 5, 0),
    k("5", Some(34), 0, 5, 0, 1),
    k("6", Some(35), 0, 6, 1, 1),
    k("7", Some(36), 0, 7, 2, 1),
    k("8", Some(37), 0, 8, 3, 1),
    k("9", Some(38), 0, 9, 4, 1),
    k("0", Some(39), 0, 10, 5, 1),
    k("minus", Some(45), 0, 11, 0, 2),
    k("equals", Some(46), 0, 12, 1, 2),
    k("backspace", Some(42), 0, 13, 3, 2),
    k("home", Some(74), 0, 14, 3, 11),
    // Row 1
    k("tab", Some(43), 1, 0, 4, 2),
    k("q", Some(20), 1, 1, 5, 2),
    k("w", Some(26), 1, 2, 0, 3),
    k("e", Some(8), 1, 3, 1, 3),
    k("r", Some(21), 1, 4, 2, 3),
    k("t", Some(23), 1, 5, 3, 3),
    k("y", Some(28), 1, 6, 4, 3),
    k("u", Some(24), 1, 7, 5, 3),
    k("i", Some(12), 1, 8, 0, 4),
    k("o", Some(18), 1, 9, 1, 4),
    k("p", Some(19), 1, 10, 2, 4),
    k("lbracket", Some(47), 1, 11, 3, 4),
    k("rbracket", Some(48), 1, 12, 4, 4),
    k("backslash", Some(49), 1, 13, 5, 4),
    k("delete", Some(76), 1, 14, 4, 11),
    // Row 2
    k("caps", Some(57), 2, 0, 0, 5),
    k("a", Some(4), 2, 1, 1, 5),
    k("s", Some(22), 2, 2, 2, 5),
    k("d", Some(7), 2, 3, 3, 5),
    k("f", Some(9), 2, 4, 4, 5),
    k("g", Some(10), 2, 5, 5, 5),
    k("h", Some(11), 2, 6, 0, 6),
    k("j", Some(13), 2, 7, 1, 6),
    k("k", Some(14), 2, 8, 2, 6),
    k("l", Some(15), 2, 9, 3, 6),
    k("semicolon", Some(51), 2, 10, 4, 6),
    k("quote", Some(52), 2, 11, 5, 6),
    k("enter", Some(40), 2, 12, 1, 7),
    k("pgup", Some(75), 2, 14, 5, 11),
    // Row 3
    k("lshift", Some(225), 3, 0, 2, 7),
    k("z", Some(29), 3, 2, 4, 7),
    k("x", Some(27), 3, 3, 5, 7),
    k("c", Some(6), 3, 4, 0, 8),
    k("v", Some(25), 3, 5, 1, 8),
    k("b", Some(5), 3, 6, 2, 8),
    k("n", Some(17), 3, 7, 3, 8),
    k("m", Some(16), 3, 8, 4, 8),
    k("comma", Some(54), 3, 9, 5, 8),
    k("period", Some(55), 3, 10, 0, 9),
    k("slash", Some(56), 3, 11, 1, 9),
    k("rshift", Some(229), 3, 12, 3, 9),
    k("up", Some(82), 3, 13, 1, 12),
    k("pgdn", Some(78), 3, 14, 0, 12),
    // Row 4
    k("lctrl", Some(224), 4, 0, 4, 9),
    k("lwin", Some(227), 4, 1, 5, 9),
    k("lalt", Some(226), 4, 2, 0, 10),
    k("lspace", None, 4, 5, 1, 10),
    k("space", Some(44), 4, 6, 2, 10),
    k("rspace", None, 4, 7, 3, 10),
    k("ralt", Some(230), 4, 10, 4, 10),
    k("fn", Some(164), 4, 11, 2, 11),
    k("left", Some(80), 4, 12, 2, 12),
    k("down", Some(81), 4, 13, 3, 12),
    k("right", Some(79), 4, 14, 4, 12),
];

const fn c(name: &'static str, usage: u8, row: u8, col: u8, ledx: usize, ledy: usize) -> Key {
    Key {
        name,
        usage: Some(usage),
        row,
        col,
        led: (ledy * 2 + ledx / 8) * 6 + ledx % 8,
    }
}

/// The 87 keys of the Alloy Origins Core (TKL). The firmware reports Fn as usage `0xE8`
/// (`KeyboardAlloyOriginsCore.Device_InputReportReceived`).
pub const CORE_KEYS: &[Key] = &[
    // Row 0
    c("esc", 41, 0, 0, 0, 0),
    c("f1", 58, 0, 2, 1, 0),
    c("f2", 59, 0, 3, 2, 0),
    c("f3", 60, 0, 4, 3, 0),
    c("f4", 61, 0, 5, 4, 0),
    c("f5", 62, 0, 6, 5, 0),
    c("f6", 63, 0, 7, 8, 0),
    c("f7", 64, 0, 8, 9, 0),
    c("f8", 65, 0, 9, 0, 4),
    c("f9", 66, 0, 10, 1, 4),
    c("f10", 67, 0, 11, 2, 4),
    c("f11", 68, 0, 12, 3, 4),
    c("f12", 69, 0, 13, 4, 4),
    c("print", 70, 0, 14, 5, 4),
    c("scroll", 71, 0, 15, 8, 4),
    c("pause", 72, 0, 16, 9, 4),
    // Row 1
    c("grave", 53, 1, 0, 10, 0),
    c("1", 30, 1, 1, 11, 0),
    c("2", 31, 1, 2, 12, 0),
    c("3", 32, 1, 3, 13, 0),
    c("4", 33, 1, 4, 0, 1),
    c("5", 34, 1, 5, 1, 1),
    c("6", 35, 1, 6, 2, 1),
    c("7", 36, 1, 7, 3, 1),
    c("8", 37, 1, 8, 4, 1),
    c("9", 38, 1, 9, 10, 4),
    c("0", 39, 1, 10, 11, 4),
    c("minus", 45, 1, 11, 12, 4),
    c("equals", 46, 1, 12, 13, 4),
    c("backspace", 42, 1, 13, 0, 5),
    c("insert", 73, 1, 14, 1, 5),
    c("home", 74, 1, 15, 2, 5),
    c("pgup", 75, 1, 16, 3, 5),
    // Row 2
    c("tab", 43, 2, 0, 5, 1),
    c("q", 20, 2, 1, 8, 1),
    c("w", 26, 2, 2, 9, 1),
    c("e", 8, 2, 3, 10, 1),
    c("r", 21, 2, 4, 11, 1),
    c("t", 23, 2, 5, 12, 1),
    c("y", 28, 2, 6, 13, 1),
    c("u", 24, 2, 7, 0, 2),
    c("i", 12, 2, 8, 4, 5),
    c("o", 18, 2, 9, 5, 5),
    c("p", 19, 2, 10, 8, 5),
    c("lbracket", 47, 2, 11, 9, 5),
    c("rbracket", 48, 2, 12, 10, 5),
    c("backslash", 49, 2, 13, 11, 5),
    c("delete", 76, 2, 14, 12, 5),
    c("end", 77, 2, 15, 13, 5),
    c("pgdn", 78, 2, 16, 0, 6),
    // Row 3
    c("caps", 57, 3, 0, 1, 2),
    c("a", 4, 3, 1, 2, 2),
    c("s", 22, 3, 2, 3, 2),
    c("d", 7, 3, 3, 4, 2),
    c("f", 9, 3, 4, 5, 2),
    c("g", 10, 3, 5, 8, 2),
    c("h", 11, 3, 6, 9, 2),
    c("j", 13, 3, 7, 10, 2),
    c("k", 14, 3, 8, 1, 6),
    c("l", 15, 3, 9, 2, 6),
    c("semicolon", 51, 3, 10, 3, 6),
    c("quote", 52, 3, 11, 4, 6),
    c("enter", 40, 3, 12, 8, 6),
    // Row 4
    c("lshift", 225, 4, 0, 11, 2),
    c("z", 29, 4, 2, 13, 2),
    c("x", 27, 4, 3, 0, 3),
    c("c", 6, 4, 4, 1, 3),
    c("v", 25, 4, 5, 2, 3),
    c("b", 5, 4, 6, 3, 3),
    c("n", 17, 4, 7, 4, 3),
    c("m", 16, 4, 8, 9, 6),
    c("comma", 54, 4, 9, 10, 6),
    c("period", 55, 4, 10, 11, 6),
    c("slash", 56, 4, 11, 12, 6),
    c("rshift", 229, 4, 13, 0, 7),
    c("up", 82, 4, 15, 1, 7),
    // Row 5
    c("lctrl", 224, 5, 0, 5, 3),
    c("lwin", 227, 5, 1, 8, 3),
    c("lalt", 226, 5, 2, 9, 3),
    c("space", 44, 5, 6, 11, 3),
    c("ralt", 230, 5, 10, 2, 7),
    c("fn", 232, 5, 11, 3, 7),
    c("menu", 101, 5, 12, 4, 7),
    c("rctrl", 228, 5, 13, 5, 7),
    c("left", 80, 5, 14, 8, 7),
    c("down", 81, 5, 15, 9, 7),
    c("right", 79, 5, 16, 10, 7),
];

/// Look up a key by its lowercase name (e.g. `"esc"`, `"w"`, `"lspace"`).
pub fn key_by_name(name: &str) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.name == name)
}

/// Look up a key by its HID usage byte reported in notification `05 04 <usage> <state>`.
pub fn key_by_usage(usage: u8) -> Option<&'static Key> {
    KEYS.iter().find(|k| k.usage == Some(usage))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn core_layout_invariants() {
        assert_eq!(CORE_KEYS.len(), 87);
        let names: HashSet<_> = CORE_KEYS.iter().map(|k| k.name).collect();
        assert_eq!(names.len(), 87);
        let leds: HashSet<_> = CORE_KEYS.iter().map(|k| k.led).collect();
        assert_eq!(leds.len(), 87);
        // OpenRGB drives 94 LEDs; index 94+ would not fit the 380-byte buffer.
        assert!(CORE_KEYS.iter().all(|k| k.led < 94));
        let esc = CORE_KEYS.iter().find(|k| k.name == "esc").unwrap();
        let f8 = CORE_KEYS.iter().find(|k| k.name == "f8").unwrap();
        let enter = CORE_KEYS.iter().find(|k| k.name == "enter").unwrap();
        assert_eq!((esc.led, f8.led, enter.led), (0, 48, 78));
    }

    #[test]
    fn layout_invariants() {
        assert_eq!(KEYS.len(), 69);
        assert_eq!(KEYS.iter().map(|k| k.col).max(), Some(MAX_COL));
        assert_eq!(KEYS.iter().map(|k| k.row).max(), Some(MAX_ROW));

        let names: HashSet<_> = KEYS.iter().map(|k| k.name).collect();
        assert_eq!(names.len(), 69);

        let leds: HashSet<_> = KEYS.iter().map(|k| k.led).collect();
        assert_eq!(leds.len(), 69);
        assert!(KEYS.iter().all(|k| k.led < NUM_LEDS));

        assert_eq!(key_by_usage(41).map(|k| k.name), Some("esc"));
        assert_eq!(key_by_usage(164).map(|k| k.name), Some("fn"));
        assert_eq!(key_by_name("lspace").and_then(|k| k.usage), None);
        assert_eq!(key_by_name("rspace").and_then(|k| k.usage), None);
    }
}

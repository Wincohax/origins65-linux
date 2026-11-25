//! US 67-key layout of the HyperX Alloy Origins 65 (`NGENUITY KeyboardAlloyOrigins65.Setup67Keyboard`).
//!
//! Each entry provides the key name, HID usage reported in notification `05 04 <usage> <state>`,
//! grid row/column used by the effect engine, and wire LED index (`LEDMatrix.Y * 6 + LEDMatrix.X`).
//! Keys without a usage (`lspace`, `rspace`) are lit on the wire but never reported as pressed.

/// Number of LED slots transmitted on the wire (5 packets x 16 LEDs).
pub const NUM_LEDS: usize = 80;

/// Maximum grid column index across all keys.
pub const MAX_COL: u8 = 14;

/// Maximum grid row index across all keys.
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

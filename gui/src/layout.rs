/// US 67-key (+ 2 outer spacebar LEDs = 69 entries) layout matching `tools/o65/layout.py`.
/// Each row's physical widths sum to 16.0 keycap units.
#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub struct KeyDef {
    pub name: &'static str,
    pub label: &'static str,
    pub row: u8,
    pub col: u8,
    /// Visual width in keycap units (1.0 = standard 1u key).
    pub width: f32,
    /// Optional blank spacer inserted BEFORE this key (in keycap units).
    pub gap_before: f32,
}

const fn k(name: &'static str, label: &'static str, row: u8, col: u8, width: f32) -> KeyDef {
    KeyDef {
        name,
        label,
        row,
        col,
        width,
        gap_before: 0.0,
    }
}

const fn kg(
    name: &'static str,
    label: &'static str,
    row: u8,
    col: u8,
    width: f32,
    gap_before: f32,
) -> KeyDef {
    KeyDef {
        name,
        label,
        row,
        col,
        width,
        gap_before,
    }
}

pub const ROW_0: &[KeyDef] = &[
    k("esc", "Esc", 0, 0, 1.0),
    k("1", "1", 0, 1, 1.0),
    k("2", "2", 0, 2, 1.0),
    k("3", "3", 0, 3, 1.0),
    k("4", "4", 0, 4, 1.0),
    k("5", "5", 0, 5, 1.0),
    k("6", "6", 0, 6, 1.0),
    k("7", "7", 0, 7, 1.0),
    k("8", "8", 0, 8, 1.0),
    k("9", "9", 0, 9, 1.0),
    k("0", "0", 0, 10, 1.0),
    k("minus", "-", 0, 11, 1.0),
    k("equals", "=", 0, 12, 1.0),
    k("backspace", "Bksp", 0, 13, 2.0),
    k("home", "Home", 0, 14, 1.0),
];

pub const ROW_1: &[KeyDef] = &[
    k("tab", "Tab", 1, 0, 1.5),
    k("q", "Q", 1, 1, 1.0),
    k("w", "W", 1, 2, 1.0),
    k("e", "E", 1, 3, 1.0),
    k("r", "R", 1, 4, 1.0),
    k("t", "T", 1, 5, 1.0),
    k("y", "Y", 1, 6, 1.0),
    k("u", "U", 1, 7, 1.0),
    k("i", "I", 1, 8, 1.0),
    k("o", "O", 1, 9, 1.0),
    k("p", "P", 1, 10, 1.0),
    k("lbracket", "[", 1, 11, 1.0),
    k("rbracket", "]", 1, 12, 1.0),
    k("backslash", "\\", 1, 13, 1.5),
    k("delete", "Del", 1, 14, 1.0),
];

pub const ROW_2: &[KeyDef] = &[
    k("caps", "Caps", 2, 0, 1.75),
    k("a", "A", 2, 1, 1.0),
    k("s", "S", 2, 2, 1.0),
    k("d", "D", 2, 3, 1.0),
    k("f", "F", 2, 4, 1.0),
    k("g", "G", 2, 5, 1.0),
    k("h", "H", 2, 6, 1.0),
    k("j", "J", 2, 7, 1.0),
    k("k", "K", 2, 8, 1.0),
    k("l", "L", 2, 9, 1.0),
    k("semicolon", ";", 2, 10, 1.0),
    k("quote", "'", 2, 11, 1.0),
    k("enter", "Enter", 2, 12, 2.25),
    k("pgup", "PgUp", 2, 14, 1.0),
];

pub const ROW_3: &[KeyDef] = &[
    k("lshift", "Shift", 3, 0, 2.25),
    k("z", "Z", 3, 2, 1.0),
    k("x", "X", 3, 3, 1.0),
    k("c", "C", 3, 4, 1.0),
    k("v", "V", 3, 5, 1.0),
    k("b", "B", 3, 6, 1.0),
    k("n", "N", 3, 7, 1.0),
    k("m", "M", 3, 8, 1.0),
    k("comma", ",", 3, 9, 1.0),
    k("period", ".", 3, 10, 1.0),
    k("slash", "/", 3, 11, 1.0),
    k("rshift", "Shift", 3, 12, 1.75),
    k("up", "↑", 3, 13, 1.0),
    k("pgdn", "PgDn", 3, 14, 1.0),
];

pub const ROW_4: &[KeyDef] = &[
    k("lctrl", "Ctrl", 4, 0, 1.25),
    k("lwin", "Win", 4, 1, 1.25),
    k("lalt", "Alt", 4, 2, 1.25),
    k("lspace", "Spc L", 4, 5, 2.0),
    k("space", "Space", 4, 6, 2.25),
    k("rspace", "Spc R", 4, 7, 2.0),
    k("ralt", "Alt", 4, 10, 1.25),
    k("fn", "Fn", 4, 11, 1.25),
    kg("left", "←", 4, 12, 1.0, 0.5),
    k("down", "↓", 4, 13, 1.0),
    k("right", "→", 4, 14, 1.0),
];

pub const KEYBOARD_ROWS: [&[KeyDef]; 5] = [ROW_0, ROW_1, ROW_2, ROW_3, ROW_4];

pub fn all_key_names() -> Vec<&'static str> {
    KEYBOARD_ROWS
        .iter()
        .flat_map(|row| row.iter().map(|k| k.name))
        .collect()
}

pub const PRESET_WASD: &[&str] = &["w", "a", "s", "d"];
pub const PRESET_ARROWS: &[&str] = &["up", "down", "left", "right"];
pub const PRESET_NUMBERS: &[&str] = &["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];
pub const PRESET_ALPHAS: &[&str] = &[
    "q", "w", "e", "r", "t", "y", "u", "i", "o", "p", "a", "s", "d", "f", "g", "h", "j", "k", "l",
    "z", "x", "c", "v", "b", "n", "m",
];
pub const PRESET_MODIFIERS: &[&str] = &[
    "esc", "tab", "caps", "lshift", "rshift", "lctrl", "lwin", "lalt", "ralt", "fn", "backspace",
    "enter",
];
pub const PRESET_NAV: &[&str] = &["home", "delete", "pgup", "pgdn", "up", "down", "left", "right"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_widths_sum_to_16u() {
        for (i, row) in KEYBOARD_ROWS.iter().enumerate() {
            let sum: f32 = row.iter().map(|k| k.width + k.gap_before).sum();
            assert!((sum - 16.0).abs() < 1e-4, "row {i} width sum was {sum}");
        }
    }

    #[test]
    fn key_count_matches_driver_layout() {
        assert_eq!(all_key_names().len(), 69);
        let driver_keys: Vec<&'static str> = origins65::layout::KEYS.iter().map(|k| k.name).collect();
        assert_eq!(all_key_names(), driver_keys);
    }
}

use origins65::model::Model;

/// On-screen keyboards for the key mask, matching `tools/o65/layout.py`:
/// - [`KEYBOARD_ROWS`]: Origins 65, US 67-key (+ 2 outer spacebar LEDs = 69 entries); each row's
///   physical widths sum to 16.0 keycap units.
/// - [`TKL_ROWS`]: Origins Core, US 87-key tenkeyless.
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

const TKL_ROW_0: &[KeyDef] = &[
    k("esc", "Esc", 0, 0, 1.0),
    kg("f1", "F1", 0, 2, 1.0, 1.0),
    k("f2", "F2", 0, 3, 1.0),
    k("f3", "F3", 0, 4, 1.0),
    k("f4", "F4", 0, 5, 1.0),
    kg("f5", "F5", 0, 6, 1.0, 0.5),
    k("f6", "F6", 0, 7, 1.0),
    k("f7", "F7", 0, 8, 1.0),
    k("f8", "F8", 0, 9, 1.0),
    kg("f9", "F9", 0, 10, 1.0, 0.5),
    k("f10", "F10", 0, 11, 1.0),
    k("f11", "F11", 0, 12, 1.0),
    k("f12", "F12", 0, 13, 1.0),
    kg("print", "PrtSc", 0, 14, 1.0, 0.25),
    k("scroll", "ScrLk", 0, 15, 1.0),
    k("pause", "Pause", 0, 16, 1.0),
];

const TKL_ROW_1: &[KeyDef] = &[
    k("grave", "`", 1, 0, 1.0),
    k("1", "1", 1, 1, 1.0),
    k("2", "2", 1, 2, 1.0),
    k("3", "3", 1, 3, 1.0),
    k("4", "4", 1, 4, 1.0),
    k("5", "5", 1, 5, 1.0),
    k("6", "6", 1, 6, 1.0),
    k("7", "7", 1, 7, 1.0),
    k("8", "8", 1, 8, 1.0),
    k("9", "9", 1, 9, 1.0),
    k("0", "0", 1, 10, 1.0),
    k("minus", "-", 1, 11, 1.0),
    k("equals", "=", 1, 12, 1.0),
    k("backspace", "Bksp", 1, 13, 2.0),
    kg("insert", "Ins", 1, 14, 1.0, 0.25),
    k("home", "Home", 1, 15, 1.0),
    k("pgup", "PgUp", 1, 16, 1.0),
];

const TKL_ROW_2: &[KeyDef] = &[
    k("tab", "Tab", 2, 0, 1.5),
    k("q", "Q", 2, 1, 1.0),
    k("w", "W", 2, 2, 1.0),
    k("e", "E", 2, 3, 1.0),
    k("r", "R", 2, 4, 1.0),
    k("t", "T", 2, 5, 1.0),
    k("y", "Y", 2, 6, 1.0),
    k("u", "U", 2, 7, 1.0),
    k("i", "I", 2, 8, 1.0),
    k("o", "O", 2, 9, 1.0),
    k("p", "P", 2, 10, 1.0),
    k("lbracket", "[", 2, 11, 1.0),
    k("rbracket", "]", 2, 12, 1.0),
    k("backslash", "\\", 2, 13, 1.5),
    kg("delete", "Del", 2, 14, 1.0, 0.25),
    k("end", "End", 2, 15, 1.0),
    k("pgdn", "PgDn", 2, 16, 1.0),
];

const TKL_ROW_3: &[KeyDef] = &[
    k("caps", "Caps", 3, 0, 1.75),
    k("a", "A", 3, 1, 1.0),
    k("s", "S", 3, 2, 1.0),
    k("d", "D", 3, 3, 1.0),
    k("f", "F", 3, 4, 1.0),
    k("g", "G", 3, 5, 1.0),
    k("h", "H", 3, 6, 1.0),
    k("j", "J", 3, 7, 1.0),
    k("k", "K", 3, 8, 1.0),
    k("l", "L", 3, 9, 1.0),
    k("semicolon", ";", 3, 10, 1.0),
    k("quote", "'", 3, 11, 1.0),
    k("enter", "Enter", 3, 12, 2.25),
];

const TKL_ROW_4: &[KeyDef] = &[
    k("lshift", "Shift", 4, 0, 2.25),
    k("z", "Z", 4, 2, 1.0),
    k("x", "X", 4, 3, 1.0),
    k("c", "C", 4, 4, 1.0),
    k("v", "V", 4, 5, 1.0),
    k("b", "B", 4, 6, 1.0),
    k("n", "N", 4, 7, 1.0),
    k("m", "M", 4, 8, 1.0),
    k("comma", ",", 4, 9, 1.0),
    k("period", ".", 4, 10, 1.0),
    k("slash", "/", 4, 11, 1.0),
    k("rshift", "Shift", 4, 13, 2.75),
    kg("up", "↑", 4, 15, 1.0, 1.25),
];

const TKL_ROW_5: &[KeyDef] = &[
    k("lctrl", "Ctrl", 5, 0, 1.25),
    k("lwin", "Win", 5, 1, 1.25),
    k("lalt", "Alt", 5, 2, 1.25),
    k("space", "Space", 5, 6, 6.25),
    k("ralt", "Alt", 5, 10, 1.25),
    k("fn", "Fn", 5, 11, 1.25),
    k("menu", "Menu", 5, 12, 1.25),
    k("rctrl", "Ctrl", 5, 13, 1.25),
    kg("left", "←", 5, 14, 1.0, 0.25),
    k("down", "↓", 5, 15, 1.0),
    k("right", "→", 5, 16, 1.0),
];

pub const TKL_ROWS: [&[KeyDef]; 6] = [TKL_ROW_0, TKL_ROW_1, TKL_ROW_2, TKL_ROW_3, TKL_ROW_4, TKL_ROW_5];

/// The on-screen rows for a keyboard model.
pub fn rows_for(model: Model) -> &'static [&'static [KeyDef]] {
    match model {
        Model::Origins65 => &KEYBOARD_ROWS,
        Model::OriginsCore => &TKL_ROWS,
    }
}

/// Width in keycap units of the widest row (gaps included).
pub fn layout_width_units(model: Model) -> f32 {
    rows_for(model)
        .iter()
        .map(|row| row.iter().map(|k| k.width + k.gap_before).sum::<f32>())
        .fold(0.0, f32::max)
}

/// Panel title for the key mask.
pub fn layout_title(model: Model) -> &'static str {
    match model {
        Model::Origins65 => "KEYBOARD KEY MASK (HYPERX ALLOY ORIGINS 65 — US 67-KEY)",
        Model::OriginsCore => "KEYBOARD KEY MASK (HYPERX ALLOY ORIGINS CORE — US 87-KEY TKL)",
    }
}

pub fn all_key_names_for(model: Model) -> Vec<&'static str> {
    rows_for(model)
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
    fn tkl_rows_match_driver_layout() {
        let mut ours: Vec<&str> = all_key_names_for(Model::OriginsCore);
        let mut driver: Vec<&str> = Model::OriginsCore.keys().iter().map(|k| k.name).collect();
        assert_eq!(ours.len(), 87);
        ours.sort_unstable();
        driver.sort_unstable();
        assert_eq!(ours, driver);
        for row in TKL_ROWS {
            for key in row {
                let dk = Model::OriginsCore.key_by_name(key.name).unwrap();
                assert_eq!((key.row, key.col), (dk.row, dk.col), "{}", key.name);
            }
        }
        assert!((layout_width_units(Model::OriginsCore) - 18.25).abs() < 1e-4);
        assert!((layout_width_units(Model::Origins65) - 16.0).abs() < 1e-4);
    }

    #[test]
    fn row_widths_sum_to_16u() {
        for (i, row) in KEYBOARD_ROWS.iter().enumerate() {
            let sum: f32 = row.iter().map(|k| k.width + k.gap_before).sum();
            assert!((sum - 16.0).abs() < 1e-4, "row {i} width sum was {sum}");
        }
    }

    #[test]
    fn key_count_matches_driver_layout() {
        assert_eq!(all_key_names_for(Model::Origins65).len(), 69);
        let driver_keys: Vec<&'static str> = origins65::layout::KEYS.iter().map(|k| k.name).collect();
        assert_eq!(all_key_names_for(Model::Origins65), driver_keys);
    }
}

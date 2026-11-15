//! Player settings, read from `settings.json` next to the save file.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// Remappable play-screen actions and their default keys.
pub const ACTIONS: &[(&str, char)] = &[
    ("rotate", 'r'),
    ("paint", 'v'),
    ("pick", 'q'),
    ("remove", 'x'),
    ("inspect", 'e'),
    ("mine", 'm'),
    ("build_menu", 'b'),
    ("craft", 'c'),
    ("research", 't'),
    ("stats", 'g'),
    ("log", 'l'),
    ("power_overlay", 'o'),
    ("pause", 'p'),
    ("fast", 'f'),
    ("select_area", 'z'),
    ("undo", 'u'),
    ("next_alert", 'n'),
    ("minimap", 'M'),
];

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Settings {
    /// Click to place, drag to lay belts, wheel to scroll. Turn off to use the terminal's own selection.
    pub mouse: bool,
    /// "default" or "colorblind".
    pub palette: String,
    /// ASCII-only glyphs (same as --ascii).
    pub ascii: bool,
    pub autosave_minutes: u64,
    /// Action name → key, e.g. {"mine": "n"}. Only listed actions change.
    pub keys: BTreeMap<String, String>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            mouse: true,
            palette: "default".into(),
            ascii: false,
            autosave_minutes: 5,
            keys: BTreeMap::new(),
        }
    }
}

impl Settings {
    /// Loads settings, writing a default file first if there is none. Errors fall back to defaults.
    pub fn load_or_create(path: &Path) -> (Settings, Option<String>) {
        match std::fs::read_to_string(path) {
            Ok(s) => match serde_json::from_str(&s) {
                Ok(set) => (set, None),
                Err(e) => (
                    Settings::default(),
                    Some(format!("Ignoring {}: {e}", path.display())),
                ),
            },
            Err(_) => {
                let set = Settings::default();
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                if let Ok(json) = serde_json::to_string_pretty(&set) {
                    let _ = std::fs::write(path, json);
                }
                (set, None)
            }
        }
    }

    /// Translation from a pressed key to the default key it stands for.
    /// `None` means the key is unbound because its action moved elsewhere.
    pub fn key_map(&self) -> (KeyMap, Vec<String>) {
        let mut map = KeyMap::default();
        let mut problems = vec![];
        for (action, key) in &self.keys {
            let Some(&(_, default)) = ACTIONS.iter().find(|(a, _)| a == action) else {
                problems.push(format!("unknown action \"{action}\""));
                continue;
            };
            let mut chars = key.chars();
            let (Some(c), None) = (chars.next(), chars.next()) else {
                problems.push(format!("key for \"{action}\" must be one character"));
                continue;
            };
            if c != default {
                map.0.insert(c, Some(default));
                map.0.entry(default).or_insert(None);
            }
        }
        (map, problems)
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct KeyMap(BTreeMap<char, Option<char>>);

impl KeyMap {
    /// The default key `c` means, or `None` if it does nothing.
    pub fn translate(&self, c: char) -> Option<char> {
        match self.0.get(&c) {
            Some(t) => *t,
            None => Some(c),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remapping_moves_an_action() {
        let mut s = Settings::default();
        s.keys.insert("mine".into(), "k".into());
        let (m, problems) = s.key_map();
        assert!(problems.is_empty());
        assert_eq!(m.translate('k'), Some('m'));
        assert_eq!(m.translate('m'), None);
        assert_eq!(m.translate('b'), Some('b'));
    }

    #[test]
    fn swapping_two_keys_works() {
        let mut s = Settings::default();
        s.keys.insert("mine".into(), "x".into());
        s.keys.insert("remove".into(), "m".into());
        let (m, _) = s.key_map();
        assert_eq!(m.translate('x'), Some('m'));
        assert_eq!(m.translate('m'), Some('x'));
    }

    #[test]
    fn missing_fields_use_defaults() {
        let s: Settings = serde_json::from_str(r#"{"mouse": false}"#).unwrap();
        assert!(!s.mouse);
        assert_eq!(s.autosave_minutes, 5);
    }
}

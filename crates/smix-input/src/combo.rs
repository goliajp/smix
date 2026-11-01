//! A key pressed with modifiers held: `ctrl+s`, `ctrl+shift+z`, `shift+tab`.

use serde::{Deserialize, Serialize};

/// One key pressed while the named modifiers are held — a keyboard
/// shortcut. Written `mod+mod+key`, modifiers first, any case.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyCombo {
    /// Ctrl held.
    pub ctrl: bool,
    /// Shift held.
    pub shift: bool,
    /// Alt (Option) held.
    pub alt: bool,
    /// Meta (Search on a Chromebook, Command on a Mac keyboard) held.
    pub meta: bool,
    /// The key pressed.
    pub key: ComboKey,
}

/// The key a [`KeyCombo`] presses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[non_exhaustive]
pub enum ComboKey {
    /// A letter `a`–`z` or a digit `0`–`9`, kept lowercase.
    Char(char),
    /// Enter / Return.
    Enter,
    /// Tab.
    Tab,
    /// Space.
    Space,
    /// Escape.
    Escape,
    /// Backspace: deletes before the cursor.
    Backspace,
    /// Delete: deletes after the cursor.
    Delete,
    /// Up arrow.
    Up,
    /// Down arrow.
    Down,
    /// Left arrow.
    Left,
    /// Right arrow.
    Right,
    /// A function key, `f1`–`f12`.
    F(u8),
}

impl ComboKey {
    /// The key's name in Android's `KeyEvent.KEYCODE_*`, without the prefix.
    #[must_use]
    pub fn android_name(self) -> String {
        match self {
            ComboKey::Char(c) => c.to_ascii_uppercase().to_string(),
            ComboKey::Enter => "ENTER".into(),
            ComboKey::Tab => "TAB".into(),
            ComboKey::Space => "SPACE".into(),
            ComboKey::Escape => "ESCAPE".into(),
            ComboKey::Backspace => "DEL".into(),
            ComboKey::Delete => "FORWARD_DEL".into(),
            ComboKey::Up => "DPAD_UP".into(),
            ComboKey::Down => "DPAD_DOWN".into(),
            ComboKey::Left => "DPAD_LEFT".into(),
            ComboKey::Right => "DPAD_RIGHT".into(),
            ComboKey::F(n) => format!("F{n}"),
        }
    }

    fn parse(s: &str) -> Option<ComboKey> {
        let mut chars = s.chars();
        if let (Some(c), None) = (chars.next(), chars.next())
            && c.is_ascii_alphanumeric()
        {
            return Some(ComboKey::Char(c.to_ascii_lowercase()));
        }
        Some(match s {
            "enter" | "return" => ComboKey::Enter,
            "tab" => ComboKey::Tab,
            "space" => ComboKey::Space,
            "escape" | "esc" => ComboKey::Escape,
            "backspace" => ComboKey::Backspace,
            "delete" | "del" => ComboKey::Delete,
            "up" => ComboKey::Up,
            "down" => ComboKey::Down,
            "left" => ComboKey::Left,
            "right" => ComboKey::Right,
            f if f.starts_with('f') => match f[1..].parse::<u8>() {
                Ok(n @ 1..=12) => ComboKey::F(n),
                _ => return None,
            },
            _ => return None,
        })
    }
}

impl std::fmt::Display for ComboKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ComboKey::Char(c) => write!(f, "{c}"),
            ComboKey::Enter => f.write_str("enter"),
            ComboKey::Tab => f.write_str("tab"),
            ComboKey::Space => f.write_str("space"),
            ComboKey::Escape => f.write_str("escape"),
            ComboKey::Backspace => f.write_str("backspace"),
            ComboKey::Delete => f.write_str("delete"),
            ComboKey::Up => f.write_str("up"),
            ComboKey::Down => f.write_str("down"),
            ComboKey::Left => f.write_str("left"),
            ComboKey::Right => f.write_str("right"),
            ComboKey::F(n) => write!(f, "f{n}"),
        }
    }
}

impl std::fmt::Display for KeyCombo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (held, name) in [
            (self.ctrl, "ctrl"),
            (self.shift, "shift"),
            (self.alt, "alt"),
            (self.meta, "meta"),
        ] {
            if held {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", self.key)
    }
}

impl std::str::FromStr for KeyCombo {
    type Err = String;

    /// `ctrl+s`, `Ctrl+Shift+Z`, `shift+tab`: modifiers then one key, joined
    /// by `+`. At least one modifier; a key alone is a plain `pressKey`.
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let lower = s.trim().to_ascii_lowercase();
        let parts: Vec<&str> = lower.split('+').map(str::trim).collect();
        let (key, mods) = parts
            .split_last()
            .ok_or_else(|| format!("{s:?} names no key"))?;
        if mods.is_empty() {
            return Err(format!(
                "{s:?} holds no modifier; a key combination is written like ctrl+s or ctrl+shift+z"
            ));
        }
        let key = ComboKey::parse(key).ok_or_else(|| {
            format!(
                "{key:?} in {s:?} is not a key smix presses in a combination: a letter, a digit, \
                 enter, tab, space, escape, backspace, delete, up, down, left, right or f1–f12"
            )
        })?;
        let mut combo = KeyCombo {
            ctrl: false,
            shift: false,
            alt: false,
            meta: false,
            key,
        };
        for m in mods {
            let held = match *m {
                "ctrl" | "control" => &mut combo.ctrl,
                "shift" => &mut combo.shift,
                "alt" | "option" => &mut combo.alt,
                "meta" | "cmd" | "command" | "search" => &mut combo.meta,
                other => {
                    return Err(format!(
                        "{other:?} in {s:?} is not a modifier: ctrl, shift, alt or meta"
                    ));
                }
            };
            if *held {
                return Err(format!("{m:?} is held twice in {s:?}"));
            }
            *held = true;
        }
        Ok(combo)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_combination_is_modifiers_then_one_key_in_any_case() {
        let c: KeyCombo = "Ctrl+Shift+Z".parse().unwrap();
        assert!(c.ctrl && c.shift && !c.alt && !c.meta);
        assert_eq!(c.key, ComboKey::Char('z'));
        assert_eq!(c.to_string(), "ctrl+shift+z");
        let c: KeyCombo = "shift + tab".parse().unwrap();
        assert_eq!((c.shift, c.key), (true, ComboKey::Tab));
        let c: KeyCombo = "cmd+f5".parse().unwrap();
        assert_eq!((c.meta, c.key), (true, ComboKey::F(5)));
    }

    #[test]
    fn the_key_is_named_as_android_names_it() {
        let name = |s: &str| s.parse::<KeyCombo>().unwrap().key.android_name();
        assert_eq!(name("ctrl+s"), "S");
        assert_eq!(name("ctrl+7"), "7");
        assert_eq!(name("ctrl+enter"), "ENTER");
        assert_eq!(name("ctrl+backspace"), "DEL");
        assert_eq!(name("ctrl+delete"), "FORWARD_DEL");
        assert_eq!(name("alt+left"), "DPAD_LEFT");
        assert_eq!(name("alt+f4"), "F4");
    }

    #[test]
    fn what_is_not_a_combination_is_named() {
        let e = |s: &str| s.parse::<KeyCombo>().unwrap_err();
        assert!(e("s").contains("no modifier"), "{}", e("s"));
        assert!(e("ctrl+hyper").contains("not a key"), "{}", e("ctrl+hyper"));
        assert!(e("ctrl+f13").contains("not a key"));
        assert!(e("super+s").contains("not a modifier"), "{}", e("super+s"));
        assert!(e("ctrl+ctrl+s").contains("held twice"));
        assert!(e("ctrl+").contains("not a key"));
    }
}

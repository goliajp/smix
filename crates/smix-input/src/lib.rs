#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![doc(html_root_url = "https://docs.smix.dev/smix-input")]

//! Small wire-only types shared by smix-driver / smix-authoring-ir /
//! smix-runner-client. Kept as a separate crate so cement (CLI / MCP /
//! SDK / recorder) can depend on them without dragging in heavier
//! driver / runner deps.

use serde::{Deserialize, Serialize};

/// Maestro yaml `direction:` semantic: the direction names what
/// content the caller wants to **see** (navigation through content),
/// NOT the finger gesture direction. `Down` = "navigate down through
/// content" = reveal what's BELOW the current viewport (visually content
/// moves up, finger gestures up). Mirrors maestro CLI `direction: DOWN`
/// semantics.
///
/// The name "SwipeDirection" predates this convention and now
/// reads as a slight misnomer (the value is the *navigation* direction,
/// not the *swipe gesture* direction). Renaming the enum would ripple
/// across every adapter/driver/runner crate without semantic gain;
/// instead the docstring carries the contract. Both runners (swift
/// XCUITest + Kotlin UiAutomator) map this enum's wire string to the
/// inverse finger gesture (e.g. `Down` → finger up via swipeUp / coord
/// y 70→30) so the same yaml flow yields the same visual behavior on
/// both platforms.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SwipeDirection {
    /// Navigate up = see what's above (content moves down; finger gestures down).
    Up,
    /// Navigate down = see what's below (content moves up; finger gestures up).
    Down,
    /// Navigate left = see what's to the left (content moves right; finger gestures right).
    Left,
    /// Navigate right = see what's to the right (content moves left; finger gestures left).
    Right,
}

impl SwipeDirection {
    /// camelCase wire string (mirrors `roleSchema` style).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SwipeDirection::Up => "up",
            SwipeDirection::Down => "down",
            SwipeDirection::Left => "left",
            SwipeDirection::Right => "right",
        }
    }
}

impl std::fmt::Display for SwipeDirection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Keyboard key name for `press_key` / `record` (camelCase on the wire).
///
/// The subset is intentional — these are the keys SDK users reliably
/// exercise in iOS-sim contexts. Arrow keys are included because
/// focus-traversal flows need them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum KeyName {
    /// Return / Enter key — submits a form or confirms an action.
    Return,
    /// Delete / Backspace key — deletes the character before the cursor.
    Delete,
    /// Tab key — moves focus to the next focusable element.
    Tab,
    /// Space key — inserts a space character.
    Space,
    /// Escape key — dismisses a modal or cancels an action.
    Escape,
    /// Up-arrow key — moves selection / focus / caret upward.
    ArrowUp,
    /// Down-arrow key — moves selection / focus / caret downward.
    ArrowDown,
    /// Left-arrow key — moves selection / focus / caret leftward.
    ArrowLeft,
    /// Right-arrow key — moves selection / focus / caret rightward.
    ArrowRight,
    /// iOS hardware Home button — XCUIDevice.shared.perform(.homeButton).
    /// Maps 1:1 to maestro `pressKey: home`.
    Home,
    /// iOS hardware Lock button — XCUIDevice.shared.perform(.lockButton).
    /// Maps 1:1 to maestro `pressKey: lock`.
    Lock,
    /// iOS hardware Volume Up button — XCUIDevice.Button.volumeUp.
    /// Maps 1:1 to maestro `pressKey: volume up`.
    VolumeUp,
    /// iOS hardware Volume Down button — XCUIDevice.Button.volumeDown.
    /// Maps 1:1 to maestro `pressKey: volume down`.
    VolumeDown,
    /// Back: the same thing the `back` verb does, and answered the same
    /// way — Android's system back, iOS's navigation-bar back. Maestro
    /// `pressKey: Back`.
    Back,
}

impl KeyName {
    /// camelCase wire string.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            KeyName::Return => "return",
            KeyName::Delete => "delete",
            KeyName::Tab => "tab",
            KeyName::Space => "space",
            KeyName::Escape => "escape",
            KeyName::ArrowUp => "arrowUp",
            KeyName::ArrowDown => "arrowDown",
            KeyName::ArrowLeft => "arrowLeft",
            KeyName::ArrowRight => "arrowRight",
            KeyName::Home => "home",
            KeyName::Lock => "lock",
            KeyName::VolumeUp => "volumeUp",
            KeyName::VolumeDown => "volumeDown",
            KeyName::Back => "back",
        }
    }

    /// Every key, in declaration order.
    pub const ALL: [KeyName; 14] = [
        KeyName::Return,
        KeyName::Delete,
        KeyName::Tab,
        KeyName::Space,
        KeyName::Escape,
        KeyName::ArrowUp,
        KeyName::ArrowDown,
        KeyName::ArrowLeft,
        KeyName::ArrowRight,
        KeyName::Home,
        KeyName::Lock,
        KeyName::VolumeUp,
        KeyName::VolumeDown,
        KeyName::Back,
    ];

    /// The key a person or a flow names, the one place a key name is
    /// read — `pressKey`, `smix press-key` and `smix_press_key` all
    /// come here.
    ///
    /// Case, spaces, underscores and hyphens are not part of a name, so
    /// maestro's `Volume Up`, a shell's `volume-up` and the wire's
    /// `volumeUp` are one key.
    ///
    /// # Errors
    ///
    /// [`KeyNameError`] naming what was written, and either why smix
    /// does not press that maestro key or which names it does read.
    pub fn from_name(name: &str) -> Result<KeyName, KeyNameError> {
        let folded: String = name
            .chars()
            .filter(|c| !matches!(c, ' ' | '_' | '-'))
            .flat_map(char::to_lowercase)
            .collect();
        if let Some(&(_, key)) = NAMES.iter().find(|(n, _)| *n == folded) {
            return Ok(key);
        }
        let not_here = |why| KeyNameError::NotPressedHere {
            name: name.to_string(),
            why,
        };
        if folded == "power" {
            return Err(not_here(
                "a phone's power button is its lock button here — write `lock`",
            ));
        }
        if folded.starts_with("remote") || folded.starts_with("tvinput") {
            return Err(not_here(
                "a TV remote or TV input key; smix drives phones and tablets, which have \
                 no such key to press",
            ));
        }
        Err(KeyNameError::Unknown {
            name: name.to_string(),
        })
    }
}

/// Every name [`KeyName::from_name`] reads, folded (lower case, no
/// spaces, `_` or `-`): each key's wire name, maestro's spelling where
/// it differs, and the shell shorthands smix has always taken.
const NAMES: [(&str, KeyName); 21] = [
    ("return", KeyName::Return),
    ("enter", KeyName::Return),
    ("delete", KeyName::Delete),
    ("backspace", KeyName::Delete),
    ("tab", KeyName::Tab),
    ("space", KeyName::Space),
    ("escape", KeyName::Escape),
    ("esc", KeyName::Escape),
    ("arrowup", KeyName::ArrowUp),
    ("up", KeyName::ArrowUp),
    ("arrowdown", KeyName::ArrowDown),
    ("down", KeyName::ArrowDown),
    ("arrowleft", KeyName::ArrowLeft),
    ("left", KeyName::ArrowLeft),
    ("arrowright", KeyName::ArrowRight),
    ("right", KeyName::ArrowRight),
    ("home", KeyName::Home),
    ("lock", KeyName::Lock),
    ("volumeup", KeyName::VolumeUp),
    ("volumedown", KeyName::VolumeDown),
    ("back", KeyName::Back),
];

/// Why a key name was not read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyNameError {
    /// Not a key name smix or maestro knows.
    Unknown {
        /// What was written.
        name: String,
    },
    /// A maestro key for a kind of device smix does not drive, or with
    /// another name here.
    NotPressedHere {
        /// What was written.
        name: String,
        /// Why, and what to write instead when there is something.
        why: &'static str,
    },
}

impl std::fmt::Display for KeyNameError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            KeyNameError::Unknown { name } => {
                write!(f, "unknown key {name:?}; the keys are ")?;
                for (i, k) in KeyName::ALL.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    f.write_str(k.as_str())?;
                }
                f.write_str(" (case, spaces, `_` and `-` do not matter)")
            }
            KeyNameError::NotPressedHere { name, why } => write!(f, "key {name:?}: {why}"),
        }
    }
}

impl std::error::Error for KeyNameError {}

impl std::fmt::Display for KeyName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]
#![doc(html_root_url = "https://docs.smix.dev/smix-input")]

//! Ported from now-retired TS source: `src/driver/types.ts:8-19`. Small wire-only types
//! shared by smix-driver / smix-recorder-ir / smix-runner-client. Kept as
//! a separate crate so cement (CLI / MCP / SDK / recorder) can depend on
//! them without dragging in heavier driver / runner deps.

use serde::{Deserialize, Serialize};

/// Maestro yaml `direction:` semantic (per
/// [[smix-must-be-superset-of-maestro]]): the direction names what
/// content the caller wants to **see** (navigation through content),
/// NOT the finger gesture direction. `Down` = "navigate down through
/// content" = reveal what's BELOW the current viewport (visually content
/// moves up, finger gestures up). Mirrors maestro CLI `direction: DOWN`
/// semantics.
///
/// v6.11 c1 — name "SwipeDirection" predates the convention pin and now
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

/// Keyboard key name for `press_key` / `record` (camelCase serde 1:1 跟 TS).
///
/// Subset is intentional — these are the keys SDK users reliably exercise
/// in iOS-sim contexts. Adding more is a c{N} discussion: arrow keys are
/// here because focus-traversal flows need them (v1.5 c5g'').
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
    /// v5.2 c2 — maestro `pressKey: home` ↔ smix core KeyName 平铺。
    Home,
    /// iOS hardware Lock button — XCUIDevice.shared.perform(.lockButton).
    /// v5.2 c2 — maestro `pressKey: lock` ↔ smix core KeyName 平铺。
    Lock,
    /// iOS hardware Volume Up button — XCUIDevice.Button.volumeUp.
    /// v5.2 c2 — maestro `pressKey: volume up` ↔ smix core KeyName 平铺。
    VolumeUp,
    /// iOS hardware Volume Down button — XCUIDevice.Button.volumeDown.
    /// v5.2 c2 — maestro `pressKey: volume down` ↔ smix core KeyName 平铺。
    VolumeDown,
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
        }
    }
}

impl std::fmt::Display for KeyName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

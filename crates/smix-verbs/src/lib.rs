//! smix-verbs — canonical verb table.
//!
//! # Reviewer invariant
//!
//! Any new yaml verb supported by smix MUST land here first as a
//! [`VerbEntry`] in [`VERB_TABLE`]. Both `smix-adapter-maestro` parser
//! and `smix-migrate` codemod depend on this crate; adding an entry
//! automatically makes the parser accept both canonical forms and the
//! codemod handle the rename.
//!
//! # Design
//!
//! - **Pure data crate** — no runtime state, no async, no I/O.
//! - **Static table** — `&'static [VerbEntry]` for zero-alloc lookup.
//! - **Two names per entry** — `maestro_name` (the yaml surface maestro
//!   test authors know) and `smix_name` (the smix-canonical form). If
//!   they're identical (e.g. `runFlow`), the entry still appears with
//!   both fields set to the same string.
//! - **Category** — grouping for reviewer sanity checks + future
//!   auto-generated cross-platform parity tables (Phase F).
//! - **Arg shape** — reserved for future codemod arg-transform lookup;
//!   currently informational.

#![forbid(unsafe_code)]

/// One verb in the canonical table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VerbEntry {
    /// Maestro yaml surface name (`tapOn`, `assertVisible`, ...).
    pub maestro_name: &'static str,
    /// smix-canonical surface name (`tap`, `expect`, ...).
    pub smix_name: &'static str,
    /// Grouping category. Used by the future cross-platform parity
    /// audit table (Phase F).
    pub category: VerbCategory,
    /// Shape of the argument — informational; used by codemod's
    /// argument transform lookup + parser's shape validation.
    pub arg_shape: ArgShape,
}

/// Semantic grouping for a verb.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VerbCategory {
    /// Tap-family (tap, tapOn, doubleTap, longPress, tapByCoord).
    Tap,
    /// Text-input family (inputText / fill, eraseText / clear,
    /// pasteText, setClipboard).
    Input,
    /// Assertion / expectation (assertVisible / expect,
    /// assertNotVisible, assertTrue, extendedWaitUntil, expect.signal).
    Assert,
    /// Navigation / control flow (runFlow, retry, repeat,
    /// pressKey, back).
    ControlFlow,
    /// App lifecycle (launchApp, stopApp / terminate, killApp,
    /// clearState / reset, clearKeychain / resetKeychain).
    Lifecycle,
    /// Screen / media (takeScreenshot, assertScreenshot,
    /// startRecording, stopRecording, addMedia).
    Media,
    /// Gesture / motion (scroll, scrollUntilVisible, swipe,
    /// swipeOnce, hideKeyboard).
    Gesture,
    /// Device-level control (openLink / openUrl, setLocation,
    /// travel, setPermissions, setOrientation, toggleAirplaneMode).
    Device,
    /// smix-native extensions (ocrText, anchorRelative, tapById,
    /// fixture, webviewEval).
    SmixNative,
    /// Utility / diagnostic (waitForAnimationToEnd, evalScript,
    /// copyTextFrom, assertWithAI).
    Utility,
}

/// Shape of a verb's argument. Determines how the parser + codemod
/// treat the yaml body.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArgShape {
    /// No argument (`- back`, `- stopApp`, `- waitForAnimationToEnd`).
    None,
    /// Bare string (`- inputText: hello`, `- pressKey: enter`).
    BareString,
    /// Selector — short-form string OR long-form mapping
    /// (`{ text | id | label ... }`).
    Selector,
    /// Mapping with domain-specific keys (`launchApp`, `extendedWaitUntil`,
    /// `assertScreenshot`, `expect.signal`).
    Mapping,
    /// Path string (`- runFlow: path/to.yaml`).
    Path,
    /// Coordinate pair or normalized 0..1 point
    /// (`tapOn: { point: "X%,Y%" }`).
    Coord,
    /// Integer count (`eraseText: 5`).
    Integer,
}

/// Static canonical verb table. Ordering: category-alphabetized
/// for grep-ability; each row is `(maestro, smix, category, shape)`.
pub static VERB_TABLE: &[VerbEntry] = &[
    // ----- Tap family -----
    v("tapOn", "tap", VerbCategory::Tap, ArgShape::Selector),
    v(
        "doubleTapOn",
        "doubleTap",
        VerbCategory::Tap,
        ArgShape::Selector,
    ),
    v(
        "longPressOn",
        "longPress",
        VerbCategory::Tap,
        ArgShape::Selector,
    ),
    v(
        "tapByCoord",
        "tapByCoord",
        VerbCategory::Tap,
        ArgShape::Coord,
    ),
    // ----- Input family -----
    v(
        "inputText",
        "fill",
        VerbCategory::Input,
        ArgShape::BareString,
    ),
    v("eraseText", "clear", VerbCategory::Input, ArgShape::Integer),
    v(
        "pasteText",
        "pasteText",
        VerbCategory::Input,
        ArgShape::None,
    ),
    v(
        "setClipboard",
        "setClipboard",
        VerbCategory::Input,
        ArgShape::BareString,
    ),
    v(
        "copyTextFrom",
        "copyTextFrom",
        VerbCategory::Input,
        ArgShape::Selector,
    ),
    // ----- Assert family -----
    v(
        "assertVisible",
        "expect",
        VerbCategory::Assert,
        ArgShape::Selector,
    ),
    v(
        "assertNotVisible",
        "expectNotVisible",
        VerbCategory::Assert,
        ArgShape::Selector,
    ),
    v(
        "extendedWaitUntil",
        "expect",
        VerbCategory::Assert,
        ArgShape::Mapping,
    ),
    v(
        "assertTrue",
        "assertTrue",
        VerbCategory::Assert,
        ArgShape::BareString,
    ),
    v(
        "assertScreenshot",
        "assertScreenshot",
        VerbCategory::Assert,
        ArgShape::Mapping,
    ),
    // ----- Control flow -----
    v(
        "runFlow",
        "runFlow",
        VerbCategory::ControlFlow,
        ArgShape::Path,
    ),
    v(
        "retry",
        "retry",
        VerbCategory::ControlFlow,
        ArgShape::Mapping,
    ),
    v(
        "repeat",
        "repeat",
        VerbCategory::ControlFlow,
        ArgShape::Mapping,
    ),
    v(
        "pressKey",
        "pressKey",
        VerbCategory::ControlFlow,
        ArgShape::BareString,
    ),
    v(
        "back",
        "pressKey",
        VerbCategory::ControlFlow,
        ArgShape::None,
    ),
    // ----- Lifecycle -----
    v(
        "launchApp",
        "launchApp",
        VerbCategory::Lifecycle,
        ArgShape::Mapping,
    ),
    v(
        "stopApp",
        "terminate",
        VerbCategory::Lifecycle,
        ArgShape::None,
    ),
    v(
        "killApp",
        "terminate",
        VerbCategory::Lifecycle,
        ArgShape::BareString,
    ),
    v(
        "clearState",
        "reset",
        VerbCategory::Lifecycle,
        ArgShape::None,
    ),
    v(
        "clearKeychain",
        "resetKeychain",
        VerbCategory::Lifecycle,
        ArgShape::None,
    ),
    // ----- Media -----
    v(
        "takeScreenshot",
        "takeScreenshot",
        VerbCategory::Media,
        ArgShape::BareString,
    ),
    v(
        "startRecording",
        "startRecording",
        VerbCategory::Media,
        ArgShape::BareString,
    ),
    v(
        "stopRecording",
        "stopRecording",
        VerbCategory::Media,
        ArgShape::None,
    ),
    v(
        "addMedia",
        "addMedia",
        VerbCategory::Media,
        ArgShape::Mapping,
    ),
    // ----- Gesture -----
    v("scroll", "scroll", VerbCategory::Gesture, ArgShape::None),
    v(
        "scrollUntilVisible",
        "scroll",
        VerbCategory::Gesture,
        ArgShape::Mapping,
    ),
    v("swipe", "swipe", VerbCategory::Gesture, ArgShape::Mapping),
    v(
        "hideKeyboard",
        "hideKeyboard",
        VerbCategory::Gesture,
        ArgShape::None,
    ),
    // ----- Device -----
    v(
        "openLink",
        "openUrl",
        VerbCategory::Device,
        ArgShape::BareString,
    ),
    v(
        "setLocation",
        "setLocation",
        VerbCategory::Device,
        ArgShape::Mapping,
    ),
    v("travel", "travel", VerbCategory::Device, ArgShape::Mapping),
    v(
        "setPermissions",
        "setPermissions",
        VerbCategory::Device,
        ArgShape::Mapping,
    ),
    v(
        "setOrientation",
        "setOrientation",
        VerbCategory::Device,
        ArgShape::BareString,
    ),
    v(
        "toggleAirplaneMode",
        "toggleAirplaneMode",
        VerbCategory::Device,
        ArgShape::None,
    ),
    // ----- smix-native extensions (parity with parser dispatch_step) -----
    v(
        "webview_eval",
        "webviewEval",
        VerbCategory::SmixNative,
        ArgShape::BareString,
    ),
    v(
        "webviewEval",
        "webviewEval",
        VerbCategory::SmixNative,
        ArgShape::BareString,
    ),
    v(
        "fixture",
        "fixture",
        VerbCategory::SmixNative,
        ArgShape::Mapping,
    ),
    v(
        "tapById",
        "tapById",
        VerbCategory::SmixNative,
        ArgShape::BareString,
    ),
    v(
        "tapAtCoord",
        "tapAtCoord",
        VerbCategory::SmixNative,
        ArgShape::Coord,
    ),
    v(
        "swipeAtCoord",
        "swipeAtCoord",
        VerbCategory::SmixNative,
        ArgShape::Coord,
    ),
    v(
        "doubleTap",
        "doubleTap",
        VerbCategory::SmixNative,
        ArgShape::Selector,
    ),
    v(
        "longPress",
        "longPress",
        VerbCategory::SmixNative,
        ArgShape::Selector,
    ),
    v(
        "ocrText",
        "ocrText",
        VerbCategory::SmixNative,
        ArgShape::BareString,
    ),
    v(
        "anchorRelative",
        "anchorRelative",
        VerbCategory::SmixNative,
        ArgShape::Mapping,
    ),
    v(
        "findTextByOcr",
        "findTextByOcr",
        VerbCategory::SmixNative,
        ArgShape::Mapping,
    ),
    v(
        "expect",
        "expect",
        VerbCategory::SmixNative,
        ArgShape::Mapping,
    ),
    // ----- Utility -----
    v(
        "waitForAnimationToEnd",
        "waitForAnimationToEnd",
        VerbCategory::Utility,
        ArgShape::None,
    ),
    // Note: `evalScript` / `runScript` / `assertWithAI` deliberately
    // NOT in the table — they're valid maestro verbs but flagged as
    // unknown by migrate to warn corpus maintainers that porting
    // requires manual review (typical case: script bodies contain
    // maestro-specific APIs). Consumers who want them silently
    // preserved can add rows locally.
];

const fn v(
    maestro_name: &'static str,
    smix_name: &'static str,
    category: VerbCategory,
    arg_shape: ArgShape,
) -> VerbEntry {
    VerbEntry {
        maestro_name,
        smix_name,
        category,
        arg_shape,
    }
}

/// Look up a verb entry by its maestro-canonical name.
pub fn find_by_maestro(name: &str) -> Option<&'static VerbEntry> {
    VERB_TABLE.iter().find(|e| e.maestro_name == name)
}

/// Look up a verb entry by its smix-canonical name. May return
/// multiple different entries because smix names are not unique
/// (e.g. `pressKey` is the smix name for both `pressKey` and
/// `back`); we return the first match. Consumers that need
/// disambiguation should use [`find_by_maestro`] or scan
/// [`VERB_TABLE`] directly.
pub fn find_by_smix(name: &str) -> Option<&'static VerbEntry> {
    VERB_TABLE.iter().find(|e| e.smix_name == name)
}

/// True if `name` matches either the maestro or smix form of any
/// known verb.
pub fn is_known_verb(name: &str) -> bool {
    find_by_maestro(name).is_some() || find_by_smix(name).is_some()
}

/// Return every entry in a given category. Iteration order is the
/// insertion order in [`VERB_TABLE`].
pub fn verbs_in_category(category: VerbCategory) -> impl Iterator<Item = &'static VerbEntry> {
    VERB_TABLE.iter().filter(move |e| e.category == category)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn find_by_maestro_hits() {
        let e = find_by_maestro("tapOn").unwrap();
        assert_eq!(e.smix_name, "tap");
        assert_eq!(e.category, VerbCategory::Tap);
    }

    #[test]
    fn find_by_smix_hits() {
        let e = find_by_smix("tap").unwrap();
        assert_eq!(e.maestro_name, "tapOn");
    }

    #[test]
    fn find_by_smix_returns_first_when_multiple_match() {
        // `pressKey` is the smix name for both `pressKey` and `back`
        // entries. find_by_smix returns the FIRST hit — that's
        // documented behavior; consumers wanting disambiguation
        // must scan VERB_TABLE directly.
        let e = find_by_smix("pressKey").unwrap();
        assert!(e.maestro_name == "pressKey" || e.maestro_name == "back");
    }

    #[test]
    fn is_known_covers_both_forms() {
        assert!(is_known_verb("tapOn"));
        assert!(is_known_verb("tap"));
        assert!(is_known_verb("assertVisible"));
        assert!(is_known_verb("expect"));
        assert!(!is_known_verb("nonexistent"));
    }

    #[test]
    fn extendedwait_maps_to_expect() {
        let e = find_by_maestro("extendedWaitUntil").unwrap();
        assert_eq!(e.smix_name, "expect");
        assert_eq!(e.category, VerbCategory::Assert);
        assert_eq!(e.arg_shape, ArgShape::Mapping);
    }

    #[test]
    fn table_has_no_duplicate_maestro_names() {
        let mut seen = Vec::new();
        for e in VERB_TABLE {
            if seen.contains(&e.maestro_name) {
                panic!("duplicate maestro_name: {}", e.maestro_name);
            }
            seen.push(e.maestro_name);
        }
    }

    #[test]
    fn each_category_has_at_least_one_entry() {
        for cat in [
            VerbCategory::Tap,
            VerbCategory::Input,
            VerbCategory::Assert,
            VerbCategory::ControlFlow,
            VerbCategory::Lifecycle,
            VerbCategory::Media,
            VerbCategory::Gesture,
            VerbCategory::Device,
            VerbCategory::SmixNative,
            VerbCategory::Utility,
        ] {
            let count = verbs_in_category(cat).count();
            assert!(count > 0, "category {cat:?} has no entries");
        }
    }

    #[test]
    fn table_covers_v0_2_5_migrate_default_rules() {
        // Sanity: every maestro name from smix-migrate's original
        // DEFAULT_RULES table is present here.
        for name in [
            "tapOn",
            "assertVisible",
            "assertNotVisible",
            "extendedWaitUntil",
            "inputText",
            "eraseText",
            "clearState",
            "clearKeychain",
            "runFlow",
            "launchApp",
            "stopApp",
            "killApp",
            "openLink",
            "back",
            "hideKeyboard",
            "pressKey",
            "scroll",
            "scrollUntilVisible",
            "swipe",
            "retry",
        ] {
            assert!(
                find_by_maestro(name).is_some(),
                "v0.2.5 DEFAULT_RULES verb {name:?} missing from VERB_TABLE"
            );
        }
    }

    #[test]
    fn table_covers_smix_native_verbs() {
        for name in ["fixture", "webviewEval"] {
            assert!(
                find_by_smix(name).is_some() || find_by_maestro(name).is_some(),
                "smix-native verb {name:?} missing from VERB_TABLE"
            );
        }
    }
}

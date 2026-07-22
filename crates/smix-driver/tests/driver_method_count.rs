//! Regression guard on the `Driver` trait's method roster. Removing or
//! renaming a method must be an explicit review decision, so the roster
//! is written out by name — the previous form asserted only
//! `count >= 26`, which passes a rename to anything and never notices a
//! swapped surface.
//!
//! Rust has no runtime reflection over trait methods, so the roster is
//! read from the trait's source text; the extraction refuses to pass
//! when its pattern stops matching anything.

use std::fs;
use std::path::PathBuf;

const ROSTER: &[&str] = &[
    "back",
    "clear",
    "double_tap",
    "fill",
    "find",
    "find_all",
    "find_norm_coord",
    "find_one",
    "find_text_by_ocr",
    "foreground",
    "hide_keyboard",
    "long_press",
    "press_key",
    "scroll",
    "set_orientation",
    "swipe_at_norm_coord",
    "swipe_once",
    "system_popup_action",
    "system_popups",
    "tap",
    "tap_at_norm_coord",
    "tap_burst",
    "tap_by_id",
    "tap_with_mode",
    "tree",
    "wait_for",
    "webview_eval",
];

#[test]
fn the_driver_trait_roster_is_exactly_the_reviewed_list() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src/traits.rs");
    let src = fs::read_to_string(path).expect("read src/traits.rs");
    let mut declared: Vec<String> = src
        .lines()
        .filter_map(|l| {
            let rest = l.trim().strip_prefix("async fn ")?;
            Some(rest.split('(').next().unwrap_or(rest).to_string())
        })
        .collect();
    declared.sort_unstable();
    declared.dedup();
    assert!(
        !declared.is_empty(),
        "extracted zero `async fn` names from traits.rs — the declaration \
         shape changed and this check would pass by knowing nothing"
    );

    let mut expected: Vec<String> = ROSTER.iter().map(|s| s.to_string()).collect();
    expected.sort_unstable();
    assert_eq!(
        declared, expected,
        "the Driver trait's async methods differ from the reviewed roster — \
         removals and renames need a deliberate update here"
    );
}

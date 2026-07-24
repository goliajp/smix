//! The Kotlin↔Rust record contract.
//!
//! The Android capture leg (`RecordMapper.kt`) emits IRAction JSON that the
//! generators here must consume verbatim. These are the exact strings that
//! leg produces for tap / fill / clear; if the IRAction wire shape drifts on
//! either side, this fails. Mirrors `RecordMapperTest.kt`'s expected output.

use smix_authoring_ir::IRAction;
use smix_recorder::{generate_maestro_yaml, generate_rust};

/// Byte-for-byte the JSON `RecordMapper.kt` emits (org.json key order aside —
/// serde is order-independent, and RecordMapperTest asserts the same fields).
const TAP: &str = r#"{"kind":"tap","selector":{"id":"login_btn"},"timestampMs":42}"#;
const FILL: &str = r#"{"kind":"fill","selector":{"id":"email"},"text":"a@b.co","timestampMs":1}"#;
const CLEAR: &str = r#"{"kind":"clear","selector":{"id":"q"},"timestampMs":1}"#;

fn parse_all() -> Vec<IRAction> {
    [TAP, FILL, CLEAR]
        .iter()
        .map(|s| {
            serde_json::from_str::<IRAction>(s).expect("Android IRAction JSON must deserialize")
        })
        .collect()
}

#[test]
fn android_json_deserializes_to_the_right_variants() {
    let actions = parse_all();
    assert!(matches!(actions[0], IRAction::Tap { .. }), "tap");
    assert!(matches!(actions[1], IRAction::Fill { .. }), "fill");
    assert!(matches!(actions[2], IRAction::Clear { .. }), "clear");

    if let IRAction::Fill { text, .. } = &actions[1] {
        assert_eq!(text, "a@b.co");
    }
}

#[test]
fn android_actions_generate_non_empty_maestro_and_rust() {
    let actions = parse_all();
    let yaml = generate_maestro_yaml(&actions, "com.android.settings").expect("maestro");
    assert!(yaml.contains("tapOn"), "maestro yaml should carry the tap");
    assert!(!yaml.trim().is_empty());

    let rust = generate_rust(&actions, "recorded", "com.android.settings").expect("rust");
    assert!(
        rust.contains("async fn recorded"),
        "rust source should be a test fn"
    );
    assert!(!rust.trim().is_empty());
}

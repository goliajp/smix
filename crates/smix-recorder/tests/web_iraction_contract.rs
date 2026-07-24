//! The web-leg <-> Rust record contract.
//!
//! The web capture leg (`npm/smix-web-record/src/mapDomEvents.ts`) emits
//! IRAction JSON the generators must consume verbatim — the same shape as the
//! Android leg, but with web selectors (id from data-testid, role, text).
//! Mirrors `mapDomEvents.test.ts`'s expected output; if the wire shape drifts
//! on either side, this fails.

use smix_authoring_ir::IRAction;
use smix_recorder::{generate_maestro_yaml, generate_rust};

const TAP_ID: &str = r#"{"kind":"tap","selector":{"id":"login-btn"},"timestampMs":42}"#;
const TAP_ROLE: &str = r#"{"kind":"tap","selector":{"role":"button"},"timestampMs":1}"#;
const TAP_TEXT: &str = r#"{"kind":"tap","selector":{"text":"Sign In"},"timestampMs":1}"#;
const FILL: &str = r#"{"kind":"fill","selector":{"id":"q"},"text":"smix","timestampMs":1}"#;
const CLEAR: &str = r#"{"kind":"clear","selector":{"id":"q"},"timestampMs":1}"#;

fn parse_all() -> Vec<IRAction> {
    [TAP_ID, TAP_ROLE, TAP_TEXT, FILL, CLEAR]
        .iter()
        .map(|s| serde_json::from_str::<IRAction>(s).expect("web IRAction JSON must deserialize"))
        .collect()
}

#[test]
fn web_json_deserializes_all_selector_kinds() {
    let a = parse_all();
    assert!(matches!(a[0], IRAction::Tap { .. }), "tap by id");
    assert!(matches!(a[1], IRAction::Tap { .. }), "tap by role");
    assert!(matches!(a[2], IRAction::Tap { .. }), "tap by text");
    assert!(matches!(a[3], IRAction::Fill { .. }), "fill");
    assert!(matches!(a[4], IRAction::Clear { .. }), "clear");
}

#[test]
fn web_actions_generate_non_empty_maestro_and_rust() {
    let a = parse_all();
    let yaml = generate_maestro_yaml(&a, "example.com").expect("maestro");
    assert!(yaml.contains("tapOn"));
    let rust = generate_rust(&a, "recorded", "example.com").expect("rust");
    assert!(rust.contains("async fn recorded"));
}

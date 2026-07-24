//! Three legs, one IR.
//!
//! iOS (RecordingApp), Android (RecordMapper) and web (mapDomEvents) each emit
//! IRAction JSON for the same canonical operations against a shared element id
//! `field`. If the recorder is truly cross-platform, the three streams generate
//! a BYTE-IDENTICAL flow — the same maestro yaml and the same rust source —
//! because they converged on the same IRAction, not merely "something similar".
//! The per-leg capture timestamp differs and must not leak into the flow.
//!
//! The per-leg JSON here is the exact shape each leg's own contract lock pins
//! (android_iraction_contract.rs / web_iraction_contract.rs) and iOS's
//! RecordingApp records; drift on any leg fails its contract first, then this.

use smix_authoring_ir::IRAction;
use smix_recorder::{generate_maestro_yaml, generate_rust};

// [tap field, fill field "smix", clear field] — the canonical op set, one per
// leg, differing only in timestampMs (capture metadata, not flow content).
fn ios() -> &'static str {
    concat!(
        r#"[{"kind":"tap","selector":{"id":"field"},"timestampMs":10},"#,
        r#"{"kind":"fill","selector":{"id":"field"},"text":"smix","timestampMs":11},"#,
        r#"{"kind":"clear","selector":{"id":"field"},"timestampMs":12}]"#
    )
}
fn android() -> &'static str {
    concat!(
        r#"[{"kind":"tap","selector":{"id":"field"},"timestampMs":20},"#,
        r#"{"kind":"fill","selector":{"id":"field"},"text":"smix","timestampMs":21},"#,
        r#"{"kind":"clear","selector":{"id":"field"},"timestampMs":22}]"#
    )
}
fn web() -> &'static str {
    concat!(
        r#"[{"kind":"tap","selector":{"id":"field"},"timestampMs":30},"#,
        r#"{"kind":"fill","selector":{"id":"field"},"text":"smix","timestampMs":31},"#,
        r#"{"kind":"clear","selector":{"id":"field"},"timestampMs":32}]"#
    )
}

fn parse(s: &str) -> Vec<IRAction> {
    serde_json::from_str(s).expect("leg IRAction JSON deserializes")
}

#[test]
fn the_three_legs_generate_byte_identical_maestro() {
    let i = generate_maestro_yaml(&parse(ios()), "com.x").unwrap();
    let a = generate_maestro_yaml(&parse(android()), "com.x").unwrap();
    let w = generate_maestro_yaml(&parse(web()), "com.x").unwrap();
    assert_eq!(i, a, "iOS vs Android maestro must be identical");
    assert_eq!(a, w, "Android vs web maestro must be identical");
}

#[test]
fn the_three_legs_generate_byte_identical_rust() {
    let i = generate_rust(&parse(ios()), "recorded", "com.x").unwrap();
    let a = generate_rust(&parse(android()), "recorded", "com.x").unwrap();
    let w = generate_rust(&parse(web()), "recorded", "com.x").unwrap();
    assert_eq!(i, a, "iOS vs Android rust must be identical");
    assert_eq!(a, w, "Android vs web rust must be identical");
}

//! How an agent names an element over MCP.
//!
//! The schema an agent reads is the only documentation it gets, so the shape
//! mirrors the yaml short form: flat optional fields, exactly one of them set.
//! Zero or several is an error rather than a guess — an agent that meant `id`
//! and typed `text` should be told, not quietly obeyed.

use smix_mcp::SelectorParams;
use smix_selector::{Pattern, Selector};

fn params(json: &str) -> SelectorParams {
    serde_json::from_str(json).expect("parse params")
}

#[test]
fn an_id_is_the_robust_path_and_mcp_can_finally_take_one() {
    // The gap this step exists to close: MCP could only tap by text, while
    // 03-selectors.md calls ids the most robust selector.
    let s = params(r#"{"id": "form-submit-btn"}"#).to_selector().unwrap();
    match s {
        Selector::Id { id, .. } => assert_eq!(id, "form-submit-btn"),
        other => panic!("expected Id, got {other:?}"),
    }
}

#[test]
fn text_stays_available() {
    let s = params(r#"{"text": "Submit"}"#).to_selector().unwrap();
    match s {
        Selector::Text { text, .. } => assert_eq!(text, Pattern::Text("Submit".into())),
        other => panic!("expected Text, got {other:?}"),
    }
}

#[test]
fn a_label_and_a_role_are_reachable_too() {
    assert!(matches!(
        params(r#"{"label": "Close"}"#).to_selector().unwrap(),
        Selector::Label { .. }
    ));
    assert!(matches!(
        params(r#"{"role": "button"}"#).to_selector().unwrap(),
        Selector::Role { .. }
    ));
}

#[test]
fn a_role_name_narrows_the_role() {
    let s = params(r#"{"role": "button", "name": "Reload"}"#)
        .to_selector()
        .unwrap();
    match s {
        Selector::Role { name, .. } => {
            assert_eq!(name, Some(Pattern::Text("Reload".into())))
        }
        other => panic!("expected Role with a name, got {other:?}"),
    }
}

#[test]
fn ocr_text_reaches_the_second_pair_of_eyes() {
    // The sense tier that has no a11y node behind it.
    let s = params(r#"{"ocrText": "Submit"}"#).to_selector().unwrap();
    match s {
        Selector::OcrText { ocr_text, .. } => assert_eq!(ocr_text, "Submit"),
        other => panic!("expected OcrText, got {other:?}"),
    }
}

#[test]
fn naming_nothing_says_what_to_name() {
    let err = params("{}").to_selector().unwrap_err();
    let msg = format!("{err}");
    // An agent that gets "invalid params" and no field list has to guess.
    for field in ["id", "text", "label", "role", "ocrText"] {
        assert!(
            msg.contains(field),
            "the error must list what it accepts; `{field}` missing from: {msg}"
        );
    }
}

#[test]
fn naming_two_things_is_an_error_not_a_silent_pick() {
    // Obeying one of them would be a coin flip the agent never sees.
    let err = params(r#"{"id": "a", "text": "b"}"#)
        .to_selector()
        .unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("id") && msg.contains("text"),
        "the error must name the fields that clashed: {msg}"
    );
}

#[test]
fn a_name_without_a_role_is_an_error() {
    // `name` narrows `role`; alone it means nothing, and silently ignoring it
    // would find the wrong element.
    let err = params(r#"{"name": "Reload"}"#).to_selector().unwrap_err();
    assert!(format!("{err}").contains("role"), "got: {err}");
}

#[test]
fn an_unknown_role_lists_the_known_ones() {
    let err = params(r#"{"role": "widget"}"#).to_selector().unwrap_err();
    let msg = format!("{err}");
    assert!(msg.contains("widget"), "echo what was given: {msg}");
    assert!(msg.contains("button"), "list what is accepted: {msg}");
}

#[test]
fn the_wire_spelling_is_the_yaml_spelling() {
    // Caught during C4: the field is ocr_text in Rust and the agent writes
    // ocrText, as the yaml docs teach. Without the rename it deserialized to
    // None and the call came back "you named nothing" — a spelling mismatch
    // wearing the costume of a usage error.
    assert!(params(r#"{"ocrText": "Submit"}"#).to_selector().is_ok());
}

#[test]
fn a_misspelled_field_is_rejected_rather_than_ignored() {
    // Without deny_unknown_fields, `{"identifier": "x"}` parses to an empty
    // struct and reports "you named nothing" — true, but it buries the typo.
    let err = serde_json::from_str::<SelectorParams>(r#"{"identifier": "x"}"#)
        .expect_err("an unknown field must not be silently dropped");
    assert!(
        format!("{err}").contains("identifier"),
        "the error must name the field it did not recognize: {err}"
    );
}

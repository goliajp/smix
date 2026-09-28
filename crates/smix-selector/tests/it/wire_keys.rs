//! The multiword selector wire keys are camelCase, like every other key
//! on the wire. `localized_text` / `ocr_text` were the two snake_case
//! outliers, and all three non-Rust SDKs (Swift, Kotlin, TS) plus the
//! yaml surface already said `localizedText` / `ocrText` — so every
//! LocalizedText selector any SDK sent failed the untagged
//! deserialization and every verb using it died with
//! InvalidSelectorJson. The old spellings stay accepted as aliases.

use smix_selector::Selector;

#[test]
fn localized_text_wire_key_is_camel_case_both_ways() {
    let json = r#"{"localizedText":{"en":"Submit","ja":"送信"}}"#;
    let sel: Selector = serde_json::from_str(json).expect("camelCase parses");
    let emitted = serde_json::to_string(&sel).expect("serializes");
    assert!(
        emitted.contains("\"localizedText\""),
        "emit drifted: {emitted}"
    );
    assert!(
        !emitted.contains("localized_text"),
        "emit drifted: {emitted}"
    );
}

#[test]
fn ocr_text_wire_key_is_camel_case_both_ways() {
    let json = r#"{"ocrText":"Submit","locales":["en"]}"#;
    let sel: Selector = serde_json::from_str(json).expect("camelCase parses");
    let emitted = serde_json::to_string(&sel).expect("serializes");
    assert!(emitted.contains("\"ocrText\""), "emit drifted: {emitted}");
}

#[test]
fn legacy_snake_case_spellings_still_parse() {
    let _: Selector = serde_json::from_str(r#"{"localized_text":{"en":"Go"}}"#)
        .expect("legacy localized_text still accepted");
    let _: Selector =
        serde_json::from_str(r#"{"ocr_text":"Go"}"#).expect("legacy ocr_text still accepted");
}

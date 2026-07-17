//! The schema is the documentation.
//!
//! An agent never reads this repo. What it gets is the JSON schema emitted
//! from these structs' doc comments, so a missing or careless description is
//! a missing manual page — the reason a stray CJK fragment in this crate's
//! schema was worth a release to remove.

use schemars::schema_for;
use smix_mcp::SelectorParams;

fn schema_json<T: schemars::JsonSchema>() -> serde_json::Value {
    serde_json::to_value(schema_for!(T)).unwrap()
}

/// Pull every `description` string out of a schema, at any depth.
fn descriptions(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::Object(map) => {
            if let Some(serde_json::Value::String(d)) = map.get("description") {
                out.push(d.clone());
            }
            for val in map.values() {
                descriptions(val, out);
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                descriptions(item, out);
            }
        }
        _ => {}
    }
}

#[test]
fn every_selector_field_tells_the_agent_what_it_is_for() {
    let schema = schema_json::<SelectorParams>();
    let props = schema
        .get("properties")
        .and_then(|p| p.as_object())
        .expect("SelectorParams has properties");

    for field in ["id", "text", "label", "role", "name", "ocrText"] {
        let p = props
            .get(field)
            .unwrap_or_else(|| panic!("`{field}` missing from the schema"));
        let desc = p
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or_default();
        assert!(
            !desc.trim().is_empty(),
            "`{field}` has no description; the agent would be guessing"
        );
    }
}

#[test]
fn the_schema_steers_toward_ids() {
    // An agent picks a field by reading these. If nothing says ids are the
    // robust choice, it will reach for text — which breaks on copy edits and
    // in other locales.
    let schema = schema_json::<SelectorParams>();
    let mut all = Vec::new();
    descriptions(&schema, &mut all);
    let joined = all.join(" ").to_lowercase();
    assert!(
        joined.contains("prefer id") || joined.contains("most robust"),
        "the schema should say ids are the robust path; got: {joined}"
    );
}

#[test]
fn the_schema_carries_no_cjk_or_development_noise() {
    // A CJK fragment shipped in this crate's schema once and reached every
    // agent that connected, because this text is the product surface.
    //
    // Typography is fine — an em dash is English. What must not appear is
    // another language, or the shorthand this team writes for itself.
    let schema = schema_json::<SelectorParams>();
    let mut all = Vec::new();
    descriptions(&schema, &mut all);
    for d in &all {
        assert!(
            !d.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
            "CJK in the schema an agent reads: {d}"
        );
        for noise in ["TODO", "FIXME", "insight", "plan-hot", "plan-cold"] {
            assert!(
                !d.contains(noise),
                "development noise `{noise}` in the schema an agent reads: {d}"
            );
        }
    }
}

#[test]
fn the_agent_facing_spelling_is_camel_case() {
    // Matching the yaml docs an agent may have read. `ocr_text` would be a
    // second spelling for the same idea.
    let schema = schema_json::<SelectorParams>();
    let props = schema.get("properties").and_then(|p| p.as_object()).unwrap();
    assert!(props.contains_key("ocrText"), "expected ocrText");
    assert!(!props.contains_key("ocr_text"), "snake_case leaked into the schema");
}

//! `smix migrate` must flag selector keys the parser will refuse.
//!
//! v2 refuses unknown keys inside a selector mapping — that refusal is
//! what stops the next silently-dropped modifier. But migrate is what a
//! user runs BEFORE discovering that, and it emitted `enabled: true`
//! (documented for a filter smix never implemented) verbatim, exited 0,
//! and left them a flow that fails at parse time. A migration tool that
//! reports success and hands back something that cannot run is worse
//! than one that refuses.

use smix_migrate::Migrator;

#[test]
fn an_unimplemented_selector_key_is_reported() {
    let yaml =
        "appId: com.example.app\n---\n- assertVisible:\n    text: \"Submit\"\n    enabled: true\n";
    let (_out, report) = Migrator::default().migrate(yaml).expect("migrates");
    assert!(
        report.unknown_selector_keys.iter().any(|k| k == "enabled"),
        "migrate must name `enabled`, which v2 refuses: {:?}",
        report.unknown_selector_keys
    );
}

#[test]
fn a_real_modifier_is_not_reported() {
    let yaml = "appId: com.example.app\n---\n- tapOn:\n    text: \"Edit\"\n    below: { text: \"Settings\" }\n";
    let (_out, report) = Migrator::default().migrate(yaml).expect("migrates");
    assert!(
        report.unknown_selector_keys.is_empty(),
        "below: is a real modifier and must not be flagged: {:?}",
        report.unknown_selector_keys
    );
}

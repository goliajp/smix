//! v2 C1 — VERB_TABLE single-source-of-truth gate.
//!
//! The `smix-verbs` crate header declares the invariant "any new yaml verb
//! MUST land in VERB_TABLE first", but nothing enforced it — the parser had
//! drifted (clearAppData / resetAppData / clearUserDefaults / expectLogClean
//! dispatched without a VerbEntry). This test is the enforcement: every verb
//! `parse_step` dispatches must be in `smix_verbs::VERB_TABLE`, except the
//! deliberately-excluded script verbs.
//!
//! `ACCEPTED` mirrors the `parse_step` match arms in `src/parser.rs`. When a
//! verb is added to the dispatch, add it here (and to VERB_TABLE) — this test
//! makes forgetting the table entry a hard failure.
//!
//! Scope: membership only. It does not check `arg_shape`, and deliberately —
//! that field holds one value while plenty of verbs accept several yaml
//! shapes, so it names the primary one rather than stating a contract. The
//! parser is the authority on what parses.

/// Every top-level verb the parser accepts (canonical, post-normalize),
/// mirroring the `parse_step` dispatch. Excludes the two script verbs.
const ACCEPTED: &[&str] = &[
    "tapOn",
    "waitForAnimationToEnd",
    "extendedWaitUntil",
    "assertVisible",
    "inputText",
    "pressKey",
    "runFlow",
    "scrollUntilVisible",
    "eraseText",
    "swipe",
    "launchApp",
    "openLink",
    "stopApp",
    "clearAppData",
    "resetAppData",
    "clearUserDefaults",
    "scroll",
    "hideKeyboard",
    "assertNotVisible",
    "killApp",
    "clearState",
    "clearKeychain",
    "takeScreenshot",
    "setClipboard",
    "pasteText",
    "copyTextFrom",
    "doubleTapOn",
    "longPressOn",
    "assertTrue",
    "repeat",
    "retry",
    "webview_eval",
    "webviewEval",
    "setLocation",
    "travel",
    "setPermissions",
    "addMedia",
    "setOrientation",
    "startRecording",
    "stopRecording",
    "assertScreenshot",
    "assertCondition",
    "assertWithAI",
    "extractWithAI",
    "extractTextWithAI",
    "expect",
    "expectLogClean",
    "fixture",
];

/// Deliberately absent from VERB_TABLE: `smix-migrate` warns corpus
/// maintainers that porting requires manual review (script bodies carry
/// maestro-specific APIs). See the tail note in `smix-verbs`.
const EXCLUDED: &[&str] = &["runScript", "evalScript"];

#[test]
fn parser_dispatch_verbs_are_in_verb_table() {
    let missing: Vec<&str> = ACCEPTED
        .iter()
        .copied()
        .filter(|v| !smix_verbs::is_known_verb(v))
        .collect();
    assert!(
        missing.is_empty(),
        "parser dispatches these verbs but they are absent from VERB_TABLE: {missing:?}"
    );
}

#[test]
fn excluded_script_verbs_stay_absent() {
    for v in EXCLUDED {
        assert!(
            !smix_verbs::is_known_verb(v),
            "{v} is marked deliberately-excluded but appears in VERB_TABLE"
        );
    }
}

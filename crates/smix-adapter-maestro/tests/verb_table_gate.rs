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
use smix_verbs::VERB_TABLE;

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

/// In VERB_TABLE, but the parser has no dispatch for them — so a flow that
/// writes one gets `UnsupportedCommand` from a verb the table advertises.
///
/// This list exists to be emptied, and eleven had accumulated behind a
/// membership check that only ran one way. Ten are gone: they were identity
/// rows, claiming a verb whose maestro name and smix name were the same, and
/// each named something that is not a verb — `ocrText` is a selector field,
/// `tapAtCoord` is `tapOn: {point}`, `toggleAirplaneMode` was never
/// implemented anywhere. Deleting them made `doubleTap` and `longPress`
/// start working: an identity row shadows the alias in
/// `normalize_verb_name`'s maestro-first lookup, so the name never reached
/// the branch that would have mapped it to `doubleTapOn`.
///
/// `back` is the real remainder. The table maps maestro's `back` onto smix's
/// `pressKey`, but the codemod drops the argument on the way, emitting a
/// bare `pressKey` rather than `pressKey: back`.
const TABLE_ROWS_THE_PARSER_LACKS: &[&str] = &["back"];

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
fn verb_table_rows_reach_the_parser() {
    // The other direction. A row the parser never learned is a verb the table
    // advertises and a flow cannot use.
    let unreachable: Vec<&str> = smix_verbs::VERB_TABLE
        .iter()
        .map(|e| e.maestro_name)
        .filter(|m| {
            !ACCEPTED.contains(m)
                && !EXCLUDED.contains(m)
                && !TABLE_ROWS_THE_PARSER_LACKS.contains(m)
        })
        .collect();
    assert!(
        unreachable.is_empty(),
        "VERB_TABLE advertises these but the parser has no dispatch, and they are not \
         on the known-gap list: {unreachable:?}"
    );
}

#[test]
fn the_known_gap_list_does_not_outlive_the_gaps() {
    // A gap closes two ways: wire the parser up, or drop the row that
    // promised the verb. This missed the second — ten rows were deleted and
    // it still passed, leaving the list describing rows that no longer
    // exist. Both count now, so the list cannot outlive the debt either way.
    let wired: Vec<&str> = TABLE_ROWS_THE_PARSER_LACKS
        .iter()
        .copied()
        .filter(|v| ACCEPTED.contains(v))
        .collect();
    assert!(
        wired.is_empty(),
        "the parser handles these now — take them off TABLE_ROWS_THE_PARSER_LACKS: {wired:?}"
    );

    let gone: Vec<&str> = TABLE_ROWS_THE_PARSER_LACKS
        .iter()
        .copied()
        .filter(|v| !VERB_TABLE.iter().any(|e| e.maestro_name == *v))
        .collect();
    assert!(
        gone.is_empty(),
        "VERB_TABLE no longer promises these, so there is no gap to record — \
         take them off TABLE_ROWS_THE_PARSER_LACKS: {gone:?}"
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

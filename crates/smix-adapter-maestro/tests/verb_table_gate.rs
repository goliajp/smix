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

/// In VERB_TABLE, but the parser has no dispatch for them — so a flow that
/// writes one gets `UnsupportedCommand` from a verb the table advertises.
///
/// This list exists to be emptied. It is here rather than absent because the
/// membership check only ran one way: it caught a verb the parser accepted
/// and the table forgot, and never a verb the table promised and the parser
/// never learned. Eleven had accumulated behind that blind spot.
///
/// Each is one of two things, and both need a decision rather than silence:
/// a native form reachable another way (`tapAtCoord` is `tapOn: {point}`,
/// `ocrText` is a selector field), in which case the row's maestro name is
/// misleading and should be corrected; or a genuine gap (`toggleAirplaneMode`
/// has no Step variant at all), in which case it wants wiring or removal.
const TABLE_ROWS_THE_PARSER_LACKS: &[&str] = &[
    "anchorRelative",
    "back",
    "doubleTap",
    "findTextByOcr",
    "longPress",
    "ocrText",
    "swipeAtCoord",
    "tapAtCoord",
    "tapByCoord",
    "tapById",
    "toggleAirplaneMode",
];

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
    // Wire one up and this fails, so the list shrinks when the debt is paid
    // instead of quietly describing a problem that no longer exists — the
    // failure mode that has cost this cycle four retractions already.
    let stale: Vec<&str> = TABLE_ROWS_THE_PARSER_LACKS
        .iter()
        .copied()
        .filter(|v| ACCEPTED.contains(v))
        .collect();
    assert!(
        stale.is_empty(),
        "the parser handles these now — take them off TABLE_ROWS_THE_PARSER_LACKS: {stale:?}"
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

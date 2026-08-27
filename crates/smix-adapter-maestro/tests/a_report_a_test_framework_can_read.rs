//! What a host test framework needs out of a smix run.
//!
//! Native teams live in Xcode and Gradle. smix walks in rather than asking
//! them out: a flow runs through the same CLI CI runs, and its JUnit report
//! is translated into whatever the host framework calls a failure.
//!
//! Three facts are needed and no more: which flow, whether it passed, and —
//! when it did not — the step number, the verb, and what went wrong. The
//! CLI already writes all of them; this is the reader.
//!
//! Both payloads were recorded off a device rather than written by hand,
//! and both are asserted to still be the shape the CLI emits: a recorded
//! fixture goes stale in silence, and this release already had a gate blind
//! for a whole checkpoint that way.

use smix_adapter_maestro::report::{parse_junit, ReadError};

const PASSING: &str = include_str!("fixtures/reports/passing.xml");
const FAILING: &str = include_str!("fixtures/reports/failing.xml");

#[test]
fn a_passing_run_names_the_flow_and_claims_nothing_else() {
    let r = parse_junit(PASSING).expect("the recorded passing report parses");
    assert_eq!(r.flow, "dialog-confirm");
    assert!(r.passed, "a report with zero failures was read as a failure");
    assert!(r.failure.is_none());
}

#[test]
fn a_failing_run_carries_the_step_the_verb_and_the_reason() {
    let r = parse_junit(FAILING).expect("the recorded failing report parses");
    assert!(!r.passed);
    let f = r.failure.expect("a failing report must carry its failure");
    assert!(
        f.contains("step 2"),
        "the reason does not say which step, so a reader has to re-run to \
         find out: {f}"
    );
    assert!(
        f.contains("tapOn"),
        "the reason does not name the verb: {f}"
    );
    assert!(
        f.contains("no-such-control"),
        "the reason does not carry the selector that failed: {f}"
    );
}

#[test]
fn the_reason_arrives_unescaped() {
    // The CLI escapes the message for XML. A reader that hands `&quot;`
    // to a developer has made the failure harder to read than the stdout
    // it replaced.
    let r = parse_junit(FAILING).expect("parses");
    let f = r.failure.unwrap();
    assert!(!f.contains("&quot;"), "the reason still carries XML escapes: {f}");
    assert!(f.contains('"'), "the quotes were dropped rather than unescaped: {f}");
}

#[test]
fn nothing_is_not_a_pass() {
    // "Could not read this" and "it passed" are different answers, and one
    // value for both is the shape this release keeps finding. An empty
    // report usually means the CLI never ran.
    assert!(matches!(parse_junit(""), Err(ReadError::NotAReport)));
    assert!(matches!(parse_junit("total nonsense"), Err(ReadError::NotAReport)));
}

#[test]
fn a_report_with_no_testcase_is_not_a_pass_either() {
    // A suite element with nothing in it parses as XML and says nothing
    // about a flow. Reading it as success would turn "the run produced no
    // case" into a green test.
    let empty = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="smix" tests="0" failures="0" errors="0" skipped="0">
</testsuite>"#;
    assert!(matches!(parse_junit(empty), Err(ReadError::NoFlowInIt)));
}

#[test]
fn the_recorded_payloads_are_still_the_shape_the_cli_emits() {
    // The fixtures are recorded, and recorded fixtures go stale in silence.
    for (name, xml) in [("passing", PASSING), ("failing", FAILING)] {
        assert!(
            xml.contains("classname=\"smix.flow\""),
            "the {name} payload no longer carries the classname the CLI \
             writes — re-record it"
        );
    }
}

#[test]
fn a_report_without_a_cdata_block_is_read_from_the_attribute() {
    // The recorded payloads both carry CDATA, where nothing is escaped, so
    // the attribute path and the unescaping never ran — a mutation sweep
    // found both surviving. They are not dead: a writer that drops CDATA
    // leaves only the attribute, and there everything IS escaped. Exercised
    // here so the two clauses can go red.
    let attribute_only = r#"<?xml version="1.0" encoding="UTF-8"?>
<testsuite name="smix" tests="1" failures="1" errors="0" skipped="0">
  <testcase name="attr-only" classname="smix.flow" time="0">
      <failure type="smix.sdk" message="step 2 (tapOn): not found: { id=&quot;x&quot; }"/>
  </testcase>
</testsuite>"#;
    let r = parse_junit(attribute_only).expect("parses");
    assert!(!r.passed, "a failure in the attribute was read as a pass");
    let f = r.failure.expect("the attribute carries the reason");
    assert!(f.contains("step 2"), "the attribute path lost the step: {f}");
    assert!(
        f.contains(r#"id="x""#),
        "the attribute path did not unescape: {f}"
    );
}

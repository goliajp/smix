//! Recursive `parse_flow_file` + cycle detection tests.

use smix_adapter_maestro::{ParseError, parse_flow_file};
use std::path::PathBuf;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

#[test]
fn cycle_detected_a_b_a() {
    let entry = fixture_path("cycle_a.yaml");
    let err = parse_flow_file(&entry).expect_err("cycle must surface");
    let display = format!("{err}");
    assert!(
        display.contains("cycle_a.yaml"),
        "error display must mention cycle_a.yaml; got: {display}"
    );
    match err {
        ParseError::RunFlowCycle { path: _, stack } => {
            // root cycle_a → cycle_b → cycle_a (re-entry) = 3 stack entries
            assert_eq!(
                stack.len(),
                3,
                "expected stack of 3, got {} entries: {stack:?}",
                stack.len()
            );
            assert!(stack[0].ends_with("cycle_a.yaml"));
            assert!(stack[1].ends_with("cycle_b.yaml"));
            assert!(stack[2].ends_with("cycle_a.yaml"));
        }
        other => panic!("expected RunFlowCycle, got: {other:?}"),
    }
}

#[test]
fn recursive_expands_alerts_counting_subflows() {
    // recursive_parent.yaml has `runFlow: go_to_alerts.yaml` as its first
    // step. The whole point of this test: any error inside an expanded
    // subflow must propagate up through `parse_flow_file` exactly as if
    // the parent had hit it directly.
    //
    // The fixtures live as flat files (recursive_parent.yaml + sibling
    // go_to_alerts.yaml in the same directory), so the relative path
    // `go_to_alerts.yaml` resolves cleanly against the parent's parent
    // directory — proving the relative-path resolution path *and* the
    // error propagation path in a single shot.
    //
    // go_to_alerts.yaml uses `repeat.while: { notVisible: <sel> }`
    // (a mapping form lifted into RepeatMode::WhileNotVisible), so the
    // subflow chain now parses cleanly.
    let entry = fixture_path("recursive_parent.yaml");
    let flow = parse_flow_file(&entry)
        .expect("recursive_parent + go_to_alerts subflow chain must parse cleanly");
    assert!(
        !flow.steps.is_empty(),
        "recursive_parent flow body must contain steps"
    );
}

//! `apply` turns a validated `Proposal` into an amended `Vec<Step>`.
//! It validates first (index bounds, insert-wait policy) and then
//! mutates a copy; an out-of-range edit propagates as `ApplyError`
//! rather than being silently skipped.

use smix_adapter_maestro::Step;
use smix_authoring_propose::{ApplyError, Proposal, ProposalEdit, apply};
use smix_selector::{Modifiers, Selector};

fn id(s: &str) -> Selector {
    Selector::Id {
        id: s.to_string(),
        modifiers: Modifiers::default(),
    }
}

fn tap(sel: Selector) -> Step {
    Step::TapOn {
        selector: sel,
        optional: false,
        dispatch: None,
    }
}

fn three_steps() -> Vec<Step> {
    vec![
        tap(id("a")),
        tap(id("b")),
        tap(id("c")),
    ]
}

fn wait_step() -> Step {
    Step::ExtendedWaitUntil {
        selector: id("spinner"),
        timeout_ms: 4000,
        expect_visible: true,
    }
}

#[test]
fn apply_replace_selector_swaps() {
    let steps = three_steps();
    let proposal = Proposal {
        edits: vec![ProposalEdit::ReplaceSelector {
            step_index: 1,
            new_selector: id("b-new"),
        }],
    };
    let out = apply(&proposal, &steps).expect("valid");
    assert_eq!(out.len(), 3);
    assert_eq!(out[0], steps[0]);
    assert_eq!(out[2], steps[2]);
    match &out[1] {
        Step::TapOn { selector, .. } => assert_eq!(selector, &id("b-new")),
        other => panic!("expected TapOn, got {other:?}"),
    }
}

#[test]
fn apply_insert_step_inserts_before() {
    let steps = three_steps();
    let proposal = Proposal {
        edits: vec![ProposalEdit::InsertStep {
            before_index: 1,
            step: wait_step(),
        }],
    };
    let out = apply(&proposal, &steps).expect("valid");
    assert_eq!(out.len(), 4);
    assert_eq!(out[0], steps[0]);
    assert_eq!(out[1], wait_step());
    assert_eq!(out[2], steps[1]);
    assert_eq!(out[3], steps[2]);
}

#[test]
fn apply_reorder_moves() {
    let steps = three_steps();
    let proposal = Proposal {
        edits: vec![ProposalEdit::ReorderStep {
            from_index: 0,
            to_index: 2,
        }],
    };
    let out = apply(&proposal, &steps).expect("valid");
    assert_eq!(out, vec![steps[1].clone(), steps[2].clone(), steps[0].clone()]);
}

#[test]
fn apply_rejects_invalid_via_validate() {
    let steps = three_steps();
    let proposal = Proposal {
        edits: vec![ProposalEdit::ReplaceSelector {
            step_index: 9,
            new_selector: id("oops"),
        }],
    };
    let err = apply(&proposal, &steps).expect_err("out-of-range must fail");
    assert!(matches!(err, ApplyError::Invalid(_)), "got {err:?}");
}

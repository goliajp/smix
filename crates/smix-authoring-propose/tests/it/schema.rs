//! Proposal schema contract: the four edit ops deserialize onto real
//! `Step` / `Selector` variants, and the v1 convergence policy in
//! `validate` rejects out-of-range indices and non-wait insertions while
//! accepting the five v1 edit classes.

use smix_adapter_maestro::Step;
use smix_authoring_propose::{Proposal, ProposalEdit};
use smix_selector::Selector;

#[test]
fn proposal_deserializes_four_edit_ops() {
    let json = r#"{
        "edits": [
            { "op": "replaceSelector", "step_index": 0, "new_selector": { "id": "submit-btn" } },
            { "op": "insertStep", "before_index": 1, "step": { "extendedWaitUntil": { "selector": { "id": "spinner" }, "timeout_ms": 5000, "expect_visible": false } } },
            { "op": "reorderStep", "from_index": 2, "to_index": 0 },
            { "op": "replaceStep", "step_index": 1, "new_step": { "tapOn": { "selector": { "text": "Login" } } } }
        ]
    }"#;

    let proposal: Proposal = serde_json::from_str(json).expect("fixture proposal deserializes");
    assert_eq!(proposal.edits.len(), 4);

    match &proposal.edits[0] {
        ProposalEdit::ReplaceSelector {
            step_index,
            new_selector,
        } => {
            assert_eq!(*step_index, 0);
            assert!(matches!(new_selector, Selector::Id { id, .. } if id == "submit-btn"));
        }
        other => panic!("edit 0 should be ReplaceSelector, got {other:?}"),
    }

    match &proposal.edits[1] {
        ProposalEdit::InsertStep { before_index, step } => {
            assert_eq!(*before_index, 1);
            assert!(matches!(step, Step::ExtendedWaitUntil { .. }));
        }
        other => panic!("edit 1 should be InsertStep, got {other:?}"),
    }

    match &proposal.edits[2] {
        ProposalEdit::ReorderStep {
            from_index,
            to_index,
        } => {
            assert_eq!(*from_index, 2);
            assert_eq!(*to_index, 0);
        }
        other => panic!("edit 2 should be ReorderStep, got {other:?}"),
    }

    match &proposal.edits[3] {
        ProposalEdit::ReplaceStep {
            step_index,
            new_step,
        } => {
            assert_eq!(*step_index, 1);
            assert!(matches!(new_step, Step::TapOn { .. }));
        }
        other => panic!("edit 3 should be ReplaceStep, got {other:?}"),
    }
}

#[test]
fn validate_rejects_out_of_range_index() {
    let json = r#"{ "edits": [ { "op": "replaceSelector", "step_index": 9, "new_selector": { "id": "x" } } ] }"#;
    let proposal: Proposal = serde_json::from_str(json).unwrap();
    assert!(proposal.validate(3).is_err());
}

#[test]
fn validate_rejects_insertstep_non_wait() {
    let json = r#"{ "edits": [ { "op": "insertStep", "before_index": 0, "step": { "tapOn": { "selector": { "id": "x" } } } } ] }"#;
    let proposal: Proposal = serde_json::from_str(json).unwrap();
    assert!(proposal.validate(3).is_err());
}

#[test]
fn validate_accepts_v1_classes() {
    let json = r#"{
        "edits": [
            { "op": "replaceSelector", "step_index": 0, "new_selector": { "id": "submit-btn" } },
            { "op": "insertStep", "before_index": 1, "step": { "extendedWaitUntil": { "selector": { "id": "spinner" }, "timeout_ms": 5000, "expect_visible": false } } },
            { "op": "insertStep", "before_index": 2, "step": { "waitForAnimationToEnd": { "ceiling_ms": 400 } } },
            { "op": "replaceStep", "step_index": 1, "new_step": { "extendedWaitUntil": { "selector": { "id": "done" }, "timeout_ms": 3000, "expect_visible": true } } },
            { "op": "replaceStep", "step_index": 2, "new_step": { "assertVisible": { "selector": { "text": "Welcome" } } } },
            { "op": "reorderStep", "from_index": 2, "to_index": 0 }
        ]
    }"#;
    let proposal: Proposal = serde_json::from_str(json).unwrap();
    assert_eq!(proposal.edits.len(), 6);
    assert!(proposal.validate(3).is_ok());
}

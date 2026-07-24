//! C3 headline: a proposal applied to a core-set flow yields an amended
//! flow that emits to legal maestro yaml which `parse_flow_yaml` accepts
//! AND parses back step-for-step equal. Device-free: apply → emit → parse,
//! no claude, no sim.

use smix_adapter_maestro::{Step, emit_flow_yaml, parse_flow_yaml};
use smix_authoring_propose::{Proposal, ProposalEdit, apply};
use smix_selector::{Modifiers, Pattern, Selector};

const APP_ID: &str = "com.x";

fn id(s: &str) -> Selector {
    Selector::Id {
        id: s.to_string(),
        modifiers: Modifiers::default(),
    }
}

fn text(s: &str) -> Selector {
    Selector::Text {
        text: Pattern::Text(s.to_string()),
        modifiers: Modifiers::default(),
    }
}

fn fixture_flow() -> Vec<Step> {
    vec![
        Step::LaunchApp {
            app_id: APP_ID.to_string(),
            clear_state: false,
            clear_keychain: false,
            permissions: Vec::new(),
            arguments: Vec::new(),
            stop_app: true,
            wait_for_interactive_ms: None,
        },
        Step::TapOn {
            selector: id("old-btn"),
            optional: false,
            dispatch: None,
        },
        Step::InputTextInto {
            selector: id("email"),
            text: "x".to_string(),
        },
        Step::AssertVisible {
            selector: text("Done"),
        },
    ]
}

fn wait_step() -> Step {
    Step::ExtendedWaitUntil {
        selector: id("spinner"),
        timeout_ms: 4000,
        expect_visible: false,
    }
}

fn round_trips(amended: &[Step]) {
    let yaml = emit_flow_yaml(amended, APP_ID).expect("amended flow emits");
    let flow =
        parse_flow_yaml(&yaml).unwrap_or_else(|e| panic!("emitted yaml must parse: {e}\n{yaml}"));
    assert_eq!(flow.steps, amended, "amended flow must round-trip faithfully\n{yaml}");
}

#[test]
fn amended_flow_round_trips_to_legal_flow() {
    let steps = fixture_flow();
    let proposal = Proposal {
        edits: vec![
            ProposalEdit::ReplaceSelector {
                step_index: 1,
                new_selector: id("new-btn"),
            },
            ProposalEdit::InsertStep {
                before_index: 3,
                step: wait_step(),
            },
            ProposalEdit::ReorderStep {
                from_index: 0,
                to_index: 2,
            },
            ProposalEdit::ReplaceStep {
                step_index: 3,
                new_step: Step::AssertVisible { selector: id("hero") },
            },
        ],
    };
    let amended = apply(&proposal, &steps).expect("proposal applies");
    round_trips(&amended);
}

#[test]
fn wellformed_gate_holds_for_insert_and_reorder_only() {
    let steps = fixture_flow();
    let proposal = Proposal {
        edits: vec![
            ProposalEdit::InsertStep {
                before_index: 1,
                step: wait_step(),
            },
            ProposalEdit::ReorderStep {
                from_index: 0,
                to_index: 1,
            },
        ],
    };
    let amended = apply(&proposal, &steps).expect("proposal applies");
    round_trips(&amended);
}

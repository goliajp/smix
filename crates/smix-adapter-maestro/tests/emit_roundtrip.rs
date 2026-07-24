//! `emit_flow_yaml` is the inverse of `parse_flow_yaml` for the
//! round-trip core set: a `Vec<Step>` emitted to maestro yaml must parse
//! back to the exact same `Vec<Step>` (step-level equality). Variants and
//! selector shapes outside the core set are refused explicitly rather
//! than silently mis-emitted.

use smix_adapter_maestro::{EmitError, Step, emit_flow_yaml, parse_flow_yaml};
use smix_selector::{Modifiers, Pattern, Selector, True};

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

fn label(s: &str) -> Selector {
    Selector::Label {
        label: s.to_string(),
        modifiers: Modifiers::default(),
    }
}

fn launch(app_id: &str) -> Step {
    Step::LaunchApp {
        app_id: app_id.to_string(),
        clear_state: false,
        clear_keychain: false,
        permissions: Vec::new(),
        arguments: Vec::new(),
        stop_app: true,
        wait_for_interactive_ms: None,
    }
}

#[test]
fn emit_core_steps_round_trip() {
    let steps = vec![
        launch("com.x"),
        Step::TapOn {
            selector: id("submit-btn"),
            optional: false,
            dispatch: None,
        },
        Step::TapOn {
            selector: text("Login"),
            optional: false,
            dispatch: None,
        },
        Step::TapOn {
            selector: label("Settings"),
            optional: false,
            dispatch: None,
        },
        Step::InputTextInto {
            selector: id("email"),
            text: "a@b.com".to_string(),
        },
        Step::InputText("hello world".to_string()),
        Step::AssertVisible {
            selector: text("Welcome"),
        },
        Step::AssertVisible {
            selector: id("hero"),
        },
        Step::AssertNotVisible {
            selector: text("Error"),
        },
        Step::ExtendedWaitUntil {
            selector: id("spinner"),
            timeout_ms: 5000,
            expect_visible: true,
        },
        Step::ExtendedWaitUntil {
            selector: id("spinner"),
            timeout_ms: 3000,
            expect_visible: false,
        },
        Step::WaitForAnimationToEnd { ceiling_ms: 400 },
        Step::Back,
        Step::PressKey("Enter".to_string()),
        Step::EraseText(10),
        Step::Swipe {
            from: (0.5, 0.7),
            to: (0.5, 0.3),
        },
        Step::ScrollUntilVisible {
            selector: id("target"),
            direction: "down".to_string(),
        },
        Step::StopApp,
        Step::HideKeyboard,
        Step::Scroll,
    ];

    let yaml = emit_flow_yaml(&steps, "com.x").expect("core steps emit");
    let flow =
        parse_flow_yaml(&yaml).unwrap_or_else(|e| panic!("emitted yaml must parse: {e}\n{yaml}"));
    assert_eq!(flow.steps, steps, "round-trip must be faithful\n{yaml}");
}

#[test]
fn emit_refuses_out_of_core_variant() {
    let steps = vec![Step::AssertCondition {
        condition: "the banner is red".to_string(),
    }];
    let err = emit_flow_yaml(&steps, "com.x").expect_err("out-of-core variant refused");
    assert!(matches!(err, EmitError::Unsupported { .. }), "got {err:?}");
}

#[test]
fn emit_refuses_unsupported_selector() {
    let steps = vec![Step::TapOn {
        selector: Selector::Focused {
            focused: True(true),
        },
        optional: false,
        dispatch: None,
    }];
    let err = emit_flow_yaml(&steps, "com.x").expect_err("focused selector refused");
    assert!(matches!(err, EmitError::Unsupported { .. }), "got {err:?}");
}

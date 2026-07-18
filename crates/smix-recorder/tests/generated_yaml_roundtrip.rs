//! What the recorder emits has to be something the parser accepts.
//!
//! `crates/smix-recorder/tests/generators.rs` checks the emitted yaml as
//! text, one substring per verb. That let `swipeOnce:` ship: the swipe
//! assertion looked for `direction: UP`, which was present, while the key
//! above it was a verb no table has and no parser dispatches — every
//! recorded flow containing a swipe died on UnsupportedCommand.
//!
//! Modelled on `smix-adapter-maestro/tests/codemod_roundtrip.rs`,
//! including its two disciplines: a case that cannot be built is a
//! failure rather than a `continue`, and the variant list is derived from
//! the type rather than copied beside it.

use smix_authoring_ir::IRAction;
use smix_input::{KeyName, SwipeDirection};
use smix_recorder::{generate_maestro_yaml, generate_rust};
use smix_selector::{Modifiers, Pattern, Role, Selector};

/// Position of `action` in the `IRAction` variant list.
///
/// The match is exhaustive, so adding a variant to `IRAction` breaks this
/// file at compile time and the author has to supply a sample below —
/// which is the closest Rust gets to deriving the list from the type.
/// [`VARIANT_COUNT`] then makes a sample that was never added a test
/// failure rather than a silent gap.
fn variant_index(action: &IRAction) -> usize {
    match action {
        IRAction::Tap { .. } => 0,
        IRAction::Fill { .. } => 1,
        IRAction::Clear { .. } => 2,
        IRAction::PressKey { .. } => 3,
        IRAction::Swipe { .. } => 4,
        IRAction::GoBack { .. } => 5,
        IRAction::WaitFor { .. } => 6,
        IRAction::HideKeyboard { .. } => 7,
    }
}

const VARIANT_COUNT: usize = 8;

fn text_sel(t: &str) -> Selector {
    Selector::Text {
        text: Pattern::text(t),
        modifiers: Modifiers::default(),
    }
}

/// One named sample per case worth round-tripping. Several variants get
/// more than one row where the emitter branches on a field (`Swipe.from`)
/// or on the selector shape.
fn samples() -> Vec<(&'static str, IRAction)> {
    vec![
        (
            "tap/text",
            IRAction::Tap {
                selector: text_sel("Login"),
                timestamp_ms: 1.0,
            },
        ),
        (
            "tap/id",
            IRAction::Tap {
                selector: Selector::Id {
                    id: "login-btn".into(),
                    modifiers: Modifiers::default(),
                },
                timestamp_ms: 2.0,
            },
        ),
        (
            "tap/label",
            IRAction::Tap {
                selector: Selector::Label {
                    label: "Sign in".into(),
                    modifiers: Modifiers::default(),
                },
                timestamp_ms: 3.0,
            },
        ),
        (
            "tap/regex",
            IRAction::Tap {
                selector: Selector::Text {
                    text: Pattern::regex("Log ?in"),
                    modifiers: Modifiers::default(),
                },
                timestamp_ms: 4.0,
            },
        ),
        (
            "tap/ocrText",
            IRAction::Tap {
                selector: Selector::OcrText {
                    ocr_text: "Continue".into(),
                    locales: Vec::new(),
                    modifiers: Modifiers::default(),
                },
                timestamp_ms: 45.0,
            },
        ),
        (
            "tap/modifier-stacked",
            IRAction::Tap {
                selector: Selector::Text {
                    text: Pattern::text("Row"),
                    modifiers: Modifiers {
                        first: Some(true),
                        ..Modifiers::default()
                    },
                },
                timestamp_ms: 46.0,
            },
        ),
        (
            "tap/role",
            IRAction::Tap {
                selector: Selector::Role {
                    role: Role::Button,
                    name: Some(Pattern::text("Submit")),
                    modifiers: Modifiers::default(),
                },
                timestamp_ms: 5.0,
            },
        ),
        (
            "fill",
            IRAction::Fill {
                selector: text_sel("Email"),
                text: "u@x.com".into(),
                timestamp_ms: 6.0,
            },
        ),
        (
            "clear",
            IRAction::Clear {
                selector: text_sel("Email"),
                timestamp_ms: 7.0,
            },
        ),
        (
            "pressKey",
            IRAction::PressKey {
                key: KeyName::Return,
                timestamp_ms: 8.0,
            },
        ),
        (
            "swipe/no-anchor",
            IRAction::Swipe {
                direction: SwipeDirection::Up,
                from: None,
                timestamp_ms: 9.0,
            },
        ),
        (
            "swipe/anchored",
            IRAction::Swipe {
                direction: SwipeDirection::Left,
                from: Some(text_sel("Row 3")),
                timestamp_ms: 10.0,
            },
        ),
        ("goBack", IRAction::GoBack { timestamp_ms: 11.0 }),
        (
            "waitFor",
            IRAction::WaitFor {
                selector: text_sel("Dashboard"),
                timestamp_ms: 12.0,
            },
        ),
        (
            "hideKeyboard",
            IRAction::HideKeyboard { timestamp_ms: 13.0 },
        ),
    ]
}

#[test]
fn the_samples_cover_every_ir_action_variant() {
    let mut seen = [false; VARIANT_COUNT];
    for (_, action) in samples() {
        seen[variant_index(&action)] = true;
    }
    let missing: Vec<usize> = seen
        .iter()
        .enumerate()
        .filter(|(_, s)| !**s)
        .map(|(i, _)| i)
        .collect();
    assert!(
        missing.is_empty(),
        "IRAction variants with no sample — their generated output is \
         never round-tripped: variant indices {missing:?}"
    );
}

#[test]
fn every_generated_maestro_flow_parses() {
    let mut rejected = Vec::new();
    for (name, action) in samples() {
        let yaml = generate_maestro_yaml(std::slice::from_ref(&action), "com.test.roundtrip")
            .unwrap_or_else(|e| panic!("{name}: generator refused a valid action: {e}"));
        if let Err(e) = smix_adapter_maestro::parse_flow_yaml(&yaml) {
            rejected.push(format!("{name}: {e}\n{yaml}"));
        }
    }
    assert!(
        rejected.is_empty(),
        "the recorder emitted yaml the maestro parser rejects:\n{}",
        rejected.join("\n")
    );
}

/// The single-action flows above each exercise one emitter branch; a whole
/// recorded session is what a user actually replays, and step order and
/// the multi-step `Fill` expansion only show up there.
#[test]
fn a_whole_recorded_session_parses() {
    let actions: Vec<IRAction> = samples().into_iter().map(|(_, a)| a).collect();
    let yaml = generate_maestro_yaml(&actions, "com.test.roundtrip").expect("generator ran");
    smix_adapter_maestro::parse_flow_yaml(&yaml)
        .unwrap_or_else(|e| panic!("full-session flow rejected: {e}\n{yaml}"));
}

/// maestro yaml has no `focused` selector — the parser's key set does not
/// contain one and never will. The generator used to serialize it anyway
/// via its catch-all serde branch, producing a flow that died on that
/// step. Refusing at generation time is the only honest answer: there is
/// yaml to emit for every other selector shape, and none for this one.
#[test]
fn a_selector_maestro_cannot_express_is_refused_not_emitted() {
    let actions = vec![IRAction::Tap {
        selector: Selector::Focused {
            focused: smix_selector::True(true),
        },
        timestamp_ms: 1.0,
    }];
    let err = generate_maestro_yaml(&actions, "com.x")
        .expect_err("focused selector must not produce a flow");
    assert!(
        err.to_string().contains("focused"),
        "error should name the unsupported selector form: {err}"
    );
}

/// The Rust generator has no parser to round-trip against, so the check is
/// that it stopped emitting the placeholder it used to leave for anchored
/// swipes — `App::swipe_from` exists now and the emitter must call it.
#[test]
fn anchored_swipe_emits_a_real_call_not_a_comment() {
    let actions = vec![IRAction::Swipe {
        direction: SwipeDirection::Left,
        from: Some(text_sel("Row 3")),
        timestamp_ms: 1.0,
    }];
    let rs = generate_rust(&actions, "f", "com.x").expect("generator ran");
    assert!(
        rs.contains("app.swipe_from(SwipeDirection::Left, &text(\"Row 3\")).await?;"),
        "anchored swipe did not emit swipe_from:\n{rs}"
    );
    assert!(
        !rs.contains("adjust the test manually"),
        "anchored swipe still emits the pre-swipe_from placeholder:\n{rs}"
    );
}

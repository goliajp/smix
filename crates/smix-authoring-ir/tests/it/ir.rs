//! Unit tests for smix-authoring-ir.

use smix_authoring_ir::{IRAction, RecorderError, RecorderErrorReason, sort_by_timestamp};
use smix_input::{KeyName, SwipeDirection};
use smix_selector::{Modifiers, Pattern, Selector};

fn text_sel(t: &str) -> Selector {
    Selector::Text {
        text: Pattern::text(t),
        modifiers: Modifiers::default(),
    }
}

// ---- wire round-trip per variant ----------------------------------------

#[test]
fn tap_wire_round_trip_kind_tag() {
    let a = IRAction::Tap {
        selector: text_sel("Login"),
        timestamp_ms: 1234.5,
    };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains("\"kind\":\"tap\""));
    assert!(json.contains("\"timestampMs\":1234.5"));
    let parsed: IRAction = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, a);
}

#[test]
fn fill_wire_round_trip() {
    let a = IRAction::Fill {
        selector: text_sel("Email"),
        text: "user@example.com".into(),
        timestamp_ms: 1.0,
    };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains("\"kind\":\"fill\""));
    assert!(json.contains("\"text\":\"user@example.com\""));
    let parsed: IRAction = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, a);
}

#[test]
fn clear_wire_round_trip() {
    let a = IRAction::Clear {
        selector: text_sel("Email"),
        timestamp_ms: 2.0,
    };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains("\"kind\":\"clear\""));
    assert_eq!(serde_json::from_str::<IRAction>(&json).unwrap(), a);
}

#[test]
fn press_key_wire_round_trip() {
    let a = IRAction::PressKey {
        key: KeyName::Return,
        timestamp_ms: 3.0,
    };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains("\"kind\":\"pressKey\""));
    assert!(json.contains("\"key\":\"return\""));
    assert_eq!(serde_json::from_str::<IRAction>(&json).unwrap(), a);
}

#[test]
fn swipe_wire_round_trip_with_from() {
    let a = IRAction::Swipe {
        direction: SwipeDirection::Up,
        from: Some(text_sel("DragHandle")),
        timestamp_ms: 4.0,
    };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains("\"kind\":\"swipe\""));
    assert!(json.contains("\"direction\":\"up\""));
    assert!(json.contains("\"from\""));
    assert_eq!(serde_json::from_str::<IRAction>(&json).unwrap(), a);
}

#[test]
fn swipe_wire_omits_from_when_none() {
    let a = IRAction::Swipe {
        direction: SwipeDirection::Down,
        from: None,
        timestamp_ms: 5.0,
    };
    let json = serde_json::to_string(&a).unwrap();
    assert!(!json.contains("\"from\""));
    assert_eq!(serde_json::from_str::<IRAction>(&json).unwrap(), a);
}

#[test]
fn go_back_wire_round_trip() {
    let a = IRAction::GoBack { timestamp_ms: 6.0 };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains("\"kind\":\"goBack\""));
    assert_eq!(serde_json::from_str::<IRAction>(&json).unwrap(), a);
}

#[test]
fn wait_for_wire_round_trip() {
    let a = IRAction::WaitFor {
        selector: text_sel("Loaded"),
        timestamp_ms: 7.0,
    };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains("\"kind\":\"waitFor\""));
    assert_eq!(serde_json::from_str::<IRAction>(&json).unwrap(), a);
}

#[test]
fn hide_keyboard_wire_round_trip() {
    let a = IRAction::HideKeyboard { timestamp_ms: 8.0 };
    let json = serde_json::to_string(&a).unwrap();
    assert!(json.contains("\"kind\":\"hideKeyboard\""));
    assert_eq!(serde_json::from_str::<IRAction>(&json).unwrap(), a);
}

// ---- accessors ----------------------------------------------------------

#[test]
fn timestamp_ms_accessor_works_across_variants() {
    let cases: Vec<IRAction> = vec![
        IRAction::Tap {
            selector: text_sel("X"),
            timestamp_ms: 1.5,
        },
        IRAction::GoBack { timestamp_ms: 2.0 },
        IRAction::HideKeyboard { timestamp_ms: 3.25 },
        IRAction::PressKey {
            key: KeyName::Tab,
            timestamp_ms: 4.5,
        },
    ];
    let expected = [1.5, 2.0, 3.25, 4.5];
    for (a, t) in cases.iter().zip(expected.iter()) {
        assert!((a.timestamp_ms() - t).abs() < 1e-9);
    }
}

#[test]
fn kind_accessor_matches_wire_tag() {
    assert_eq!(
        IRAction::Tap {
            selector: text_sel("X"),
            timestamp_ms: 0.0,
        }
        .kind(),
        "tap"
    );
    assert_eq!(
        IRAction::PressKey {
            key: KeyName::Return,
            timestamp_ms: 0.0,
        }
        .kind(),
        "pressKey"
    );
    assert_eq!(IRAction::GoBack { timestamp_ms: 0.0 }.kind(), "goBack");
    assert_eq!(
        IRAction::HideKeyboard { timestamp_ms: 0.0 }.kind(),
        "hideKeyboard"
    );
}

// ---- sort_by_timestamp --------------------------------------------------

#[test]
fn sort_by_timestamp_orders_ascending() {
    let actions = vec![
        IRAction::GoBack { timestamp_ms: 3.0 },
        IRAction::Tap {
            selector: text_sel("X"),
            timestamp_ms: 1.0,
        },
        IRAction::HideKeyboard { timestamp_ms: 2.0 },
    ];
    let sorted = sort_by_timestamp(&actions);
    let ts: Vec<f64> = sorted.iter().map(|a| a.timestamp_ms()).collect();
    assert_eq!(ts, vec![1.0, 2.0, 3.0]);
}

#[test]
fn sort_by_timestamp_idempotent() {
    let actions = vec![
        IRAction::Tap {
            selector: text_sel("X"),
            timestamp_ms: 1.0,
        },
        IRAction::GoBack { timestamp_ms: 2.0 },
    ];
    let once = sort_by_timestamp(&actions);
    let twice = sort_by_timestamp(&once);
    assert_eq!(once, twice);
}

#[test]
fn sort_by_timestamp_handles_equal_timestamps_stable() {
    // serde_sort doesn't guarantee stable across versions, but our impl
    // uses std slice sort_by which is stable.
    let actions = vec![
        IRAction::Tap {
            selector: text_sel("A"),
            timestamp_ms: 1.0,
        },
        IRAction::Tap {
            selector: text_sel("B"),
            timestamp_ms: 1.0,
        },
    ];
    let sorted = sort_by_timestamp(&actions);
    assert_eq!(sorted, actions);
}

// ---- RecorderError ------------------------------------------------------

#[test]
fn recorder_error_reason_kebab_wire() {
    let cases = [
        (RecorderErrorReason::EmptySession, "\"empty-session\""),
        (RecorderErrorReason::MalformedAction, "\"malformed-action\""),
        (RecorderErrorReason::CleanupFailed, "\"cleanup-failed\""),
        (
            RecorderErrorReason::CleanupEmptyOutput,
            "\"cleanup-empty-output\"",
        ),
        (
            RecorderErrorReason::CleanupInvalidOutput,
            "\"cleanup-invalid-output\"",
        ),
    ];
    for (r, expected) in cases {
        let json = serde_json::to_string(&r).unwrap();
        assert_eq!(json, expected);
        let parsed: RecorderErrorReason = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, r);
    }
}

#[test]
fn recorder_error_impl_error_trait() {
    let e = RecorderError::new(RecorderErrorReason::EmptySession, "no actions captured");
    let s = format!("{}", e);
    assert!(s.contains("empty-session"));
    assert!(s.contains("no actions captured"));
    let _boxed: Box<dyn std::error::Error> = Box::new(e);
}

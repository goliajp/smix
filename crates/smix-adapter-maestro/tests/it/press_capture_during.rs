//! `captureDuring` promises a frame of the held state, so a hold too
//! short to contain one is refused where it is written rather than
//! discovered as an empty result on a device.

use smix_adapter_maestro::{Step, parse_flow_yaml};

fn flow(body: &str) -> Result<smix_adapter_maestro::Flow, String> {
    parse_flow_yaml(&format!("appId: com.example.app\n---\n{body}")).map_err(|e| e.to_string())
}

#[test]
fn a_long_enough_hold_carries_the_flag() {
    let f =
        flow("- longPressOn:\n    id: hdr-back-btn\n    duration: 1200\n    captureDuring: true\n")
            .expect("parses");
    match &f.steps[0] {
        Step::LongPressOn {
            duration_ms,
            capture_during,
            ..
        } => {
            assert_eq!(*duration_ms, 1200);
            assert!(*capture_during);
        }
        other => panic!("expected LongPressOn, got {other:?}"),
    }
}

/// The default hold is 500ms — shorter than one capture plus resolving
/// the element, so asking to capture inside it cannot be honoured.
#[test]
fn the_default_hold_is_refused_for_capture_with_the_reason() {
    let err = flow("- longPressOn:\n    id: hdr-back-btn\n    captureDuring: true\n")
        .expect_err("500ms cannot contain a capture");
    assert!(err.contains("800"), "the floor belongs in it: {err}");
    assert!(
        err.contains("230ms") && err.contains("resolved"),
        "and what eats the hold: {err}"
    );
}

#[test]
fn without_the_flag_a_short_hold_is_still_fine() {
    let f = flow("- longPressOn:\n    id: hdr-back-btn\n    duration: 200\n").expect("parses");
    assert!(matches!(
        &f.steps[0],
        Step::LongPressOn {
            capture_during: false,
            ..
        }
    ));
}

#[test]
fn the_scalar_form_never_captures() {
    let f = flow("- longPressOn: 'Delete'\n").expect("parses");
    assert!(matches!(
        &f.steps[0],
        Step::LongPressOn {
            capture_during: false,
            ..
        }
    ));
}

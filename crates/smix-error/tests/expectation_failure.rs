//! Unit tests for smix-error.

use smix_error::{
    ExpectationFailure, FailureCode, FailureInit, build_suggestions, edit_distance, similarity,
};
use smix_screen::{ElementSummary, Rect, Role};
use smix_selector::{Modifiers, Pattern, Selector};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect { x, y, w, h }
}

fn summary(role: Option<Role>, name: Option<&str>, id: Option<&str>) -> ElementSummary {
    ElementSummary {
        role,
        name: name.map(String::from),
        id: id.map(String::from),
        text: None,
        bounds: rect(0.0, 0.0, 10.0, 10.0),
        enabled: true,
    }
}

// ---- FailureCode wire ---------------------------------------------------

#[test]
fn failure_code_serde_screaming_snake_case() {
    let codes = [
        (FailureCode::ElementNotFound, "\"ELEMENT_NOT_FOUND\""),
        (FailureCode::NotVisible, "\"NOT_VISIBLE\""),
        (FailureCode::NotEnabled, "\"NOT_ENABLED\""),
        (FailureCode::Ambiguous, "\"AMBIGUOUS\""),
        (FailureCode::Timeout, "\"TIMEOUT\""),
        (FailureCode::AssertionFailed, "\"ASSERTION_FAILED\""),
        (FailureCode::AppNotRunning, "\"APP_NOT_RUNNING\""),
        (FailureCode::SimulatorNotBooted, "\"SIMULATOR_NOT_BOOTED\""),
        (FailureCode::DriverError, "\"DRIVER_ERROR\""),
    ];
    for (c, expected) in codes {
        let json = serde_json::to_string(&c).expect("serialize");
        assert_eq!(json, expected);
        let parsed: FailureCode = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(parsed, c);
    }
}

// ---- ExpectationFailure shape ------------------------------------------

#[test]
fn expectation_failure_basic_construct() {
    let f = ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::ElementNotFound),
        message: "element not found".into(),
        ..Default::default()
    });
    assert_eq!(f.code, FailureCode::ElementNotFound);
    assert_eq!(f.message, "element not found");
    assert!(f.selector.is_none());
    assert!(f.suggestions.is_empty());
    assert!(f.visible_elements.is_empty());
}

#[test]
fn expectation_failure_serde_round_trip_camel_case() {
    let f = ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::NotVisible),
        message: "hidden".into(),
        selector: Some(Selector::Text {
            text: Pattern::text("Submit"),
            modifiers: Modifiers::default(),
        }),
        suggestions: vec!["Did you mean 'Submit'?".into()],
        visible_elements: vec![summary(Some(Role::Button), Some("OK"), Some("btn-ok"))],
        hint: Some("button is offscreen".into()),
        device_log: vec!["[stderr] foo".into()],
        ..Default::default()
    });
    let json = serde_json::to_string(&f).expect("serialize");
    assert!(json.contains("\"ok\":false"));
    assert!(json.contains("\"code\":\"NOT_VISIBLE\""));
    assert!(json.contains("\"visibleElements\""));
    assert!(json.contains("\"deviceLog\""));
    let parsed: ExpectationFailure = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(parsed.code, FailureCode::NotVisible);
    assert_eq!(parsed.suggestions.len(), 1);
    assert_eq!(parsed.visible_elements.len(), 1);
}

#[test]
fn expectation_failure_skip_empty_optionals() {
    // Empty suggestions / visible_elements / device_log default-serialize
    // to empty arrays (wire contract: present-but-empty). hint / selector /
    // screenshot use skip_serializing_if so they're absent when None.
    let f = ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::Timeout),
        message: "tap timed out".into(),
        ..Default::default()
    });
    let json = serde_json::to_string(&f).expect("serialize");
    assert!(!json.contains("\"hint\""));
    assert!(!json.contains("\"selector\""));
    assert!(!json.contains("\"screenshot\""));
    assert!(!json.contains("\"deviceLog\""));
}

// ---- to_prompt rendering ------------------------------------------------

#[test]
fn to_prompt_basic_shape() {
    let f = ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::ElementNotFound),
        message: "element not found: { text: \"Login\" }".into(),
        selector: Some(Selector::Text {
            text: Pattern::text("Login"),
            modifiers: Modifiers::default(),
        }),
        suggestions: vec!["Did you mean \"Log in\"?".into()],
        visible_elements: vec![
            summary(Some(Role::Button), Some("Log in"), Some("btn-login")),
            summary(Some(Role::Cell), Some("Dashboard"), None),
        ],
        hint: Some("selector text is case-insensitive — check label".into()),
        ..Default::default()
    });
    let prompt = f.to_prompt();
    assert!(prompt.starts_with("FAIL [ELEMENT_NOT_FOUND]:"));
    assert!(prompt.contains("selector:"));
    assert!(prompt.contains("suggestions:"));
    assert!(prompt.contains("Did you mean"));
    assert!(prompt.contains("visible elements"));
    assert!(prompt.contains("button "));
    assert!(prompt.contains("hint:"));
}

#[test]
fn to_prompt_caps_visible_at_10() {
    let visibles: Vec<ElementSummary> = (0..15)
        .map(|i| summary(Some(Role::Cell), Some(&format!("Cell-{}", i)), None))
        .collect();
    let f = ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::ElementNotFound),
        message: "x".into(),
        visible_elements: visibles,
        ..Default::default()
    });
    let prompt = f.to_prompt();
    // top 10 only.
    assert!(prompt.contains("(top 10):"));
    assert!(prompt.contains("Cell-0"));
    assert!(prompt.contains("Cell-9"));
    assert!(!prompt.contains("Cell-10"));
}

#[test]
fn to_prompt_device_log_caps_at_last_200_lines() {
    let logs: Vec<String> = (0..300).map(|i| format!("line {}", i)).collect();
    let f = ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message: "x".into(),
        device_log: logs,
        ..Default::default()
    });
    let prompt = f.to_prompt();
    // Last 200 lines kept; line 99 dropped, line 100 kept.
    assert!(prompt.contains("(last 300 lines):"));
    assert!(!prompt.contains("- line 99\n"));
    assert!(prompt.contains("- line 100"));
    assert!(prompt.contains("- line 299"));
}

#[test]
fn expectation_failure_impl_error_trait() {
    let f = ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::ElementNotFound),
        message: "x".into(),
        ..Default::default()
    });
    let _err_obj: Box<dyn std::error::Error> = Box::new(f);
}

// ---- edit_distance / similarity ----------------------------------------

#[test]
fn edit_distance_basic() {
    assert_eq!(edit_distance("", ""), 0);
    assert_eq!(edit_distance("abc", ""), 3);
    assert_eq!(edit_distance("", "abc"), 3);
    assert_eq!(edit_distance("abc", "abc"), 0);
    assert_eq!(edit_distance("abc", "abd"), 1);
    assert_eq!(edit_distance("kitten", "sitting"), 3);
    assert_eq!(edit_distance("flaw", "lawn"), 2);
}

#[test]
fn similarity_basic() {
    assert_eq!(similarity("", ""), 1.0);
    assert_eq!(similarity("abc", "abc"), 1.0);
    let s = similarity("kitten", "sitting");
    // (7 - 3) / 7 = 0.571
    assert!((s - 4.0 / 7.0).abs() < 1e-9);
}

// ---- build_suggestions ------------------------------------------------------

#[test]
fn build_suggestions_target_none_returns_empty() {
    let visible = vec![summary(Some(Role::Button), Some("OK"), None)];
    assert!(build_suggestions(None, &visible).is_empty());
}

#[test]
fn build_suggestions_top_3_ordered_by_score_desc_then_field_then_index() {
    let visible = vec![
        summary(Some(Role::Button), Some("Submit"), None),
        summary(Some(Role::Button), Some("Login"), None),
        summary(Some(Role::Button), Some("Cancel"), None),
        summary(Some(Role::Cell), Some("Logout"), None),
    ];
    let s = build_suggestions(Some("Logn"), &visible);
    // Top match should be "Login" (closest), other relevant by score.
    assert!(s.len() <= 3);
    assert!(s[0].contains("Login"));
}

#[test]
fn build_suggestions_matches_id_field() {
    // An id-selector typo is the most common failure: the visible element
    // carries the correct id, differing by one character. build_suggestions
    // must surface it via the `id` field, not only name / text.
    let visible = vec![
        summary(Some(Role::Button), None, Some("search_action_bar")),
        summary(Some(Role::Button), Some("Settings"), Some("settings_homepage_container")),
    ];
    let s = build_suggestions(Some("search_action_barX"), &visible);
    assert!(!s.is_empty(), "an id typo should yield a suggestion");
    assert!(s[0].contains("search_action_bar"), "suggestion names the correct id: {s:?}");
    assert!(s[0].contains("field id"), "suggestion attributes the id field: {s:?}");
}

#[test]
fn build_suggestions_below_threshold_skipped() {
    let visible = vec![summary(
        Some(Role::Button),
        Some("CompletelyDifferent"),
        None,
    )];
    let s = build_suggestions(Some("xyz"), &visible);
    assert!(s.is_empty()); // similarity << 0.5
}

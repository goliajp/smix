//! Parse-layer contract for the AI-assertion verbs.
//!
//! The gate is checked at parse time on purpose: a flow that asks for a
//! non-deterministic judge without opting in should fail before a device is
//! ever touched, not halfway through a run.

use smix_adapter_maestro::{ParseError, Step, parse_flow_yaml, set_ai_assertions_override};

/// The override is a thread-local seam. Process env is global while cargo runs
/// tests across threads, so toggling it for real would race every sibling test.
struct AiGate;

impl AiGate {
    fn on() -> Self {
        set_ai_assertions_override(Some(true));
        AiGate
    }
    fn off() -> Self {
        set_ai_assertions_override(Some(false));
        AiGate
    }
}

impl Drop for AiGate {
    fn drop(&mut self) {
        set_ai_assertions_override(None);
    }
}

fn flow(body: &str) -> String {
    format!("appId: com.test.app\n---\n{body}")
}

#[test]
fn assert_condition_parses_when_enabled() {
    let _gate = AiGate::on();
    let f = parse_flow_yaml(&flow("- assertCondition: 'a red error toast is visible'\n")).unwrap();
    match &f.steps[0] {
        Step::AssertCondition { condition } => {
            assert_eq!(condition, "a red error toast is visible");
        }
        other => panic!("expected AssertCondition, got {other:?}"),
    }
}

#[test]
fn extract_with_ai_parses_when_enabled() {
    let _gate = AiGate::on();
    let f = parse_flow_yaml(&flow(
        "- extractWithAI:\n    into: order\n    fields: ['total', 'currency']\n",
    ))
    .unwrap();
    match &f.steps[0] {
        Step::ExtractWithAI { into, fields } => {
            assert_eq!(into, "order");
            assert_eq!(fields, &["total".to_string(), "currency".to_string()]);
        }
        other => panic!("expected ExtractWithAI, got {other:?}"),
    }
}

#[test]
fn ai_verbs_are_refused_without_opt_in() {
    let _gate = AiGate::off();
    let err = parse_flow_yaml(&flow("- assertCondition: 'a toast is visible'\n")).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("assertCondition"),
        "the error must name the verb that was refused; got: {msg}"
    );
    assert!(
        msg.contains("SMIX_ENABLE_AI_ASSERTIONS"),
        "refusing without saying how to enable it just strands the author; got: {msg}"
    );
}

#[test]
fn extract_with_ai_is_refused_without_opt_in() {
    let _gate = AiGate::off();
    let err =
        parse_flow_yaml(&flow("- extractWithAI:\n    into: order\n    fields: ['total']\n"))
            .unwrap_err();
    assert!(err.to_string().contains("extractWithAI"));
}

#[test]
fn extract_with_ai_requires_into_and_fields() {
    let _gate = AiGate::on();
    let err = parse_flow_yaml(&flow("- extractWithAI:\n    fields: ['total']\n")).unwrap_err();
    assert!(
        matches!(err, ParseError::MissingField(_) | ParseError::InvalidValue { .. }),
        "missing `into` should be a field error, got: {err:?}"
    );
}

#[test]
fn the_ai_verbs_are_in_the_canonical_table() {
    // The maestro names are the alias an author writes when porting.
    assert!(smix_verbs::is_known_verb("assertWithAI"));
    assert!(smix_verbs::is_known_verb("assertCondition"));
    assert!(smix_verbs::is_known_verb("extractTextWithAI"));
    assert!(smix_verbs::is_known_verb("extractWithAI"));
}

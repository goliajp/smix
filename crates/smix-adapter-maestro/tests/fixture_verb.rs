//! v0.3.0 Phase B B3 — `- fixture:` yaml verb parser tests.

use smix_adapter_maestro::{Step, parse_flow_yaml};

fn parse_and_take_step(yaml: &str, idx: usize) -> Step {
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    flow.steps.into_iter().nth(idx).expect("step exists")
}

#[test]
fn fixture_short_form() {
    let yaml = "
appId: com.example
---
- fixture: prime-search-history
";
    let step = parse_and_take_step(yaml, 0);
    match step {
        Step::Fixture { id, timeout_ms } => {
            assert_eq!(id, "prime-search-history");
            assert_eq!(timeout_ms, None);
        }
        other => panic!("expected Step::Fixture, got {other:?}"),
    }
}

#[test]
fn fixture_long_form_with_timeout_override() {
    let yaml = "
appId: com.example
---
- fixture:
    id: prime-search-history
    timeoutMs: 12000
";
    let step = parse_and_take_step(yaml, 0);
    match step {
        Step::Fixture { id, timeout_ms } => {
            assert_eq!(id, "prime-search-history");
            assert_eq!(timeout_ms, Some(12000));
        }
        other => panic!("expected Step::Fixture, got {other:?}"),
    }
}

#[test]
fn fixture_long_form_without_timeout_uses_registry_default() {
    let yaml = "
appId: com.example
---
- fixture:
    id: enter-qa-mode
";
    let step = parse_and_take_step(yaml, 0);
    match step {
        Step::Fixture { id, timeout_ms } => {
            assert_eq!(id, "enter-qa-mode");
            assert_eq!(timeout_ms, None);
        }
        other => panic!("expected Step::Fixture, got {other:?}"),
    }
}

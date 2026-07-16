//! `expect.signal` / `expect.signals` /
//! `expect.logClean` yaml verb parser tests.

use smix_adapter_maestro::{SignalMatch, SignalOrderKind, SignalWindow, Step, parse_flow_yaml};

fn parse_and_take_step(yaml: &str, idx: usize) -> Step {
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    flow.steps.into_iter().nth(idx).expect("step exists")
}

#[test]
fn expect_signal_short_form() {
    let yaml = r#"
appId: com.example
---
- expect:
    signal:
      regex: "env=qa-mode"
      timeoutMs: 8000
"#;
    let step = parse_and_take_step(yaml, 0);
    match step {
        Step::ExpectSignal {
            regex,
            timeout_ms,
            window,
            level,
        } => {
            assert_eq!(regex, "env=qa-mode");
            assert_eq!(timeout_ms, 8000);
            assert!(matches!(window, SignalWindow::SinceRun));
            assert_eq!(level, None);
        }
        other => panic!("expected ExpectSignal, got {other:?}"),
    }
}

#[test]
fn expect_signal_with_level_and_since_step_window() {
    let yaml = r#"
appId: com.example
---
- expect:
    signal:
      regex: "unexpected warning"
      level: warn
    timeoutMs: 5000
    window:
      sinceStep: 3
"#;
    let step = parse_and_take_step(yaml, 0);
    match step {
        Step::ExpectSignal {
            regex,
            level,
            timeout_ms,
            window,
        } => {
            assert_eq!(regex, "unexpected warning");
            assert_eq!(level.as_deref(), Some("warn"));
            assert_eq!(timeout_ms, 5000);
            assert!(matches!(window, SignalWindow::SinceStep { since_step: 3 }));
        }
        other => panic!("expected ExpectSignal, got {other:?}"),
    }
}

#[test]
fn expect_signals_strict_order() {
    let yaml = r#"
appId: com.example
---
- expect:
    signals:
      - regex: "^launchOverrideConsumed"
      - regex: "^autoLoginValidated"
      - regex: "^readyForInteractive"
    order: strict
    timeoutMs: 30000
"#;
    let step = parse_and_take_step(yaml, 0);
    match step {
        Step::ExpectSignals {
            signals,
            order,
            timeout_ms,
            window,
        } => {
            assert_eq!(signals.len(), 3);
            assert!(matches!(order, SignalOrderKind::Strict));
            assert_eq!(timeout_ms, 30000);
            assert!(matches!(window, SignalWindow::SinceRun));
            let regexes: Vec<&str> = signals
                .iter()
                .map(|s: &SignalMatch| s.regex.as_str())
                .collect();
            assert_eq!(
                regexes,
                vec![
                    "^launchOverrideConsumed",
                    "^autoLoginValidated",
                    "^readyForInteractive",
                ]
            );
        }
        other => panic!("expected ExpectSignals, got {other:?}"),
    }
}

#[test]
fn expect_signals_default_any_order() {
    let yaml = r#"
appId: com.example
---
- expect:
    signals:
      - regex: "a"
      - regex: "b"
    timeoutMs: 5000
"#;
    let step = parse_and_take_step(yaml, 0);
    match step {
        Step::ExpectSignals { order, .. } => {
            assert!(matches!(order, SignalOrderKind::Any));
        }
        other => panic!("expected ExpectSignals, got {other:?}"),
    }
}

#[test]
fn expect_log_clean_shorthand() {
    let yaml = r#"
appId: com.example
---
- expectLogClean
"#;
    let step = parse_and_take_step(yaml, 0);
    assert!(matches!(step, Step::ExpectLogClean));
}

#[test]
fn expect_log_clean_via_expect_key() {
    let yaml = r#"
appId: com.example
---
- expect:
    logClean: true
"#;
    let step = parse_and_take_step(yaml, 0);
    assert!(matches!(step, Step::ExpectLogClean));
}

#[test]
fn expect_visible_still_falls_through() {
    // Backward-compat: `expect: { visible: Foo }` should keep working
    // as assertVisible.
    let yaml = r#"
appId: com.example
---
- expect: Login
"#;
    let step = parse_and_take_step(yaml, 0);
    match step {
        Step::AssertVisible { .. } => {}
        other => panic!("expected AssertVisible fall-through, got {other:?}"),
    }
}

//! Generator tests for smix-recorder.

use smix_input::{KeyName, SwipeDirection};
use smix_recorder::{RecordSession, generate_maestro_yaml, generate_rust};
use smix_authoring_ir::{IRAction, RecorderErrorReason};
use smix_selector::{Modifiers, Pattern, Selector};

fn text_sel(t: &str) -> Selector {
    Selector::Text {
        text: Pattern::text(t),
        modifiers: Modifiers::default(),
    }
}

// ---- RecordSession ------------------------------------------------------

#[test]
fn record_session_empty_by_default() {
    let s = RecordSession::new();
    assert!(s.is_empty());
    assert_eq!(s.len(), 0);
}

#[test]
fn record_session_buffers_actions_in_order() {
    let s = RecordSession::new();
    s.record(IRAction::Tap {
        selector: text_sel("Login"),
        timestamp_ms: 1.0,
    });
    s.record(IRAction::GoBack { timestamp_ms: 2.0 });
    assert_eq!(s.len(), 2);
    let snap = s.snapshot();
    assert_eq!(snap[0].kind(), "tap");
    assert_eq!(snap[1].kind(), "goBack");
}

#[test]
fn record_session_stop_drains_and_resets() {
    let s = RecordSession::new();
    s.record(IRAction::Tap {
        selector: text_sel("X"),
        timestamp_ms: 1.0,
    });
    let drained = s.stop();
    assert_eq!(drained.len(), 1);
    assert!(s.is_empty());
}

#[test]
fn record_session_snapshot_is_independent_copy() {
    let s = RecordSession::new();
    s.record(IRAction::Tap {
        selector: text_sel("X"),
        timestamp_ms: 1.0,
    });
    let snap = s.snapshot();
    s.record(IRAction::GoBack { timestamp_ms: 2.0 });
    assert_eq!(snap.len(), 1); // snapshot was independent
    assert_eq!(s.snapshot().len(), 2);
}

// ---- generate_maestro_yaml ---------------------------------------------

#[test]
fn maestro_yaml_empty_session_returns_error() {
    let err = generate_maestro_yaml(&[], "com.example.app").unwrap_err();
    assert_eq!(err.reason, RecorderErrorReason::EmptySession);
}

#[test]
fn maestro_yaml_emits_app_id_header_and_separator() {
    let actions = vec![IRAction::Tap {
        selector: text_sel("Login"),
        timestamp_ms: 1.0,
    }];
    let yaml = generate_maestro_yaml(&actions, "com.example.app").unwrap();
    assert!(yaml.contains("appId: com.example.app"));
    assert!(yaml.contains("---"));
    assert!(yaml.contains("tapOn: Login"));
}

#[test]
fn maestro_yaml_fill_emits_two_steps() {
    let actions = vec![IRAction::Fill {
        selector: text_sel("Email"),
        text: "u@x.com".into(),
        timestamp_ms: 1.0,
    }];
    let yaml = generate_maestro_yaml(&actions, "com.x").unwrap();
    assert!(yaml.contains("tapOn: Email"));
    assert!(yaml.contains("inputText: u@x.com"));
}

#[test]
fn maestro_yaml_press_key_capitalized() {
    let actions = vec![IRAction::PressKey {
        key: KeyName::Return,
        timestamp_ms: 1.0,
    }];
    let yaml = generate_maestro_yaml(&actions, "com.x").unwrap();
    assert!(yaml.contains("pressKey: Return"));
}

#[test]
fn maestro_yaml_swipe_direction_uppercase() {
    let actions = vec![IRAction::Swipe {
        direction: SwipeDirection::Up,
        from: None,
        timestamp_ms: 1.0,
    }];
    let yaml = generate_maestro_yaml(&actions, "com.x").unwrap();
    assert!(yaml.contains("direction: UP"));
}

#[test]
fn maestro_yaml_go_back_emits_bare_string() {
    let actions = vec![IRAction::GoBack { timestamp_ms: 1.0 }];
    let yaml = generate_maestro_yaml(&actions, "com.x").unwrap();
    assert!(yaml.contains("- back"));
}

#[test]
fn maestro_yaml_wait_for_extended() {
    let actions = vec![IRAction::WaitFor {
        selector: text_sel("Dashboard"),
        timestamp_ms: 1.0,
    }];
    let yaml = generate_maestro_yaml(&actions, "com.x").unwrap();
    assert!(yaml.contains("extendedWaitUntil:"));
    assert!(yaml.contains("visible: Dashboard"));
    assert!(yaml.contains("timeout: 5000"));
}

// ---- generate_rust -----------------------------------------------------

#[test]
fn rust_empty_session_returns_error() {
    let err = generate_rust(&[], "my_test", "com.example.app").unwrap_err();
    assert_eq!(err.reason, RecorderErrorReason::EmptySession);
}

#[test]
fn rust_emits_use_statements_and_tokio_test_attr() {
    let actions = vec![IRAction::Tap {
        selector: text_sel("Login"),
        timestamp_ms: 1.0,
    }];
    let rs = generate_rust(&actions, "login_flow", "com.example.app").unwrap();
    assert!(rs.contains("use smix_sdk"));
    assert!(rs.contains("#[tokio::test"));
    assert!(rs.contains("async fn login_flow()"));
    assert!(rs.contains("app.launch(\"com.example.app\")"));
    assert!(rs.contains("app.open_session(\"com.example.app\", true)"));
    assert!(rs.contains("let app = session.app();"));
}

#[test]
fn rust_text_selector_uses_ergonomic_factory() {
    let actions = vec![IRAction::Tap {
        selector: text_sel("Login"),
        timestamp_ms: 1.0,
    }];
    let rs = generate_rust(&actions, "f", "com.x").unwrap();
    assert!(rs.contains("app.tap(&text(\"Login\")).await?;"));
}

#[test]
fn rust_fill_emits_two_args() {
    let actions = vec![IRAction::Fill {
        selector: text_sel("Email"),
        text: "u@x.com".into(),
        timestamp_ms: 1.0,
    }];
    let rs = generate_rust(&actions, "f", "com.x").unwrap();
    assert!(rs.contains("app.fill(&text(\"Email\"), \"u@x.com\").await?;"));
}

#[test]
fn rust_press_key_emits_typed_variant() {
    let actions = vec![IRAction::PressKey {
        key: KeyName::ArrowUp,
        timestamp_ms: 1.0,
    }];
    let rs = generate_rust(&actions, "f", "com.x").unwrap();
    assert!(rs.contains("app.press_key(KeyName::ArrowUp).await?;"));
}

#[test]
fn rust_swipe_emits_typed_direction() {
    let actions = vec![IRAction::Swipe {
        direction: SwipeDirection::Down,
        from: None,
        timestamp_ms: 1.0,
    }];
    let rs = generate_rust(&actions, "f", "com.x").unwrap();
    assert!(rs.contains("app.swipe_once(SwipeDirection::Down).await?;"));
}

#[test]
fn rust_go_back_and_hide_keyboard_passthrough() {
    let actions = vec![
        IRAction::GoBack { timestamp_ms: 1.0 },
        IRAction::HideKeyboard { timestamp_ms: 2.0 },
    ];
    let rs = generate_rust(&actions, "f", "com.x").unwrap();
    assert!(rs.contains("app.go_back().await?;"));
    assert!(rs.contains("app.hide_keyboard().await?;"));
}

#[test]
fn rust_wait_for_emits_duration_secs_5() {
    let actions = vec![IRAction::WaitFor {
        selector: text_sel("Dashboard"),
        timestamp_ms: 1.0,
    }];
    let rs = generate_rust(&actions, "f", "com.x").unwrap();
    assert!(rs.contains("app.wait_for(&text(\"Dashboard\"), Duration::from_secs(5)).await?;"));
}

#[test]
fn rust_str_literal_escapes_special_chars() {
    let actions = vec![IRAction::Fill {
        selector: text_sel("Path"),
        text: "C:\\Users\\\"alice\"\nline2".into(),
        timestamp_ms: 1.0,
    }];
    let rs = generate_rust(&actions, "f", "com.x").unwrap();
    assert!(rs.contains(r#""C:\\Users\\\"alice\"\nline2""#));
}

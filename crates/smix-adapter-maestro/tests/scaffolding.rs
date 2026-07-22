//! Scaffolding placeholder tests — these verify that the public API
//! surface compiles.

use smix_adapter_maestro::{Flow, ParseError, Step};

#[test]
fn step_enum_compiles() {
    let _ = Step::WaitForAnimationToEnd { ceiling_ms: 400 };
    let _ = Step::InputText("test".to_string());
    let _ = Step::Back;
}

#[test]
fn flow_struct_compiles() {
    let flow = Flow {
        app_id: "com.example.app".to_string(),
        app: None,
        launch_activity: None,
        steps: vec![Step::WaitForAnimationToEnd { ceiling_ms: 400 }],
    };
    assert_eq!(flow.app_id, "com.example.app");
    assert_eq!(flow.steps.len(), 1);
}

#[test]
fn parse_error_displays() {
    let err = ParseError::UnsupportedCommand("foobar".to_string());
    let display = format!("{err}");
    assert!(display.contains("foobar"));
}

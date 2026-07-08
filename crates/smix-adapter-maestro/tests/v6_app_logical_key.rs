//! yaml `app:` logical key parsing + resolve into Flow.

use smix_adapter_maestro::{AppsConfig, Flow, Step, parse_flow_yaml, resolve_app_into_flow};
use smix_driver::Platform;

const APPS_YAML: &str = r#"
apps:
  demoApp:
    ios:
      bundleId: com.example.app
    android:
      package: com.example.app
      activity: .MainActivity
"#;

#[test]
fn parser_accepts_app_key_without_app_id() {
    let yaml = "app: demoApp\n---\n- waitForAnimationToEnd\n";
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    assert_eq!(flow.app_id, "");
    assert_eq!(flow.app.as_deref(), Some("demoApp"));
}

#[test]
fn parser_accepts_legacy_app_id_with_no_app() {
    let yaml = "appId: com.example.app\n---\n- waitForAnimationToEnd\n";
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    assert_eq!(flow.app_id, "com.example.app");
    assert_eq!(flow.app, None);
}

#[test]
fn parser_accepts_both_app_and_app_id() {
    let yaml = "appId: com.example.app\napp: demoApp\n---\n- waitForAnimationToEnd\n";
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    assert_eq!(flow.app_id, "com.example.app");
    assert_eq!(flow.app.as_deref(), Some("demoApp"));
}

#[test]
fn parser_errors_when_neither_present() {
    // Header doc is a mapping with neither `app` nor `appId` field.
    let yaml = "stopApp: false\n---\n- waitForAnimationToEnd\n";
    let err = parse_flow_yaml(yaml).unwrap_err();
    let msg = format!("{err}");
    assert!(
        msg.contains("app or appId"),
        "expected error to mention `app or appId`, got: {msg}"
    );
}

#[test]
fn resolve_logical_to_ios_bundle() {
    let apps = AppsConfig::from_yaml(APPS_YAML).unwrap();
    let mut flow = Flow {
        app_id: String::new(),
        app: Some("demoApp".to_string()),
        steps: vec![Step::WaitForAnimationToEnd],
    };
    resolve_app_into_flow(&mut flow, &apps, Platform::Ios).unwrap();
    assert_eq!(flow.app_id, "com.example.app");
}

#[test]
fn resolve_logical_to_android_package() {
    let apps = AppsConfig::from_yaml(APPS_YAML).unwrap();
    let mut flow = Flow {
        app_id: String::new(),
        app: Some("demoApp".to_string()),
        steps: vec![Step::WaitForAnimationToEnd],
    };
    resolve_app_into_flow(&mut flow, &apps, Platform::Android).unwrap();
    assert_eq!(flow.app_id, "com.example.app");
}

#[test]
fn resolve_patches_inherited_launch_app_step() {
    // bare `- launchApp` step inherits flow header app id. Verify the
    // resolver patches the empty app_id field on the LaunchApp step.
    let yaml = "app: demoApp\n---\n- launchApp\n";
    let mut flow = parse_flow_yaml(yaml).expect("parse ok");
    // Before resolve: flow.app_id empty, LaunchApp.app_id = "" inherited
    assert_eq!(flow.app_id, "");
    assert!(matches!(&flow.steps[0], Step::LaunchApp { app_id, .. } if app_id.is_empty()));

    let apps = AppsConfig::from_yaml(APPS_YAML).unwrap();
    resolve_app_into_flow(&mut flow, &apps, Platform::Ios).unwrap();
    assert_eq!(flow.app_id, "com.example.app");
    if let Step::LaunchApp { app_id, .. } = &flow.steps[0] {
        assert_eq!(app_id, "com.example.app");
    } else {
        panic!("expected LaunchApp");
    }
}

//! `takeScreenshot: { name, annotate: [...] }` yaml verb.

use smix_adapter_maestro::{AnnotationPos, AnnotationSpec, Step, parse_flow_yaml};

fn first_step(yaml: &str) -> Step {
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    flow.steps.into_iter().next().expect("step exists")
}

#[test]
fn takescreenshot_bare_still_parses() {
    let yaml = "
appId: com.example
---
- takeScreenshot
";
    let step = first_step(yaml);
    match step {
        Step::TakeScreenshot { path, annotations } => {
            assert!(path.is_none());
            assert!(annotations.is_empty());
        }
        other => panic!("expected TakeScreenshot, got {other:?}"),
    }
}

#[test]
fn takescreenshot_string_still_parses() {
    let yaml = "
appId: com.example
---
- takeScreenshot: hub-form.png
";
    let step = first_step(yaml);
    match step {
        Step::TakeScreenshot { path, annotations } => {
            assert_eq!(path.as_deref(), Some("hub-form.png"));
            assert!(annotations.is_empty());
        }
        other => panic!("expected TakeScreenshot, got {other:?}"),
    }
}

#[test]
fn takescreenshot_long_form_with_circle() {
    let yaml = r#"
appId: com.example
---
- takeScreenshot:
    name: hub-form.png
    annotate:
      - circle:
          at: { x: 200, y: 150 }
          color: red
          radius: 40
          stroke: 3
"#;
    let step = first_step(yaml);
    match step {
        Step::TakeScreenshot { path, annotations } => {
            assert_eq!(path.as_deref(), Some("hub-form.png"));
            assert_eq!(annotations.len(), 1);
            match &annotations[0] {
                AnnotationSpec::Circle {
                    at,
                    color,
                    radius,
                    stroke,
                } => {
                    matches!(at, AnnotationPos::Pixel { x: 200, y: 150 });
                    assert_eq!(color, "red");
                    assert_eq!(*radius, 40);
                    assert_eq!(*stroke, 3);
                }
                other => panic!("expected Circle, got {other:?}"),
            }
        }
        other => panic!("expected TakeScreenshot, got {other:?}"),
    }
}

#[test]
fn takescreenshot_text_annotation() {
    let yaml = r#"
appId: com.example
---
- takeScreenshot:
    annotate:
      - text:
          at: { x: 20, y: 20 }
          content: step-1
          color: green
          size: 24
"#;
    let step = first_step(yaml);
    match step {
        Step::TakeScreenshot { path, annotations } => {
            assert!(path.is_none());
            assert_eq!(annotations.len(), 1);
            match &annotations[0] {
                AnnotationSpec::Text {
                    content,
                    color,
                    size,
                    ..
                } => {
                    assert_eq!(content, "step-1");
                    assert_eq!(color, "green");
                    assert_eq!(*size, 24.0);
                }
                other => panic!("expected Text, got {other:?}"),
            }
        }
        other => panic!("expected TakeScreenshot, got {other:?}"),
    }
}

#[test]
fn takescreenshot_normalized_position() {
    let yaml = r#"
appId: com.example
---
- takeScreenshot:
    annotate:
      - circle:
          at: { nx: 0.5, ny: 0.5 }
          color: red
"#;
    let step = first_step(yaml);
    match step {
        Step::TakeScreenshot { annotations, .. } => match &annotations[0] {
            AnnotationSpec::Circle { at, .. } => match at {
                AnnotationPos::Normalized { nx, ny } => {
                    assert!((*nx - 0.5).abs() < 0.001);
                    assert!((*ny - 0.5).abs() < 0.001);
                }
                other => panic!("expected Normalized, got {other:?}"),
            },
            _ => panic!("expected Circle"),
        },
        _ => panic!("expected TakeScreenshot"),
    }
}

#[test]
fn takescreenshot_multiple_annotations() {
    let yaml = r#"
appId: com.example
---
- takeScreenshot:
    name: multi.png
    annotate:
      - circle: { at: { x: 100, y: 100 }, color: red, radius: 30 }
      - arrow: { from: { x: 10, y: 10 }, to: { x: 200, y: 200 }, color: blue }
      - text: { at: { x: 50, y: 50 }, content: hello }
      - box: { at: { x: 5, y: 5 }, width: 100, height: 100 }
      - line: { from: { x: 0, y: 300 }, to: { x: 300, y: 0 } }
"#;
    let step = first_step(yaml);
    match step {
        Step::TakeScreenshot { annotations, .. } => {
            assert_eq!(annotations.len(), 5);
        }
        _ => panic!("expected TakeScreenshot"),
    }
}

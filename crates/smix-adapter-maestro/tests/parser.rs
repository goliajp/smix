//! Fixture-driven parser tests. Each test reads a yaml fixture from
//! `tests/fixtures/` and asserts the full [`Flow`] structure round-trips
//! into the Step enum.

use smix_adapter_maestro::{Flow, ParseError, Step, parse_flow_yaml, text_to_pattern};
use smix_selector::{Modifiers, Pattern, Role, Selector};

fn read_fixture(name: &str) -> String {
    let path = format!("tests/fixtures/{name}");
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read fixture {path}: {e}"))
}

fn text_selector(p: Pattern) -> Selector {
    Selector::Text {
        text: p,
        modifiers: Modifiers::default(),
    }
}

fn id_selector(id: &str) -> Selector {
    Selector::Id {
        id: id.to_string(),
        modifiers: Modifiers::default(),
    }
}

#[test]
fn parse_alerts_counting_full_match() {
    let yaml = read_fixture("alerts_counting.yaml");
    let flow = parse_flow_yaml(&yaml).expect("parse alerts_counting.yaml");

    let expected = Flow {
        app_id: "com.example.app".to_string(),
        app: None,
        launch_activity: None,
        steps: vec![
            Step::RunFlow("../../subflows/launch-warm.yaml".to_string()),
            Step::RunFlow("../../subflows/ensure-login.yaml".to_string()),
            Step::RunFlow("../../subflows/go-to-alerts.yaml".to_string()),
            // tapOn: "Counting" — short string form, no '|' → Pattern::Text
            Step::TapOn {
                selector: text_selector(Pattern::Text("Counting".to_string())),
                optional: false,
                dispatch: None,
            },
            Step::WaitForAnimationToEnd { ceiling_ms: 400 },
            // extendedWaitUntil { visible: { text: "...|...|..." }, timeout: 30000 }
            Step::ExtendedWaitUntil {
                selector: text_selector(Pattern::Regex {
                    regex: "No counting configurations|Area Counting|Line Crossing|Crowd"
                        .to_string(),
                    flags: "i".to_string(),
                }),
                timeout_ms: 30000,
                expect_visible: true,
            },
            // tapOn { text: "Area Counting|...|...", index: 0, optional: true }
            Step::TapOn {
                selector: Selector::Text {
                    text: Pattern::Regex {
                        regex: "Area Counting|Line Crossing|Crowd Estimate".to_string(),
                        flags: "i".to_string(),
                    },
                    modifiers: Modifiers {
                        nth: Some(0),
                        ..Modifiers::default()
                    },
                },
                optional: true,
                dispatch: None,
            },
            Step::WaitForAnimationToEnd { ceiling_ms: 400 },
            Step::ExtendedWaitUntil {
                selector: text_selector(Pattern::Regex {
                    regex: "Day|Week|Month|No counting configurations|Area Counting".to_string(),
                    flags: "i".to_string(),
                }),
                timeout_ms: 30000,
                expect_visible: true,
            },
            // tapOn { text: "Week", optional: true } — no '|' → Pattern::Text
            Step::TapOn {
                selector: text_selector(Pattern::Text("Week".to_string())),
                optional: true,
                dispatch: None,
            },
            // tapOn { text: "Month", optional: true }
            Step::TapOn {
                selector: text_selector(Pattern::Text("Month".to_string())),
                optional: true,
                dispatch: None,
            },
        ],
    };

    assert_eq!(flow, expected);
}

#[test]
fn parse_ensure_login_with_runflow_when_clause() {
    let yaml = read_fixture("ensure_login.yaml");
    let flow = parse_flow_yaml(&yaml).expect("parse ensure_login.yaml");

    let expected = Flow {
        app_id: "com.example.app".to_string(),
        app: None,
        launch_activity: None,
        steps: vec![
            // runFlow: { when: { visible: "Log in" }, file: ../subflows/login.yaml }
            Step::RunFlowConditional {
                file: "../subflows/login.yaml".to_string(),
                when: Some(visible_only(text_selector(Pattern::Text("Log in".to_string())))),
                as_name: None,
                env: Vec::new(),
                opts: Default::default(),
            },
            // extendedWaitUntil: { visible: { id: "btn-open-menu" }, timeout: 30000 }
            Step::ExtendedWaitUntil {
                selector: id_selector("btn-open-menu"),
                timeout_ms: 30000,
                expect_visible: true,
            },
        ],
    };

    assert_eq!(flow, expected);
}

// `runFlow: { when: { visible }, commands: [...] }` inline form.
// Mirrors maestro YamlRunFlow's `commands:` alternative to `file:`. Body is
// a literal step list (no child yaml lookup); the parser must surface it as
// `Step::RunFlowInline`.
#[test]
fn parse_run_flow_inline_commands_with_when() {
    let yaml = concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    when:\n",
        "      visible: 'Open in'\n",
        "    commands:\n",
        "      - tapOn: 'Open'\n",
        "      - waitForAnimationToEnd\n",
    );
    let flow = parse_flow_yaml(yaml).expect("parse inline commands form");
    let expected = Flow {
        app_id: "com.t.r".to_string(),
        app: None,
        launch_activity: None,
        steps: vec![Step::RunFlowInline {
            when: Some(visible_only(text_selector(Pattern::Text("Open in".to_string())))),
            env: Vec::new(),
            opts: Default::default(),
            steps: vec![
                Step::TapOn {
                    selector: text_selector(Pattern::Text("Open".to_string())),
                    optional: false,
                    dispatch: None,
                },
                Step::WaitForAnimationToEnd { ceiling_ms: 400 },
            ],
        }],
    };
    assert_eq!(flow, expected);
}

#[test]
fn parse_run_flow_inline_commands_no_when() {
    // `commands:` is accepted with no `when:` block — body runs unconditionally.
    let yaml = concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    commands:\n",
        "      - tapOn: 'Hello'\n",
    );
    let flow = parse_flow_yaml(yaml).expect("parse inline no-when");
    assert_eq!(
        flow.steps,
        vec![Step::RunFlowInline {
            when: None,
            env: Vec::new(),
            opts: Default::default(),
            steps: vec![Step::TapOn {
                selector: text_selector(Pattern::Text("Hello".to_string())),
                optional: false,
                dispatch: None,
            }],
        }]
    );
}

#[test]
fn parse_run_flow_inline_rejects_file_and_commands_together() {
    let yaml = concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    file: foo.yaml\n",
        "    commands:\n",
        "      - tapOn: 'X'\n",
    );
    let err = parse_flow_yaml(yaml).expect_err("file + commands must be mutually exclusive");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("file") && msg.contains("commands"),
        "expected error to mention both `file` and `commands`, got {msg}"
    );
}

#[test]
fn parse_run_flow_inline_rejects_as_alias() {
    let yaml = concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    as: probe\n",
        "    commands:\n",
        "      - tapOn: 'X'\n",
    );
    let err = parse_flow_yaml(yaml).expect_err("`as` is only valid with `file`");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("as"),
        "expected error to mention `as` alias, got {msg}"
    );
}

#[test]
fn parse_run_flow_missing_file_and_commands() {
    let yaml = concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    when:\n",
        "      visible: 'X'\n",
    );
    let err = parse_flow_yaml(yaml).expect_err("must specify file or commands");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("file") || msg.contains("commands"),
        "expected error to mention `file` or `commands`, got {msg}"
    );
}

#[test]
fn parse_launch_warm_extras() {
    let yaml = read_fixture("launch_warm.yaml");
    let flow = parse_flow_yaml(&yaml).expect("parse launch_warm.yaml");

    let expected = Flow {
        app_id: "com.example.app".to_string(),
        app: None,
        launch_activity: None,
        steps: vec![
            Step::StopApp,
            Step::OpenLink(
                "exp+example-app://expo-development-client/?url=http%3A%2F%2Flocalhost%3A8081"
                    .to_string(),
            ),
            // short string with '|' → Pattern::Regex
            Step::ExtendedWaitUntil {
                selector: text_selector(Pattern::Regex {
                    regex: "Log in|Device".to_string(),
                    flags: "i".to_string(),
                }),
                timeout_ms: 45000,
                expect_visible: true,
            },
            Step::WaitForAnimationToEnd { ceiling_ms: 400 },
        ],
    };

    assert_eq!(flow, expected);
}

#[test]
fn pattern_regex_inference_on_pipe() {
    // No '|' → plain text
    assert_eq!(
        text_to_pattern("Counting"),
        Pattern::Text("Counting".to_string())
    );
    // Contains '|' → regex with /i
    assert_eq!(
        text_to_pattern("A|B"),
        Pattern::Regex {
            regex: "A|B".to_string(),
            flags: "i".to_string(),
        }
    );
}

#[test]
fn unsupported_command_returns_error() {
    // A maestro command smix does not wire must be reported, not quietly
    // skipped.
    //
    // This used to probe with `back`, on the grounds that it "is Android-only
    // and does not apply to the iOS simulator". It is not: the iOS runner
    // serves POST /back and taps the navigation bar's back button, Android
    // presses the hardware key, and the parity table has always listed it on
    // both. The comment asserted a limitation the code does not have, and the
    // assertion held `back` unparseable for as long as it stood.
    //
    // `inputRandomEmail` is a real maestro verb smix genuinely does not wire.
    // (`assertWithAI` is wired now, gated behind the AI tier's opt-in; its
    // refusal path is covered in tests/ai_verbs.rs.)
    let yaml = "appId: com.test.app\n---\n- inputRandomEmail\n";
    let err = parse_flow_yaml(yaml).expect_err("inputRandomEmail is not wired");
    match err {
        ParseError::UnsupportedCommand(cmd) => {
            assert_eq!(cmd, "inputRandomEmail");
        }
        other => panic!("expected UnsupportedCommand inputRandomEmail, got: {other:?}"),
    }
}

// ----------------------------------------------------------------------
// Selector::LocalizedText DSL parser tests (5 cases)
// ----------------------------------------------------------------------

#[test]
fn localized_text_basic_3_locale_table() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    localized_text:
      en: "Submit"
      ja: "送信"
      es: "Enviar"
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    assert_eq!(flow.steps.len(), 1);
    match &flow.steps[0] {
        Step::TapOn {
            selector, optional, ..
        } => {
            assert!(!optional);
            match selector {
                Selector::LocalizedText {
                    localized_text,
                    modifiers,
                } => {
                    assert_eq!(localized_text.get("en"), Some(&"Submit".to_string()));
                    assert_eq!(localized_text.get("ja"), Some(&"送信".to_string()));
                    assert_eq!(localized_text.get("es"), Some(&"Enviar".to_string()));
                    assert_eq!(localized_text.len(), 3);
                    assert_eq!(modifiers, &Modifiers::default());
                }
                other => panic!("expected LocalizedText, got: {other:?}"),
            }
        }
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

#[test]
fn localized_text_empty_table_rejected() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    localized_text: {}
"#;
    let err = parse_flow_yaml(yaml).expect_err("empty table must error");
    match err {
        ParseError::InvalidValue { field, reason } => {
            assert!(field.contains("localized_text"), "field={field}");
            assert!(reason.to_lowercase().contains("empty"), "reason={reason}");
        }
        other => panic!("expected InvalidValue, got: {other:?}"),
    }
}

#[test]
fn localized_text_single_locale_ok() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    localized_text:
      ja: "送信"
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn { selector, .. } => match selector {
            Selector::LocalizedText { localized_text, .. } => {
                assert_eq!(localized_text.len(), 1);
                assert_eq!(localized_text.get("ja"), Some(&"送信".to_string()));
            }
            other => panic!("expected LocalizedText, got: {other:?}"),
        },
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

#[test]
fn localized_text_empty_value_rejected() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    localized_text:
      en: ""
"#;
    let err = parse_flow_yaml(yaml).expect_err("empty value must error");
    match err {
        ParseError::InvalidValue { field, reason } => {
            assert!(field.contains("localized_text"), "field={field}");
            assert!(reason.contains("non-empty"), "reason={reason}");
        }
        other => panic!("expected InvalidValue, got: {other:?}"),
    }
}

#[test]
fn localized_text_with_optional_flag() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    localized_text:
      en: "Maybe"
      ja: "多分"
    optional: true
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn {
            selector, optional, ..
        } => {
            assert!(optional);
            match selector {
                Selector::LocalizedText { localized_text, .. } => {
                    assert_eq!(localized_text.len(), 2);
                }
                other => panic!("expected LocalizedText, got: {other:?}"),
            }
        }
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

// ----------------------------------------------------------------------
// Selector::OcrText DSL parser tests (5 cases)
// ----------------------------------------------------------------------

#[test]
fn ocr_text_short_form_string() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    ocrText: "Submit"
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn {
            selector, optional, ..
        } => {
            assert!(!optional);
            match selector {
                Selector::OcrText {
                    ocr_text,
                    locales,
                    modifiers,
                } => {
                    assert_eq!(ocr_text, "Submit");
                    assert!(
                        locales.is_empty(),
                        "short form leaves locales empty (adapter fills)"
                    );
                    assert_eq!(modifiers, &Modifiers::default());
                }
                other => panic!("expected OcrText, got: {other:?}"),
            }
        }
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

#[test]
fn ocr_text_full_form_map() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    ocrText:
      text: "送信"
      locales: ["ja"]
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn { selector, .. } => match selector {
            Selector::OcrText {
                ocr_text, locales, ..
            } => {
                assert_eq!(ocr_text, "送信");
                assert_eq!(locales, &vec!["ja".to_string()]);
            }
            other => panic!("expected OcrText, got: {other:?}"),
        },
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

#[test]
fn ocr_text_full_form_multi_locales() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    ocrText:
      text: "OK"
      locales: ["en", "ja", "es"]
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn { selector, .. } => match selector {
            Selector::OcrText { locales, .. } => {
                assert_eq!(locales.len(), 3);
                assert_eq!(locales[0], "en");
                assert_eq!(locales[2], "es");
            }
            other => panic!("expected OcrText, got: {other:?}"),
        },
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

#[test]
fn ocr_text_empty_string_rejected() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    ocrText: ""
"#;
    let err = parse_flow_yaml(yaml).expect_err("empty ocrText must error");
    match err {
        ParseError::InvalidValue { field, reason } => {
            assert!(field.contains("ocrText"), "field={field}");
            assert!(reason.contains("non-empty"), "reason={reason}");
        }
        other => panic!("expected InvalidValue, got: {other:?}"),
    }
}

#[test]
fn ocr_text_full_form_missing_text_field_rejected() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    ocrText:
      locales: ["en"]
"#;
    let err = parse_flow_yaml(yaml).expect_err("missing text must error");
    match err {
        ParseError::InvalidValue { field, .. } => {
            assert!(field.contains("ocrText.text"), "field={field}");
        }
        other => panic!("expected InvalidValue, got: {other:?}"),
    }
}

// ----------------------------------------------------------------------
// Selector::AnchorRelative DSL parser tests (5 cases)
// ----------------------------------------------------------------------

#[test]
fn anchored_basic_id_anchor() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    anchored:
      anchor:
        id: "submit-btn"
      dx: 0.15
      dy: -0.05
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn {
            selector, optional, ..
        } => {
            assert!(!optional);
            match selector {
                Selector::AnchorRelative { anchor, dx, dy } => {
                    assert_eq!(*dx, 0.15);
                    assert_eq!(*dy, -0.05);
                    match anchor.as_ref() {
                        Selector::Id { id, .. } => assert_eq!(id, "submit-btn"),
                        other => panic!("expected anchor Id, got: {other:?}"),
                    }
                }
                other => panic!("expected AnchorRelative, got: {other:?}"),
            }
        }
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

#[test]
fn anchored_text_anchor() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    anchored:
      anchor:
        text: "Status"
      dx: 0.3
      dy: 0
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn { selector, .. } => match selector {
            Selector::AnchorRelative { anchor, dx, dy } => {
                assert_eq!(*dx, 0.3);
                assert_eq!(*dy, 0.0);
                match anchor.as_ref() {
                    Selector::Text { .. } => {}
                    other => panic!("expected anchor Text, got: {other:?}"),
                }
            }
            other => panic!("expected AnchorRelative, got: {other:?}"),
        },
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

#[test]
fn anchored_missing_anchor_rejected() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    anchored:
      dx: 0.1
      dy: 0.1
"#;
    let err = parse_flow_yaml(yaml).expect_err("missing anchor must error");
    match err {
        ParseError::InvalidValue { field, .. } => {
            assert!(field.contains("anchored.anchor"), "field={field}");
        }
        other => panic!("expected InvalidValue, got: {other:?}"),
    }
}

#[test]
fn anchored_missing_dx_rejected() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    anchored:
      anchor:
        id: "x"
      dy: 0
"#;
    let err = parse_flow_yaml(yaml).expect_err("missing dx must error");
    match err {
        ParseError::InvalidValue { field, .. } => {
            assert!(field.contains("anchored.dx"), "field={field}");
        }
        other => panic!("expected InvalidValue, got: {other:?}"),
    }
}

#[test]
fn anchored_negative_dx_ok() {
    // Negative offsets are legal: the yaml does not constrain sign,
    // and the adapter clamps into [0, 1].
    let yaml = r#"appId: com.test.app
---
- tapOn:
    anchored:
      anchor:
        id: "x"
      dx: -0.25
      dy: 0
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn { selector, .. } => match selector {
            Selector::AnchorRelative { dx, .. } => assert_eq!(*dx, -0.25),
            other => panic!("expected AnchorRelative, got: {other:?}"),
        },
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

// ----------------------------------------------------------------------
// Selector::Fallback + Selector::Point DSL parser tests (5)
// ----------------------------------------------------------------------

#[test]
fn fallback_chain_basic_3_layer() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    fallback:
      - id: "submit-btn"
      - ocrText: "Submit"
      - point: [0.5, 0.9]
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn { selector, .. } => match selector {
            Selector::Fallback { fallback } => {
                assert_eq!(fallback.len(), 3);
                assert!(matches!(fallback[0], Selector::Id { .. }));
                assert!(matches!(fallback[1], Selector::OcrText { .. }));
                match &fallback[2] {
                    Selector::Point { nx, ny } => {
                        assert_eq!(*nx, 0.5);
                        assert_eq!(*ny, 0.9);
                    }
                    other => panic!("expected Point, got: {other:?}"),
                }
            }
            other => panic!("expected Fallback, got: {other:?}"),
        },
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

#[test]
fn fallback_chain_all_seven_layers() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    fallback:
      - id: "x"
      - text: "y"
      - localized_text: { en: "OK", ja: "確定" }
      - ocrText: "OK"
      - anchored:
          anchor:
            id: "anchor-x"
          dx: 0.1
          dy: 0
      - point: [0.5, 0.5]
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn { selector, .. } => match selector {
            Selector::Fallback { fallback } => {
                assert_eq!(fallback.len(), 6);
            }
            other => panic!("expected Fallback, got: {other:?}"),
        },
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

#[test]
fn fallback_empty_chain_rejected() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    fallback: []
"#;
    let err = parse_flow_yaml(yaml).expect_err("empty chain must error");
    match err {
        ParseError::InvalidValue { field, reason } => {
            assert!(field.contains("fallback"), "field={field}");
            assert!(reason.to_lowercase().contains("empty"), "reason={reason}");
        }
        other => panic!("expected InvalidValue, got: {other:?}"),
    }
}

#[test]
fn fallback_element_unknown_shape_rejected() {
    let yaml = r#"appId: com.test.app
---
- tapOn:
    fallback:
      - unknown_key: "x"
"#;
    let err = parse_flow_yaml(yaml).expect_err("unknown element shape must error");
    match err {
        ParseError::InvalidValue { field, .. } => {
            assert!(field.contains("fallback"), "field={field}");
        }
        other => panic!("expected InvalidValue, got: {other:?}"),
    }
}

#[test]
fn fallback_point_pct_string_form() {
    // Accept both 'X%,Y%' (Step::TapAtPoint syntax) and [nx, ny] array form.
    let yaml = r#"appId: com.test.app
---
- tapOn:
    fallback:
      - id: "x"
      - point: "50%,90%"
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::TapOn { selector, .. } => match selector {
            Selector::Fallback { fallback } => match &fallback[1] {
                Selector::Point { nx, ny } => {
                    assert!((*nx - 0.5).abs() < 1e-6);
                    assert!((*ny - 0.9).abs() < 1e-6);
                }
                other => panic!("expected Point, got: {other:?}"),
            },
            other => panic!("expected Fallback, got: {other:?}"),
        },
        other => panic!("expected TapOn, got: {other:?}"),
    }
}

// ----------------------------------------------------------------------
// Webview_eval Step parser tests (5)
// ----------------------------------------------------------------------

#[test]
fn webview_eval_short_string_form() {
    let yaml = r#"appId: com.test.app
---
- webview_eval: "1 + 2"
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::WebViewEval { js, assert_eq } => {
            assert_eq!(js, "1 + 2");
            assert!(assert_eq.is_none());
        }
        other => panic!("expected WebViewEval, got: {other:?}"),
    }
}

#[test]
fn webview_eval_full_form_with_assert_eq() {
    let yaml = r#"appId: com.test.app
---
- webview_eval:
    js: "document.title"
    assert_eq: "smix"
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::WebViewEval { js, assert_eq } => {
            assert_eq!(js, "document.title");
            assert_eq!(assert_eq.as_ref().unwrap(), &serde_json::json!("smix"));
        }
        other => panic!("expected WebViewEval, got: {other:?}"),
    }
}

#[test]
fn webview_eval_assert_eq_number_value() {
    let yaml = r#"appId: com.test.app
---
- webview_eval:
    js: "1 + 2"
    assert_eq: 3
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok");
    match &flow.steps[0] {
        Step::WebViewEval { assert_eq, .. } => {
            assert_eq!(assert_eq.as_ref().unwrap(), &serde_json::json!(3));
        }
        other => panic!("expected WebViewEval, got: {other:?}"),
    }
}

#[test]
fn webview_eval_empty_js_rejected_short() {
    let yaml = r#"appId: com.test.app
---
- webview_eval: ""
"#;
    let err = parse_flow_yaml(yaml).expect_err("empty js must error");
    match err {
        ParseError::InvalidValue { field, .. } => {
            assert!(field.contains("webview_eval"), "field={field}");
        }
        other => panic!("expected InvalidValue, got: {other:?}"),
    }
}

#[test]
fn webview_eval_missing_js_field_rejected() {
    let yaml = r#"appId: com.test.app
---
- webview_eval:
    assert_eq: "x"
"#;
    let err = parse_flow_yaml(yaml).expect_err("missing js must error");
    match err {
        ParseError::InvalidValue { field, .. } => {
            assert!(field.contains("webview_eval.js"), "field={field}");
        }
        other => panic!("expected InvalidValue, got: {other:?}"),
    }
}

#[test]
fn webview_eval_camel_case_key_accepted() {
    let yaml = r#"appId: com.test.app
---
- webviewEval: "42"
"#;
    let flow = parse_flow_yaml(yaml).expect("parse ok (camelCase)");
    match &flow.steps[0] {
        Step::WebViewEval { js, .. } => assert_eq!(js, "42"),
        other => panic!("expected WebViewEval, got: {other:?}"),
    }
}

// -- `expect: { visible: ... }` shape tests ------------------------------
//
// `smix migrate` rewrites `extendedWaitUntil: { visible: X, timeout: N }`
// to `expect: { visible: X, timeoutMs: N }`. These tests pin every shape
// the migrate tool and the documented shorthand can emit so that
// migrate + parse round-trip byte-cleanly.

#[test]
fn parse_expect_visible_text_with_timeout_is_extended_wait_until() {
    let yaml = r#"appId: com.test.app
---
- expect:
    visible:
      text: 'Force Update'
    timeoutMs: 8000
"#;
    let flow = parse_flow_yaml(yaml).expect("parse expect { visible: { text }, timeoutMs }");
    match &flow.steps[0] {
        Step::ExtendedWaitUntil {
            selector,
            timeout_ms,
            expect_visible,
        } => {
            assert_eq!(*timeout_ms, 8000);
            assert!(*expect_visible);
            assert_eq!(selector, &text_selector(text_to_pattern("Force Update")));
        }
        other => panic!("expected ExtendedWaitUntil, got: {other:?}"),
    }
}

#[test]
fn parse_expect_visible_id_with_timeout_is_extended_wait_until() {
    let yaml = r#"appId: com.test.app
---
- expect:
    visible:
      id: 'btn-continue'
    timeoutMs: 3000
"#;
    let flow = parse_flow_yaml(yaml).expect("parse expect { visible: { id }, timeoutMs }");
    match &flow.steps[0] {
        Step::ExtendedWaitUntil {
            selector,
            timeout_ms,
            expect_visible,
        } => {
            assert_eq!(*timeout_ms, 3000);
            assert!(*expect_visible);
            assert_eq!(selector, &id_selector("btn-continue"));
        }
        other => panic!("expected ExtendedWaitUntil, got: {other:?}"),
    }
}

#[test]
fn parse_expect_visible_map_without_timeout_is_assert_visible() {
    // The `expect: { visible: <selector> }` shape without `timeoutMs`
    // — no wait, just an assertion.
    let yaml = r#"appId: com.test.app
---
- expect:
    visible:
      text: 'Dashboard'
"#;
    let flow = parse_flow_yaml(yaml).expect("parse expect { visible: {...} } no timeout");
    match &flow.steps[0] {
        Step::AssertVisible { selector } => {
            assert_eq!(selector, &text_selector(text_to_pattern("Dashboard")));
        }
        other => panic!("expected AssertVisible, got: {other:?}"),
    }
}

#[test]
fn parse_expect_visible_flow_style_map_with_timeout() {
    // Flow-style: `visible: { text: 'X' }` on one line inside expect.
    let yaml = r#"appId: com.test.app
---
- expect:
    visible: { text: 'Force Update' }
    timeoutMs: 8000
"#;
    let flow = parse_flow_yaml(yaml).expect("parse expect flow-style");
    match &flow.steps[0] {
        Step::ExtendedWaitUntil {
            selector,
            timeout_ms,
            ..
        } => {
            assert_eq!(*timeout_ms, 8000);
            assert_eq!(selector, &text_selector(text_to_pattern("Force Update")));
        }
        other => panic!("expected ExtendedWaitUntil, got: {other:?}"),
    }
}

#[test]
fn parse_expect_not_visible_with_timeout_is_extended_wait_until_false() {
    let yaml = r#"appId: com.test.app
---
- expect:
    notVisible:
      id: 'spinner'
    timeoutMs: 5000
"#;
    let flow = parse_flow_yaml(yaml).expect("parse expect { notVisible, timeoutMs }");
    match &flow.steps[0] {
        Step::ExtendedWaitUntil {
            selector,
            timeout_ms,
            expect_visible,
        } => {
            assert_eq!(*timeout_ms, 5000);
            assert!(!*expect_visible);
            assert_eq!(selector, &id_selector("spinner"));
        }
        other => panic!("expected ExtendedWaitUntil (expect_visible=false), got: {other:?}"),
    }
}

#[test]
fn parse_expect_not_visible_without_timeout_is_assert_not_visible() {
    let yaml = r#"appId: com.test.app
---
- expect:
    notVisible:
      id: 'spinner'
"#;
    let flow = parse_flow_yaml(yaml).expect("parse expect { notVisible: {...} } no timeout");
    match &flow.steps[0] {
        Step::AssertNotVisible { selector } => {
            assert_eq!(selector, &id_selector("spinner"));
        }
        other => panic!("expected AssertNotVisible, got: {other:?}"),
    }
}

#[test]
fn parse_expect_bare_string_still_shorthand_assert_visible() {
    // `expect: "X"` bare string — historical maestro shorthand for
    // `assertVisible: "X"`. Must still work after the visible-shape fix.
    let yaml = r#"appId: com.test.app
---
- expect: 'Dashboard'
"#;
    let flow = parse_flow_yaml(yaml).expect("parse expect bare string");
    match &flow.steps[0] {
        Step::AssertVisible { selector } => {
            assert_eq!(selector, &text_selector(text_to_pattern("Dashboard")));
        }
        other => panic!("expected AssertVisible, got: {other:?}"),
    }
}

#[test]
fn parse_expect_top_level_text_still_works() {
    // `expect: { text: 'X' }` — the historical maestro-alias form.
    // Distinct from the new `expect: { visible: { text: 'X' } }`.
    let yaml = r#"appId: com.test.app
---
- expect:
    text: 'Dashboard'
"#;
    let flow = parse_flow_yaml(yaml).expect("parse expect { text }");
    match &flow.steps[0] {
        Step::AssertVisible { selector } => {
            assert_eq!(selector, &text_selector(text_to_pattern("Dashboard")));
        }
        other => panic!("expected AssertVisible, got: {other:?}"),
    }
}

// ClearAppData parser tests. Bare + map form both
// accepted; map form extracts launchArgs / launchEnv (or the shorthand
// args / env aliases). Wiring is verified end-to-end at the runner
// route (real-sim gate), but the parse shape is locked here so a
// regression to unit form breaks tests before it breaks consumers.

#[test]
fn parse_clear_app_data_bare_yields_empty_options() {
    let yaml = "appId: com.test.app\n---\n- clearAppData\n";
    let flow = parse_flow_yaml(yaml).expect("parse bare clearAppData");
    match &flow.steps[0] {
        Step::ClearAppData {
            launch_args,
            launch_env,
        } => {
            assert!(launch_args.is_empty());
            assert!(launch_env.is_empty());
        }
        other => panic!("expected ClearAppData, got: {other:?}"),
    }
}

#[test]
fn parse_clear_app_data_with_launch_args_and_env() {
    let yaml = r#"appId: com.test.app
---
- clearAppData:
    launchArgs:
      - "-EXInternalMetroPort"
      - "8081"
    launchEnv:
      EX_DEV_CLIENT_METRO_URL: "http://localhost:8081"
"#;
    let flow = parse_flow_yaml(yaml).expect("parse clearAppData with args + env");
    match &flow.steps[0] {
        Step::ClearAppData {
            launch_args,
            launch_env,
        } => {
            assert_eq!(
                launch_args,
                &vec!["-EXInternalMetroPort".to_string(), "8081".to_string()]
            );
            assert_eq!(
                launch_env
                    .get("EX_DEV_CLIENT_METRO_URL")
                    .map(String::as_str),
                Some("http://localhost:8081")
            );
        }
        other => panic!("expected ClearAppData, got: {other:?}"),
    }
}

#[test]
fn parse_clear_app_data_accepts_short_args_and_env_aliases() {
    // Shorthand `args` / `env` accepted alongside canonical
    // `launchArgs` / `launchEnv` so yaml stays concise.
    let yaml = r#"appId: com.test.app
---
- clearAppData:
    args: ["-Foo"]
    env:
      BAR: "baz"
"#;
    let flow = parse_flow_yaml(yaml).expect("parse clearAppData short form");
    match &flow.steps[0] {
        Step::ClearAppData {
            launch_args,
            launch_env,
        } => {
            assert_eq!(launch_args, &vec!["-Foo".to_string()]);
            assert_eq!(launch_env.get("BAR").map(String::as_str), Some("baz"));
        }
        other => panic!("expected ClearAppData, got: {other:?}"),
    }
}

// ResetAppData parser shape locks.

#[test]
fn parse_reset_app_data_short_form_url_string() {
    let yaml = "appId: com.test.app\n---\n- resetAppData: 'myapp://dev-mutate?action=reset'\n";
    let flow = parse_flow_yaml(yaml).expect("parse short-form resetAppData");
    match &flow.steps[0] {
        Step::ResetAppData {
            url,
            wait_for,
            timeout_ms,
        } => {
            assert_eq!(url, "myapp://dev-mutate?action=reset");
            assert!(wait_for.is_none());
            assert_eq!(*timeout_ms, 5000);
        }
        other => panic!("expected ResetAppData, got: {other:?}"),
    }
}

#[test]
fn parse_reset_app_data_map_form_with_log_line_pattern() {
    use smix_sdk::ResetAppDataWaitFor;
    let yaml = r#"appId: com.test.app
---
- resetAppData:
    via: url-scheme
    url: 'myapp://dev-mutate?action=reset'
    waitFor:
      logLinePattern: '\[myapp-dev\] reset-complete token='
    timeoutMs: 7000
"#;
    let flow = parse_flow_yaml(yaml).expect("parse map-form resetAppData");
    match &flow.steps[0] {
        Step::ResetAppData {
            url,
            wait_for,
            timeout_ms,
        } => {
            assert_eq!(url, "myapp://dev-mutate?action=reset");
            assert_eq!(*timeout_ms, 7000);
            match wait_for {
                Some(ResetAppDataWaitFor::LogLinePattern(p)) => {
                    assert_eq!(p, r"\[myapp-dev\] reset-complete token=");
                }
                other => panic!("expected LogLinePattern, got: {other:?}"),
            }
        }
        other => panic!("expected ResetAppData, got: {other:?}"),
    }
}

#[test]
fn parse_reset_app_data_map_form_with_sleep_fallback() {
    use smix_sdk::ResetAppDataWaitFor;
    let yaml = r#"appId: com.test.app
---
- resetAppData:
    url: 'myapp://dev-mutate?action=reset'
    waitFor:
      sleepMs: 500
"#;
    let flow = parse_flow_yaml(yaml).expect("parse resetAppData with sleep waitFor");
    match &flow.steps[0] {
        Step::ResetAppData { wait_for, .. } => match wait_for {
            Some(ResetAppDataWaitFor::Sleep(ms)) => assert_eq!(*ms, 500),
            other => panic!("expected Sleep(500), got {other:?}"),
        },
        other => panic!("expected ResetAppData, got {other:?}"),
    }
}

// WaitForAnimationToEnd numeric override.

#[test]
fn parse_wait_for_animation_to_end_bare_default_400ms() {
    let yaml = "appId: com.test.app\n---\n- waitForAnimationToEnd\n";
    let flow = parse_flow_yaml(yaml).expect("parse bare waitForAnimationToEnd");
    match &flow.steps[0] {
        Step::WaitForAnimationToEnd { ceiling_ms } => assert_eq!(*ceiling_ms, 400),
        other => panic!("expected WaitForAnimationToEnd, got: {other:?}"),
    }
}

#[test]
fn parse_wait_for_animation_to_end_numeric_override() {
    let yaml = "appId: com.test.app\n---\n- waitForAnimationToEnd: 750\n";
    let flow = parse_flow_yaml(yaml).expect("parse numeric waitForAnimationToEnd");
    match &flow.steps[0] {
        Step::WaitForAnimationToEnd { ceiling_ms } => assert_eq!(*ceiling_ms, 750),
        other => panic!("expected WaitForAnimationToEnd, got: {other:?}"),
    }
}

// ExtendedWaitUntil.visible now accepts every selector
// key that tapOn does (docs promise; parser was rejecting).
#[test]
fn parse_extended_wait_until_visible_ocr_text() {
    let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible:
      ocrText: '1'
    timeout: 10000
";
    let flow = parse_flow_yaml(yaml).expect("visible:{ocrText} must parse");
    match &flow.steps[0] {
        Step::ExtendedWaitUntil {
            selector: Selector::OcrText { ocr_text, .. },
            timeout_ms: 10000,
            expect_visible: true,
        } => assert_eq!(ocr_text, "1"),
        other => panic!("expected ExtendedWaitUntil with OcrText selector, got: {other:?}"),
    }
}

#[test]
fn parse_extended_wait_until_visible_role_name() {
    let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible:
      role: button
      name: 'OK'
    timeout: 5000
";
    let flow = parse_flow_yaml(yaml).expect("visible:{role,name} must parse");
    match &flow.steps[0] {
        Step::ExtendedWaitUntil {
            selector:
                Selector::Role {
                    role,
                    name: Some(Pattern::Text(t)),
                    ..
                },
            timeout_ms: 5000,
            expect_visible: true,
        } => {
            assert_eq!(*role, Role::Button);
            assert_eq!(t, "OK");
        }
        other => panic!("expected ExtendedWaitUntil with Role selector, got: {other:?}"),
    }
}

#[test]
fn parse_extended_wait_until_visible_label() {
    let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible:
      label: 'Settings'
    timeout: 3000
";
    let flow = parse_flow_yaml(yaml).expect("visible:{label} must parse");
    match &flow.steps[0] {
        Step::ExtendedWaitUntil {
            selector: Selector::Label { label, .. },
            expect_visible: true,
            ..
        } => assert_eq!(label, "Settings"),
        other => panic!("expected ExtendedWaitUntil with Label selector, got: {other:?}"),
    }
}

// TapOn: {role, name} + tapOn: {label}
#[test]
fn parse_tap_on_role_name() {
    let yaml = "appId: com.test.app\n---\n- tapOn:\n    role: button\n    name: 'Submit'\n";
    let flow = parse_flow_yaml(yaml).expect("tapOn:{role,name} must parse");
    match &flow.steps[0] {
        Step::TapOn {
            selector:
                Selector::Role {
                    role,
                    name: Some(Pattern::Text(t)),
                    ..
                },
            ..
        } => {
            assert_eq!(*role, Role::Button);
            assert_eq!(t, "Submit");
        }
        other => panic!("expected TapOn with Role selector, got: {other:?}"),
    }
}

#[test]
fn parse_tap_on_role_lowercase_alias() {
    // Docs promise `role: textfield` (lowercase); wire is camelCase.
    // Parser accepts both.
    let yaml = "appId: com.test.app\n---\n- tapOn:\n    role: textfield\n";
    let flow = parse_flow_yaml(yaml).expect("lowercase `textfield` must alias to TextField");
    match &flow.steps[0] {
        Step::TapOn {
            selector: Selector::Role {
                role, name: None, ..
            },
            ..
        } => assert_eq!(*role, Role::TextField),
        other => panic!("expected TapOn with Role::TextField, got: {other:?}"),
    }
}

#[test]
fn parse_tap_on_role_unknown_errors_actionably() {
    let yaml = "appId: com.test.app\n---\n- tapOn:\n    role: notarole\n";
    let err = parse_flow_yaml(yaml).expect_err("unknown role must error");
    let msg = format!("{err:?}");
    assert!(msg.contains("unknown role"), "err={msg}");
    assert!(
        msg.contains("button"),
        "err message must list accepted roles: {msg}"
    );
}

#[test]
fn parse_tap_on_label() {
    let yaml = "appId: com.test.app\n---\n- tapOn:\n    label: 'Home tab'\n";
    let flow = parse_flow_yaml(yaml).expect("tapOn:{label} must parse");
    match &flow.steps[0] {
        Step::TapOn {
            selector: Selector::Label { label, .. },
            ..
        } => assert_eq!(label, "Home tab"),
        other => panic!("expected TapOn with Label selector, got: {other:?}"),
    }
}

// Bare-string auto-OCR opt-in via SMIX_AUTO_OCR_FALLBACK.
//
// These tests use the thread-local override seam
// (`set_auto_ocr_fallback_override`) instead of mutating process env.
// Process env is global while Cargo runs tests on parallel threads:
// the old set_var/restore approach raced every OTHER test parsing a
// bare-string selector on a sibling thread, flipping their parse
// output between Text and Fallback mid-run (observed as flaky
// failures on parse_ensure_login_with_runflow_when_clause /
// parse_launch_warm_extras). The thread-local pins the decision for
// this test's thread only. `val` maps to truthiness the same way the
// env would.
fn with_env<F: FnOnce()>(_key: &str, val: Option<&str>, f: F) {
    let pinned = matches!(val, Some("1" | "true" | "TRUE" | "yes"));
    smix_adapter_maestro::set_auto_ocr_fallback_override(Some(pinned));
    let unwind = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
    smix_adapter_maestro::set_auto_ocr_fallback_override(None);
    if let Err(e) = unwind {
        std::panic::resume_unwind(e);
    }
}

#[test]
fn parse_visible_bare_string_default_stays_text() {
    with_env("SMIX_AUTO_OCR_FALLBACK", None, || {
        let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible: 'Log in'
    timeout: 5000
";
        let flow = parse_flow_yaml(yaml).expect("bare-string default parse");
        match &flow.steps[0] {
            Step::ExtendedWaitUntil {
                selector:
                    Selector::Text {
                        text: Pattern::Text(t),
                        ..
                    },
                ..
            } => assert_eq!(t, "Log in"),
            other => panic!("expected Text selector without env, got {other:?}"),
        }
    });
}

#[test]
fn parse_visible_bare_string_with_env_lifts_to_fallback() {
    with_env("SMIX_AUTO_OCR_FALLBACK", Some("1"), || {
        let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible: 'Log in'
    timeout: 5000
";
        let flow = parse_flow_yaml(yaml).expect("bare-string parse with env");
        match &flow.steps[0] {
            Step::ExtendedWaitUntil {
                selector: Selector::Fallback { fallback },
                ..
            } => {
                assert_eq!(fallback.len(), 2, "expected 2-layer fallback");
                match &fallback[0] {
                    Selector::Text {
                        text: Pattern::Text(t),
                        ..
                    } => assert_eq!(t, "Log in"),
                    other => panic!("layer 0 should be Text, got {other:?}"),
                }
                match &fallback[1] {
                    Selector::OcrText {
                        ocr_text, locales, ..
                    } => {
                        assert_eq!(ocr_text, "Log in");
                        assert!(locales.is_empty());
                    }
                    other => panic!("layer 1 should be OcrText, got {other:?}"),
                }
            }
            other => panic!("expected Fallback with env, got {other:?}"),
        }
    });
}

#[test]
fn parse_visible_bare_string_with_env_true() {
    with_env("SMIX_AUTO_OCR_FALLBACK", Some("true"), || {
        let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible: 'Login'
    timeout: 5000
";
        let flow = parse_flow_yaml(yaml).expect("bare parse with env=true");
        assert!(matches!(
            &flow.steps[0],
            Step::ExtendedWaitUntil {
                selector: Selector::Fallback { .. },
                ..
            }
        ));
    });
}

#[test]
fn parse_visible_bare_string_with_env_zero_stays_text() {
    // Explicit off — `0` shouldn't lift. Same as unset.
    with_env("SMIX_AUTO_OCR_FALLBACK", Some("0"), || {
        let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible: 'Login'
    timeout: 5000
";
        let flow = parse_flow_yaml(yaml).expect("parse");
        assert!(matches!(
            &flow.steps[0],
            Step::ExtendedWaitUntil {
                selector: Selector::Text { .. },
                ..
            }
        ));
    });
}

// `runFlow.when.notVisible` inverse gate parses.
#[test]
fn parse_run_flow_conditional_when_not_visible() {
    let yaml = concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    when:\n",
        "      notVisible: 'qa-bubble'\n",
        "    file: ../subflows/enter-qa.yaml\n",
    );
    let flow = parse_flow_yaml(yaml).expect("parse when.notVisible + file");
    match &flow.steps[0] {
        Step::RunFlowConditional {
            file,
            when:
                Some(smix_adapter_maestro::FlowCondition {
                    visible: None,
                    not_visible: Some(sel),
                    ..
                }),
            as_name: None,
            ..
        } => {
            assert!(file.ends_with("enter-qa.yaml"));
            match sel {
                Selector::Text {
                    text: Pattern::Text(t),
                    ..
                } => assert_eq!(t, "qa-bubble"),
                other => panic!("expected Text selector, got {other:?}"),
            }
        }
        other => panic!("expected RunFlowConditional with when_not_visible, got: {other:?}"),
    }
}

#[test]
fn parse_run_flow_inline_when_not_visible() {
    let yaml = concat!(
        "appId: com.t.r\n",
        "---\n",
        "- runFlow:\n",
        "    when:\n",
        "      notVisible:\n",
        "        id: 'qa-bubble'\n",
        "    commands:\n",
        "      - tapOn: 'Enter'\n",
    );
    let flow = parse_flow_yaml(yaml).expect("parse when.notVisible + inline");
    match &flow.steps[0] {
        Step::RunFlowInline {
            when:
                Some(smix_adapter_maestro::FlowCondition {
                    visible: None,
                    not_visible: Some(Selector::Id { id, .. }),
                    ..
                }),
            steps,
            ..
        } => {
            assert_eq!(id, "qa-bubble");
            assert_eq!(steps.len(), 1);
        }
        other => panic!("expected RunFlowInline with when_not_visible, got: {other:?}"),
    }
}

// Regex-OR `A|B` auto-lift splits per alternative on OCR tier.

#[test]
fn parse_visible_bare_string_regex_or_splits_ocr_per_alternative() {
    with_env("SMIX_AUTO_OCR_FALLBACK", Some("1"), || {
        let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible: 'Log in|Device'
    timeout: 5000
";
        let flow = parse_flow_yaml(yaml).expect("regex-OR bare with env");
        match &flow.steps[0] {
            Step::ExtendedWaitUntil {
                selector: Selector::Fallback { fallback },
                ..
            } => {
                // Expected: [Text(regex A|B), OcrText(A), OcrText(B)]
                assert_eq!(
                    fallback.len(),
                    3,
                    "expected 3 tiers, got {}",
                    fallback.len()
                );
                match &fallback[0] {
                    Selector::Text {
                        text: Pattern::Regex { regex, .. },
                        ..
                    } => {
                        assert_eq!(regex, "Log in|Device");
                    }
                    other => panic!("tier 0 should be Text regex, got {other:?}"),
                }
                match &fallback[1] {
                    Selector::OcrText { ocr_text, .. } => assert_eq!(ocr_text, "Log in"),
                    other => panic!("tier 1 should be OcrText 'Log in', got {other:?}"),
                }
                match &fallback[2] {
                    Selector::OcrText { ocr_text, .. } => assert_eq!(ocr_text, "Device"),
                    other => panic!("tier 2 should be OcrText 'Device', got {other:?}"),
                }
            }
            other => panic!("expected Fallback, got {other:?}"),
        }
    });
}

#[test]
fn parse_visible_bare_string_no_pipe_unchanged() {
    // No `|` = single OcrText tier.
    with_env("SMIX_AUTO_OCR_FALLBACK", Some("1"), || {
        let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible: 'Sign In'
    timeout: 5000
";
        let flow = parse_flow_yaml(yaml).expect("no-pipe bare with env");
        match &flow.steps[0] {
            Step::ExtendedWaitUntil {
                selector: Selector::Fallback { fallback },
                ..
            } => {
                assert_eq!(fallback.len(), 2);
                match &fallback[0] {
                    Selector::Text {
                        text: Pattern::Text(t),
                        ..
                    } => assert_eq!(t, "Sign In"),
                    other => panic!("tier 0 should be Text literal, got {other:?}"),
                }
                match &fallback[1] {
                    Selector::OcrText { ocr_text, .. } => assert_eq!(ocr_text, "Sign In"),
                    other => panic!("tier 1 should be OcrText literal, got {other:?}"),
                }
            }
            other => panic!("expected Fallback, got {other:?}"),
        }
    });
}

#[test]
fn parse_visible_bare_string_three_alternatives() {
    with_env("SMIX_AUTO_OCR_FALLBACK", Some("1"), || {
        let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible: 'A|B|C'
    timeout: 5000
";
        let flow = parse_flow_yaml(yaml).expect("3-alt bare with env");
        match &flow.steps[0] {
            Step::ExtendedWaitUntil {
                selector: Selector::Fallback { fallback },
                ..
            } => {
                // [Text(regex A|B|C), OcrText(A), OcrText(B), OcrText(C)]
                assert_eq!(fallback.len(), 4);
                let ocrs: Vec<&str> = fallback[1..]
                    .iter()
                    .map(|s| match s {
                        Selector::OcrText { ocr_text, .. } => ocr_text.as_str(),
                        _ => panic!("expected OcrText"),
                    })
                    .collect();
                assert_eq!(ocrs, vec!["A", "B", "C"]);
            }
            other => panic!("expected Fallback, got {other:?}"),
        }
    });
}

#[test]
fn parse_visible_bare_string_empty_alternatives_filtered() {
    // `'|A|'` → one alternative "A", not three empties.
    with_env("SMIX_AUTO_OCR_FALLBACK", Some("1"), || {
        let yaml = "\
appId: com.test.app
---
- extendedWaitUntil:
    visible: '|A|'
    timeout: 5000
";
        let flow = parse_flow_yaml(yaml).expect("empty-alt bare");
        match &flow.steps[0] {
            Step::ExtendedWaitUntil {
                selector: Selector::Fallback { fallback },
                ..
            } => {
                // [Text(regex |A|), OcrText(A)] — empties filtered
                assert_eq!(fallback.len(), 2);
                match &fallback[1] {
                    Selector::OcrText { ocr_text, .. } => assert_eq!(ocr_text, "A"),
                    other => panic!("expected OcrText 'A', got {other:?}"),
                }
            }
            other => panic!("expected Fallback, got {other:?}"),
        }
    });
}

// Maestro-canonical map form `waitForAnimationToEnd: { timeout: N }`.
// The docs show this shape; the parser originally rejected it.
#[test]
fn parse_wait_for_animation_to_end_map_timeout_form() {
    let yaml = "appId: com.test.app\n---\n- waitForAnimationToEnd:\n    timeout: 5000\n";
    let flow = parse_flow_yaml(yaml).expect("parse map-form waitForAnimationToEnd");
    match &flow.steps[0] {
        Step::WaitForAnimationToEnd { ceiling_ms } => assert_eq!(*ceiling_ms, 5000),
        other => panic!("expected WaitForAnimationToEnd, got: {other:?}"),
    }
}

#[test]
fn parse_wait_for_animation_to_end_map_missing_timeout_rejects() {
    let yaml = "appId: com.test.app\n---\n- waitForAnimationToEnd:\n    seconds: 5\n";
    let err = parse_flow_yaml(yaml).expect_err("map without `timeout` key must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("timeout"),
        "err should name the expected key: {msg}"
    );
}

// `tapOn: { dispatch: xcui | daemonProxy }` explicit
// dispatch-mechanism override. Generic replacement for the old
// fixture-namespace auto-routing; docs used to promise a `mode:` key
// that never parsed.
#[test]
fn parse_tap_on_dispatch_xcui() {
    let yaml = "appId: com.t\n---\n- tapOn:\n    id: 'modal-dismiss-btn'\n    dispatch: xcui\n";
    let flow = parse_flow_yaml(yaml).expect("dispatch: xcui must parse");
    match &flow.steps[0] {
        Step::TapOn {
            selector: Selector::Id { id, .. },
            dispatch: Some(smix_adapter_maestro::TapDispatch::Xcui),
            ..
        } => assert_eq!(id, "modal-dismiss-btn"),
        other => panic!("expected TapOn with dispatch xcui, got: {other:?}"),
    }
}

#[test]
fn parse_tap_on_dispatch_daemon_proxy() {
    let yaml = "appId: com.t\n---\n- tapOn:\n    id: 'btn-login'\n    dispatch: daemonProxy\n";
    let flow = parse_flow_yaml(yaml).expect("dispatch: daemonProxy must parse");
    assert!(matches!(
        &flow.steps[0],
        Step::TapOn {
            dispatch: Some(smix_adapter_maestro::TapDispatch::DaemonProxy),
            ..
        }
    ));
}

#[test]
fn parse_tap_on_dispatch_unknown_rejects() {
    let yaml = "appId: com.t\n---\n- tapOn:\n    id: 'x'\n    dispatch: warp\n";
    let err = parse_flow_yaml(yaml).expect_err("unknown dispatch must error");
    let msg = format!("{err:?}");
    assert!(
        msg.contains("xcui") && msg.contains("daemonProxy"),
        "err lists accepted: {msg}"
    );
}

#[test]
fn parse_tap_on_no_dispatch_defaults_none() {
    let yaml = "appId: com.t\n---\n- tapOn:\n    id: 'x'\n";
    let flow = parse_flow_yaml(yaml).expect("parse");
    assert!(matches!(&flow.steps[0], Step::TapOn { dispatch: None, .. }));
}

// `anchorRelative:` alias for `anchored:` (docs promised
// the alias; the parser originally read only `anchored`).
#[test]
fn parse_tap_on_anchor_relative_alias() {
    let yaml = "\
appId: com.t
---
- tapOn:
    anchorRelative:
      anchor: { id: 'header' }
      dx: 0.45
      dy: 0.0
";
    let flow = parse_flow_yaml(yaml).expect("anchorRelative alias must parse");
    match &flow.steps[0] {
        Step::TapOn {
            selector: Selector::AnchorRelative { dx, dy, .. },
            ..
        } => {
            assert!((dx - 0.45).abs() < 1e-9);
            assert!(dy.abs() < 1e-9);
        }
        other => panic!("expected TapOn AnchorRelative, got: {other:?}"),
    }
}

// `clearUserDefaults: { keys: [...], bundleId?: ... }`.
#[test]
fn parse_clear_user_defaults_keys_only() {
    let yaml = "\
appId: com.t
---
- clearUserDefaults:
    keys:
      - 'expo.devlauncher.pendingDeepLink'
      - 'another.key'
";
    let flow = parse_flow_yaml(yaml).expect("clearUserDefaults must parse");
    match &flow.steps[0] {
        Step::ClearUserDefaults {
            keys,
            bundle_id: None,
        } => {
            assert_eq!(keys.len(), 2);
            assert_eq!(keys[0], "expo.devlauncher.pendingDeepLink");
        }
        other => panic!("expected ClearUserDefaults, got: {other:?}"),
    }
}

#[test]
fn parse_clear_user_defaults_with_bundle_override() {
    let yaml = "\
appId: com.t
---
- clearUserDefaults:
    keys: ['k1']
    bundleId: 'com.other.app'
";
    let flow = parse_flow_yaml(yaml).expect("parse with bundleId");
    match &flow.steps[0] {
        Step::ClearUserDefaults {
            bundle_id: Some(b), ..
        } => assert_eq!(b, "com.other.app"),
        other => panic!("expected bundle override, got: {other:?}"),
    }
}

#[test]
fn parse_clear_user_defaults_empty_keys_rejects() {
    let yaml = "appId: com.t\n---\n- clearUserDefaults:\n    keys: []\n";
    let err = parse_flow_yaml(yaml).expect_err("empty keys must error");
    assert!(
        format!("{err:?}").contains("keys"),
        "err names keys: {err:?}"
    );
}

#[test]
fn parse_clear_user_defaults_missing_keys_rejects() {
    let yaml = "appId: com.t\n---\n- clearUserDefaults:\n    bundleId: 'x'\n";
    let err = parse_flow_yaml(yaml).expect_err("missing keys must error");
    assert!(
        format!("{err:?}").contains("keys"),
        "err names keys: {err:?}"
    );
}

/// maestro's directional swipe form parses and desugars to the finger
/// coordinates maestro documents — it used to be rejected with
/// MissingField("swipe.from").
#[test]
fn swipe_direction_form_desugars_to_finger_coords() {
    let flow =
        smix_adapter_maestro::parse_flow_yaml("appId: x\n---\n- swipe:\n    direction: UP\n")
            .expect("direction form parses");
    match &flow.steps[0] {
        smix_adapter_maestro::Step::Swipe { from, to } => {
            assert_eq!(*from, (0.5, 0.7));
            assert_eq!(*to, (0.5, 0.3));
        }
        other => panic!("expected Swipe, got {other:?}"),
    }
    assert!(
        smix_adapter_maestro::parse_flow_yaml("appId: x\n---\n- swipe:\n    direction: sideways\n")
            .is_err(),
        "unknown direction must be rejected"
    );
}

// ---------------------------------------------------------------------
// Conditions: `runFlow.when` and `repeat.while` share one shape, and the
// mappings around them name every key they read. A key nobody reads is
// a parse error, not a silently unconditional block (from a consumer's
// Android round, 2026-09-22: `when: { platform: Android }` ran on iOS).
// ---------------------------------------------------------------------

use smix_adapter_maestro::{BlockOptions, CONDITION_KEYS, ConditionPlatform, FlowCondition};

fn visible_only(sel: Selector) -> FlowCondition {
    FlowCondition {
        platform: None,
        visible: Some(sel),
        not_visible: None,
        script: None,
        label: None,
    }
}

fn only_step(yaml_body: &str) -> Step {
    let yaml = format!("appId: com.t.r\n---\n{yaml_body}");
    let mut flow = parse_flow_yaml(&yaml).unwrap_or_else(|e| panic!("parse {yaml_body}: {e:?}"));
    assert_eq!(flow.steps.len(), 1, "{yaml_body}");
    flow.steps.remove(0)
}

fn parse_err(yaml_body: &str) -> (String, String) {
    let yaml = format!("appId: com.t.r\n---\n{yaml_body}");
    match parse_flow_yaml(&yaml) {
        Err(ParseError::InvalidValue { field, reason }) => (field, reason),
        other => panic!("expected InvalidValue for {yaml_body}, got {other:?}"),
    }
}

fn inline_when(step: Step) -> FlowCondition {
    match step {
        Step::RunFlowInline { when: Some(c), .. } => c,
        other => panic!("expected RunFlowInline with a condition, got {other:?}"),
    }
}

#[test]
fn condition_platform_is_read_case_insensitively() {
    for (spelling, want) in [
        ("Android", ConditionPlatform::Android),
        ("android", ConditionPlatform::Android),
        ("ANDROID", ConditionPlatform::Android),
        ("iOS", ConditionPlatform::Ios),
        ("ios", ConditionPlatform::Ios),
        ("Web", ConditionPlatform::Web),
    ] {
        let c = inline_when(only_step(&format!(
            "- runFlow:\n    when:\n      platform: {spelling}\n    commands:\n      - tapOn: x\n"
        )));
        assert_eq!(c.platform, Some(want), "{spelling}");
    }
}

#[test]
fn condition_platform_outside_the_three_is_refused_by_name() {
    let (field, reason) = parse_err(
        "- runFlow:\n    when:\n      platform: Linux\n    commands:\n      - tapOn: x\n",
    );
    assert_eq!(field, "runFlow.when.platform");
    assert!(reason.contains("Android, iOS, Web"), "{reason}");
}

#[test]
fn condition_true_is_read_in_all_three_yaml_spellings() {
    for (spelling, want) in [
        ("true: ${output.x == 'y'}", "${output.x == 'y'}"),
        ("\"true\": false", "false"),
        ("true: true", "true"),
    ] {
        let c = inline_when(only_step(&format!(
            "- runFlow:\n    when:\n      {spelling}\n    commands:\n      - tapOn: x\n"
        )));
        assert_eq!(c.script.as_deref(), Some(want), "{spelling}");
    }
}

#[test]
fn condition_keys_combine_instead_of_excluding_each_other() {
    let c = inline_when(only_step(
        "- runFlow:\n    when:\n      platform: Android\n      visible: Allow\n    commands:\n      - tapOn: x\n",
    ));
    assert_eq!(c.platform, Some(ConditionPlatform::Android));
    assert_eq!(c.visible, Some(text_selector(Pattern::Text("Allow".into()))));
    let c = inline_when(only_step(
        "- runFlow:\n    when:\n      visible: a\n      notVisible: b\n    commands:\n      - tapOn: x\n",
    ));
    assert_eq!(c.visible, Some(text_selector(Pattern::Text("a".into()))));
    assert_eq!(c.not_visible, Some(text_selector(Pattern::Text("b".into()))));
}

#[test]
fn condition_unknown_key_is_refused_listing_the_known_ones() {
    let (field, reason) = parse_err(
        "- runFlow:\n    when:\n      platfrom: Android\n    commands:\n      - tapOn: x\n",
    );
    assert_eq!(field, "runFlow.when");
    assert!(reason.contains("`platfrom`"), "{reason}");
    for k in CONDITION_KEYS {
        assert!(reason.contains(k), "{reason} should list {k}");
    }
}

#[test]
fn condition_optional_is_refused_saying_maestro_does_not_read_it_either() {
    let (field, reason) = parse_err(
        "- runFlow:\n    when:\n      optional: true\n      visible: a\n    commands:\n      - tapOn: x\n",
    );
    assert_eq!(field, "runFlow.when");
    assert!(reason.contains("`optional`") && reason.contains("maestro"), "{reason}");
}

#[test]
fn condition_with_nothing_to_check_is_refused() {
    for body in ["when: {}", "when:\n      label: x"] {
        let (field, reason) = parse_err(&format!(
            "- runFlow:\n    {body}\n    commands:\n      - tapOn: x\n"
        ));
        assert_eq!(field, "runFlow.when", "{body}");
        assert!(reason.contains("no condition"), "{body}: {reason}");
    }
}

#[test]
fn condition_non_string_key_is_refused() {
    let (field, reason) = parse_err(
        "- runFlow:\n    when:\n      1: x\n    commands:\n      - tapOn: x\n",
    );
    assert_eq!(field, "runFlow.when");
    assert!(reason.contains("`1`"), "{reason}");
}

#[test]
fn condition_run_flow_reads_env_label_and_optional() {
    match only_step(
        "- runFlow:\n    env:\n      A: '1'\n      B: ${x}\n      C: 3\n    label: dismiss promo\n    optional: true\n    commands:\n      - tapOn: x\n",
    ) {
        Step::RunFlowInline { when: None, env, opts, .. } => {
            assert_eq!(
                env,
                vec![("A".into(), "1".into()), ("B".into(), "${x}".into()), ("C".into(), "3".into())]
            );
            assert_eq!(opts, BlockOptions { label: Some("dismiss promo".into()), optional: true });
        }
        other => panic!("{other:?}"),
    }
    match only_step("- runFlow:\n    file: sub.yaml\n    env:\n      A: b\n") {
        Step::RunFlowConditional { env, when: None, .. } => {
            assert_eq!(env, vec![("A".into(), "b".into())]);
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn condition_run_flow_refuses_what_it_does_not_read() {
    let (field, reason) = parse_err("- runFlow:\n    fille: sub.yaml\n");
    assert_eq!(field, "runFlow");
    assert!(reason.contains("`fille`"), "{reason}");
    for k in ["file", "commands", "when", "as", "env", "label", "optional"] {
        assert!(reason.contains(k), "{reason} should list {k}");
    }
    let (field, _) = parse_err("- runFlow:\n    file: sub.yaml\n    env:\n      A: [1, 2]\n");
    assert_eq!(field, "runFlow.env.A");
    let (field, _) = parse_err("- runFlow:\n    file: sub.yaml\n    optional: maybe\n");
    assert_eq!(field, "runFlow.optional");
}

#[test]
fn condition_repeat_reads_label_optional_and_a_while_condition() {
    match only_step(
        "- repeat:\n    while:\n      platform: iOS\n      visible: x\n    label: drain\n    optional: true\n    commands:\n      - tapOn: y\n",
    ) {
        Step::Repeat { while_: Some(c), opts, .. } => {
            assert_eq!(c.platform, Some(ConditionPlatform::Ios));
            assert_eq!(c.visible, Some(text_selector(Pattern::Text("x".into()))));
            assert_eq!(opts, BlockOptions { label: Some("drain".into()), optional: true });
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn condition_repeat_refuses_unknown_keys_outside_and_inside_while() {
    let (field, reason) = parse_err("- repeat:\n    tims: 2\n    commands:\n      - tapOn: y\n");
    assert_eq!(field, "repeat");
    assert!(reason.contains("`tims`"), "{reason}");
    let (field, reason) =
        parse_err("- repeat:\n    while:\n      foo: 1\n    commands:\n      - tapOn: y\n");
    assert_eq!(field, "repeat.while");
    assert!(reason.contains("`foo`"), "{reason}");
}

/// Five: maestro's `YamlCondition` has six fields (`platform`, `visible`,
/// `notVisible`, `true`, `label`, `optional`), and `optional` never reaches
/// its `Condition` (`YamlFluentCommand.toCondition`), so smix refuses it.
#[test]
fn condition_keys_are_exactly_maestros_five() {
    assert_eq!(CONDITION_KEYS.len(), 5);
    for k in ["platform", "visible", "notVisible", "true", "label"] {
        assert!(CONDITION_KEYS.contains(&k), "{k}");
    }
}

#[test]
fn selector_map_refuses_non_string_keys() {
    for body in ["- tapOn:\n    id: x\n    true: y\n", "- tapOn:\n    id: x\n    1: y\n"] {
        let (_, reason) = parse_err(body);
        assert!(reason.contains("unknown key"), "{body}: {reason}");
    }
}

// ---- scrollUntilVisible: every maestro key is read or refused by name ----

use smix_adapter_maestro::SCROLL_UNTIL_VISIBLE_KEYS;
use smix_driver::{DEFAULT_SCROLL_TIMEOUT, ScrollUntil};
use smix_host_coord_resolver::Reach;

fn scroll_step(yaml_body: &str) -> (ScrollUntil, BlockOptions, String) {
    match only_step(yaml_body) {
        Step::ScrollUntilVisible {
            until,
            opts,
            direction,
            ..
        } => (until, opts, direction),
        other => panic!("expected ScrollUntilVisible, got {other:?}"),
    }
}

#[test]
fn scroll_until_visible_defaults_are_maestros() {
    let (until, opts, direction) = scroll_step("- scrollUntilVisible:\n    element: x\n");
    assert_eq!(until, ScrollUntil::default());
    assert_eq!(until.reach, Reach::default());
    assert_eq!(until.timeout, DEFAULT_SCROLL_TIMEOUT);
    assert_eq!(opts, BlockOptions::default());
    assert_eq!(direction, "down");
}

#[test]
fn scroll_until_visible_reads_visibility_centre_timeout_label_and_optional() {
    let (until, opts, direction) = scroll_step(
        "- scrollUntilVisible:\n    element: x\n    direction: UP\n    visibilityPercentage: 50\n    centerElement: true\n    timeout: 30000\n    label: find the row\n    optional: true\n",
    );
    assert!((until.reach.visibility - 0.5).abs() < 1e-9);
    assert!(until.reach.center_element);
    assert_eq!(until.timeout, std::time::Duration::from_secs(30));
    assert_eq!(opts.label.as_deref(), Some("find the row"));
    assert!(opts.optional);
    assert_eq!(direction, "UP");
    // maestro's `timeout` is a string in its schema; both spellings mean ms.
    let (until, _, _) =
        scroll_step("- scrollUntilVisible:\n    element: x\n    timeout: \"30000\"\n");
    assert_eq!(until.timeout, std::time::Duration::from_secs(30));
}

#[test]
fn scroll_until_visible_refuses_the_knobs_smix_swipes_do_not_have() {
    for (key, value) in [("speed", "40"), ("waitToSettleTimeoutMs", "500")] {
        let (field, reason) =
            parse_err(&format!("- scrollUntilVisible:\n    element: x\n    {key}: {value}\n"));
        assert_eq!(field, "scrollUntilVisible");
        assert!(reason.contains(&format!("`{key}`")), "{reason}");
        assert!(!reason.contains("unknown key"), "named, not unknown: {reason}");
    }
}

#[test]
fn scroll_until_visible_refuses_a_visibility_outside_one_to_a_hundred_and_unknown_keys() {
    for v in ["0", "101", "\"half\"", "-5"] {
        let (field, _) = parse_err(&format!(
            "- scrollUntilVisible:\n    element: x\n    visibilityPercentage: {v}\n"
        ));
        assert_eq!(field, "scrollUntilVisible.visibilityPercentage", "{v}");
    }
    let (field, reason) = parse_err("- scrollUntilVisible:\n    elemnt: x\n");
    assert_eq!(field, "scrollUntilVisible");
    assert!(reason.contains("unknown key `elemnt`"), "{reason}");
    let (field, _) = parse_err("- scrollUntilVisible:\n    element: x\n    centerElement: yes please\n");
    assert_eq!(field, "scrollUntilVisible.centerElement");
}

/// Seven: maestro's `YamlScrollUntilVisible` has nine fields, and smix
/// refuses two of them (`speed`, `waitToSettleTimeoutMs`) by name.
#[test]
fn scroll_until_visible_keys_are_maestros_nine_less_the_two_refused() {
    assert_eq!(SCROLL_UNTIL_VISIBLE_KEYS.len(), 7);
    for k in [
        "element",
        "direction",
        "timeout",
        "visibilityPercentage",
        "centerElement",
        "label",
        "optional",
    ] {
        assert!(SCROLL_UNTIL_VISIBLE_KEYS.contains(&k), "{k}");
    }
}

// --- clearLocation: the way back from setLocation / travel -------------

/// A simulated location outlives the flow that set it, and until this
/// verb there was no way to put it back: `DeviceControl` had no
/// `location_clear`, so a phone driven by `setLocation` kept lying to
/// every app on it and smix could not stop. smix's own, not maestro's
/// — maestro has no verb for it.
#[test]
fn clear_location_takes_no_arguments() {
    let yaml = "appId: com.t.r\n---\n- clearLocation\n";
    let flow = parse_flow_yaml(yaml).expect("parse clearLocation");
    assert!(matches!(flow.steps.as_slice(), [Step::ClearLocation]));
}

#[test]
fn clear_location_rejects_arguments_by_name() {
    // It clears the one location a device has. A mapping here would be
    // a reader believing they had scoped it to something.
    let (field, reason) = parse_err("- clearLocation:\n    latitude: 1.0\n");
    assert_eq!(field, "clearLocation");
    assert!(
        reason.contains("takes no arguments"),
        "the refusal does not say what is wrong: {reason}"
    );
}

// --- repeat: times AND while, and runScript's condition ---------------

/// maestro's `repeat` takes both, and runs while BOTH hold.
///
/// `YamlRepeatCommand` has two nullable fields and `Orchestra.kt`'s loop
/// is `while (checkCondition() && counter < maxRuns)` — `times` absent
/// means `Int.MAX_VALUE`, a condition absent means true. smix refused
/// the pair outright, so a flow that wanted "up to five times, while the
/// spinner is there" had no way to say it (open-items H1).
#[test]
fn repeat_takes_a_count_and_a_condition_together() {
    let yaml = "appId: com.t.r\n---\n- repeat:\n    times: 3\n    while:\n      visible: Spinner\n    commands:\n      - tapOn: X\n";
    let flow = parse_flow_yaml(yaml).expect("parse repeat with both");
    match &flow.steps[0] {
        Step::Repeat { times, while_, .. } => {
            assert_eq!(*times, Some(3));
            assert!(while_.is_some(), "the condition was dropped");
        }
        other => panic!("expected Repeat, got {other:?}"),
    }
}

#[test]
fn repeat_with_neither_a_count_nor_a_condition_is_refused() {
    // Both absent is a loop with no way out. maestro's defaults make it
    // `while true, up to Int.MAX_VALUE`; smix refuses instead, because
    // a flow that says neither has said nothing about when to stop.
    let (field, reason) = parse_err("- repeat:\n    commands:\n      - tapOn: X\n");
    assert_eq!(field, "repeat");
    assert!(reason.contains("times") && reason.contains("while"), "{reason}");
}

/// maestro's `runScript` carries a condition; smix refused the mapping
/// form outright, so there was no way to write one (open-items H2).
#[test]
fn run_script_takes_a_condition_and_its_own_keys() {
    let yaml = "appId: com.t.r\n---\n- runScript:\n    file: x.js\n    when:\n      platform: Android\n";
    let flow = parse_flow_yaml(yaml).expect("parse runScript with when");
    match &flow.steps[0] {
        Step::RunScript { when, .. } => {
            let c = when.as_ref().expect("the condition was dropped");
            assert!(c.platform.is_some(), "platform did not survive parsing");
        }
        other => panic!("expected RunScript, got {other:?}"),
    }
}

#[test]
fn run_script_as_a_bare_string_still_means_the_same_thing() {
    let map = parse_flow_yaml("appId: com.t.r\n---\n- runScript:\n    file: x.js\n")
        .expect("mapping form");
    let bare = parse_flow_yaml("appId: com.t.r\n---\n- runScript: x.js\n").expect("string form");
    match (&map.steps[0], &bare.steps[0]) {
        (Step::RunScript { source: a, when: None, .. }, Step::RunScript { source: b, .. }) => {
            assert_eq!(a, b);
        }
        other => panic!("expected two RunScripts, got {other:?}"),
    }
}

#[test]
fn run_script_keys_are_closed() {
    let (field, reason) = parse_err("- runScript:\n    file: x.js\n    fiel: y.js\n");
    assert_eq!(field, "runScript");
    assert!(reason.contains("unknown key `fiel`"), "{reason}");
}

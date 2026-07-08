//! Fixture-driven parser tests. Each test reads a yaml fixture from
//! `tests/fixtures/` and asserts the full [`Flow`] structure round-trips
//! into the Step enum.

use smix_adapter_maestro::{Flow, ParseError, Step, parse_flow_yaml, text_to_pattern};
use smix_selector::{Modifiers, Pattern, Selector};

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
        steps: vec![
            Step::RunFlow("../../subflows/launch-warm.yaml".to_string()),
            Step::RunFlow("../../subflows/ensure-login.yaml".to_string()),
            Step::RunFlow("../../subflows/go-to-alerts.yaml".to_string()),
            // tapOn: "Counting" — short string form, no '|' → Pattern::Text
            Step::TapOn {
                selector: text_selector(Pattern::Text("Counting".to_string())),
                optional: false,
            },
            Step::WaitForAnimationToEnd,
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
            },
            Step::WaitForAnimationToEnd,
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
            },
            // tapOn { text: "Month", optional: true }
            Step::TapOn {
                selector: text_selector(Pattern::Text("Month".to_string())),
                optional: true,
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
        steps: vec![
            // runFlow: { when: { visible: "Log in" }, file: ../subflows/login.yaml }
            Step::RunFlowConditional {
                file: "../subflows/login.yaml".to_string(),
                when_visible: Some(text_selector(Pattern::Text("Log in".to_string()))),
                as_name: None,
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

// v6.8 c1 — `runFlow: { when: { visible }, commands: [...] }` inline form.
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
        steps: vec![Step::RunFlowInline {
            when_visible: Some(text_selector(Pattern::Text("Open in".to_string()))),
            steps: vec![
                Step::TapOn {
                    selector: text_selector(Pattern::Text("Open".to_string())),
                    optional: false,
                },
                Step::WaitForAnimationToEnd,
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
            when_visible: None,
            steps: vec![Step::TapOn {
                selector: text_selector(Pattern::Text("Hello".to_string())),
                optional: false,
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
            Step::WaitForAnimationToEnd,
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
    // v5.2 c4 — evalScript / runScript / repeat / retry 已 wire (parser
    // 接受 + runtime graceful unsupported / 真跑); 旧"未实现 c4"断言已失效.
    // pivot 到 maestro 文档列出但 smix 仍未 wire 的命令作"真未实现"探针:
    //   - `back`:Android-only (matrix #7), iOS sim 不适用; parser 应报
    //     UnsupportedCommand (本 cp parser 未单独列 N/A 命令进 whitelist).
    //   - `addMedia`:Media gap, 仍是 ❌ SDK-gap (matrix #30, 留 v5.2 c5).
    //
    // 这两个真是当前 smix 不支持的 maestro 命令,parser 须 explicit 报
    // UnsupportedCommand (不静默 noop, §13).
    let yaml = "appId: com.test.app\n---\n- back\n";
    let err = parse_flow_yaml(yaml).expect_err("back (Android-only) must error");
    match err {
        ParseError::UnsupportedCommand(cmd) => {
            assert_eq!(cmd, "back");
        }
        other => panic!("expected UnsupportedCommand back, got: {other:?}"),
    }

    // assertWithAI 是 verdict 段明示 out-of-scope (AI 三件套 — user 走
    // claude CLI 自有 key, 不在 SDK 表面 ship), 永不 wire — 真未实现探针.
    let yaml = "appId: com.test.app\n---\n- assertWithAI: \"is logged in\"\n";
    let err = parse_flow_yaml(yaml).expect_err("assertWithAI must error (verdict out-of-scope)");
    match err {
        ParseError::UnsupportedCommand(cmd) => {
            assert_eq!(cmd, "assertWithAI");
        }
        other => panic!("expected UnsupportedCommand assertWithAI, got: {other:?}"),
    }
}

// ----------------------------------------------------------------------
// v5.18 c1 — Selector::LocalizedText DSL parser tests (5 cases)
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
        Step::TapOn { selector, optional } => {
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
        Step::TapOn { selector, optional } => {
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
// v5.19 c1 — Selector::OcrText DSL parser tests (5 cases)
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
        Step::TapOn { selector, optional } => {
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
// v5.20 c1 — Selector::AnchorRelative DSL parser tests (5 cases)
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
        Step::TapOn { selector, optional } => {
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
    // 负方向偏移合法 (yaml 不限正负, adapter clamp 在 [0,1])
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
// v5.20 c2 — Selector::Fallback + Selector::Point DSL parser tests (5)
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
// v5.21 c1b — webview_eval Step parser tests (5)
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

//! Unit tests for smix-selector `match_text`.
//!
//! Covers: 6-field OR + case-insensitive + auto-/i regex flag +
//! empty-string reject + identifier-late-DFS hit.

use smix_screen::{A11yNode, Rect};
use smix_selector::{Pattern, match_text};

fn mk(partial: NodePartial) -> A11yNode {
    A11yNode {
        hittable: None,
        raw_type: "other".into(),
        element_type_raw: 1,
        role: None,
        identifier: partial.identifier,
        label: partial.label,
        title: partial.title,
        placeholder_value: partial.placeholder_value,
        value: partial.value,
        text: partial.text,
        bounds: Rect {
            x: 0.0,
            y: 0.0,
            w: 10.0,
            h: 10.0,
        },
        enabled: true,
        selected: false,
        has_focus: false,
        visible: true,
        children: vec![],
    }
}

#[derive(Default)]
struct NodePartial {
    identifier: Option<String>,
    label: Option<String>,
    title: Option<String>,
    placeholder_value: Option<String>,
    value: Option<String>,
    text: Option<String>,
}

// ---- string pattern -----------------------------------------------------

#[test]
fn text_string_label_hit_case_insensitive() {
    let n = mk(NodePartial {
        label: Some("Settings".into()),
        ..Default::default()
    });
    assert!(match_text(&n, &Pattern::text("settings")));
    assert!(match_text(&n, &Pattern::text("Settings")));
    assert!(match_text(&n, &Pattern::text("SETTINGS")));
}

#[test]
fn text_string_identifier_field5_hit() {
    let n = mk(NodePartial {
        identifier: Some("btn-settings".into()),
        ..Default::default()
    });
    assert!(match_text(&n, &Pattern::text("BTN-SETTINGS")));
}

#[test]
fn text_string_text_field6_hit() {
    let n = mk(NodePartial {
        text: Some("legacy-text".into()),
        ..Default::default()
    });
    assert!(match_text(&n, &Pattern::text("legacy-text")));
}

#[test]
fn text_string_miss_returns_false() {
    let n = mk(NodePartial {
        label: Some("Dashboard".into()),
        ..Default::default()
    });
    assert!(!match_text(&n, &Pattern::text("Settings")));
}

#[test]
fn text_string_empty_returns_false() {
    // An empty pattern rejects, even against an empty label.
    let n = mk(NodePartial {
        label: Some("".into()),
        ..Default::default()
    });
    assert!(!match_text(&n, &Pattern::text("")));
    // Non-empty label still doesn't match empty pattern.
    let n2 = mk(NodePartial {
        label: Some("X".into()),
        ..Default::default()
    });
    assert!(!match_text(&n2, &Pattern::text("")));
}

#[test]
fn text_string_no_partial_match() {
    // strict equal — "Settings" does NOT match "Setting" or "Settings panel".
    let n = mk(NodePartial {
        label: Some("Settings panel".into()),
        ..Default::default()
    });
    assert!(!match_text(&n, &Pattern::text("Settings")));
}

// ---- regex pattern ------------------------------------------------------

#[test]
fn regex_default_auto_i_flag_inject() {
    // A regex missing the /i flag gets it injected automatically.
    let n = mk(NodePartial {
        label: Some("Hello".into()),
        ..Default::default()
    });
    let p = Pattern::regex_with_flags("^hello", ""); // no /i
    assert!(match_text(&n, &p));
}

#[test]
fn regex_explicit_i_flag() {
    let n = mk(NodePartial {
        label: Some("HELLO".into()),
        ..Default::default()
    });
    let p = Pattern::regex_with_flags("^hello", "i");
    assert!(match_text(&n, &p));
}

#[test]
fn regex_partial_match_allowed() {
    // RegExp partial match (no auto-anchor), unlike string strict equal.
    let n = mk(NodePartial {
        label: Some("Hello world".into()),
        ..Default::default()
    });
    let p = Pattern::regex("^Hel");
    assert!(match_text(&n, &p));
}

#[test]
fn regex_miss_returns_false() {
    let n = mk(NodePartial {
        label: Some("Dashboard".into()),
        ..Default::default()
    });
    let p = Pattern::regex("^Settings");
    assert!(!match_text(&n, &p));
}

#[test]
fn regex_invalid_pattern_returns_false() {
    let n = mk(NodePartial {
        label: Some("X".into()),
        ..Default::default()
    });
    // Unbalanced bracket is a regex compile error.
    let p = Pattern::regex_with_flags("[", "i");
    assert!(!match_text(&n, &p));
}

// ---- serde round-trip ---------------------------------------------------

#[test]
fn pattern_text_serde_round_trip_as_plain_string() {
    let p = Pattern::text("hello");
    let json = serde_json::to_string(&p).unwrap();
    assert_eq!(json, r#""hello""#);
    let parsed: Pattern = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, p);
}

#[test]
fn pattern_regex_serde_round_trip_as_tagged_object() {
    let p = Pattern::Regex {
        regex: "^hi".into(),
        flags: "i".into(),
    };
    let json = serde_json::to_string(&p).unwrap();
    let parsed: Pattern = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, p);
    assert!(json.contains("\"regex\":\"^hi\""));
    assert!(json.contains("\"flags\":\"i\""));
}

#[test]
fn selector_text_serde_round_trip_camel_case_modifiers() {
    use smix_selector::{Modifiers, Selector};
    let s = Selector::Text {
        text: Pattern::text("Login"),
        modifiers: Modifiers {
            below: Some(Box::new(Selector::Text {
                text: Pattern::text("Header"),
                modifiers: Modifiers::default(),
            })),
            ..Default::default()
        },
    };
    let json = serde_json::to_string(&s).unwrap();
    let parsed: Selector = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, s);
}

#[test]
fn selector_anchor_serde_with_index_modifiers() {
    use smix_selector::{AnchorBox, IndexModifiers, Selector};
    let s = Selector::Anchor {
        anchor: AnchorBox {
            below: Some(Box::new(Selector::Text {
                text: Pattern::text("Anchor"),
                modifiers: Default::default(),
            })),
            ..Default::default()
        },
        index: IndexModifiers {
            nth: Some(1),
            ..Default::default()
        },
    };
    let json = serde_json::to_string(&s).unwrap();
    let parsed: Selector = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, s);
}

// ---- Modifiers::ancestor -------------------------------------------------

#[test]
fn selector_text_serde_round_trip_with_ancestor_modifier() {
    use smix_screen::Role;
    use smix_selector::{Modifiers, Selector};
    let s = Selector::Text {
        text: Pattern::text("Tracking"),
        modifiers: Modifiers {
            ancestor: Some(Box::new(Selector::Role {
                role: Role::TabBar,
                name: None,
                modifiers: Modifiers::default(),
            })),
            ..Default::default()
        },
    };
    let json = serde_json::to_string(&s).unwrap();
    assert!(
        json.contains("\"ancestor\":{"),
        "wire field name should be camelCase `ancestor`; actual json = {json}"
    );
    let parsed: Selector = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, s);
}

#[test]
fn modifiers_omit_ancestor_when_none_back_compat() {
    use smix_selector::{Modifiers, Selector};
    let s = Selector::Text {
        text: Pattern::text("X"),
        modifiers: Modifiers {
            below: Some(Box::new(Selector::Text {
                text: Pattern::text("Header"),
                modifiers: Modifiers::default(),
            })),
            ..Default::default()
        },
    };
    let json = serde_json::to_string(&s).unwrap();
    assert!(
        !json.contains("ancestor"),
        "skip_serializing_if=Option::is_none should keep `ancestor` off the wire when None; actual json = {json}"
    );
}

#[test]
fn describe_selector_renders_ancestor_segment() {
    use smix_selector::{Modifiers, Selector, describe_selector};
    let s = Selector::Text {
        text: Pattern::text("Tracking"),
        modifiers: Modifiers {
            ancestor: Some(Box::new(Selector::Id {
                id: "tab-bar-root".into(),
                modifiers: Modifiers::default(),
            })),
            ..Default::default()
        },
    };
    let rendered = describe_selector(&s);
    assert!(
        rendered.contains("ancestor=("),
        "describe_selector should render an ancestor=( segment; actual = {rendered}"
    );
}

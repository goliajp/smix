//! A question this layer could not evaluate is not an answer of "no".
//!
//! `assertNotVisible: { ocrText: 'smix fixture' }` passed against a
//! screen with those very words on it. The tree resolver has no image
//! to read, so it matched nothing, and "matched nothing" was read as
//! "absent". Measured on emulator-5554 alongside two controls: the same
//! string with `text:` was correctly found, and `assertNotVisible` with
//! `text:` correctly failed. Same verb, same screen, same words — the
//! form decided whether anything was checked at all.
//!
//! A chain counts too. `fallback: [id, ocrText]` where the id misses
//! has not been evaluated to the end, so its silence says nothing
//! either.

use smix_sdk::unevaluable_form;
use smix_selector::{AnchorBox, IndexModifiers, Modifiers, Pattern, Role, Selector, True};

fn id(name: &str) -> Selector {
    Selector::Id {
        id: name.into(),
        modifiers: Modifiers::default(),
    }
}

fn ocr(text: &str) -> Selector {
    Selector::OcrText {
        ocr_text: text.into(),
        locales: vec![],
        modifiers: Modifiers::default(),
    }
}

#[test]
fn the_forms_this_layer_cannot_read_are_named() {
    assert_eq!(unevaluable_form(&ocr("Submit")), Some("ocrText"));
    assert_eq!(
        unevaluable_form(&Selector::LocalizedText {
            localized_text: [("en".to_string(), "Submit".to_string())]
                .into_iter()
                .collect(),
            modifiers: Modifiers::default(),
        }),
        Some("localizedText"),
    );
    assert_eq!(
        unevaluable_form(&Selector::AnchorRelative {
            anchor: Box::new(id("x")),
            dx: 1.0,
            dy: 2.0,
        }),
        Some("anchorRelative"),
    );
}

#[test]
fn the_forms_it_can_read_are_not_named() {
    for sel in [
        id("x"),
        Selector::Text {
            text: Pattern::Text("Submit".into()),
            modifiers: Modifiers::default(),
        },
        Selector::Label {
            label: "Submit".into(),
            modifiers: Modifiers::default(),
        },
        Selector::Role {
            role: Role::Button,
            name: None,
            modifiers: Modifiers::default(),
        },
        Selector::Focused {
            focused: True(true),
        },
        Selector::Anchor {
            anchor: AnchorBox::default(),
            index: IndexModifiers::default(),
        },
        Selector::Point { nx: 0.5, ny: 0.5 },
    ] {
        assert_eq!(unevaluable_form(&sel), None, "{sel:?} is readable here");
    }
}

#[test]
fn a_layer_hidden_in_a_chain_still_counts() {
    // The chain was not evaluated to the end, so its silence is not
    // evidence of absence — which is the whole reason this exists.
    assert_eq!(
        unevaluable_form(&Selector::Fallback {
            fallback: vec![id("nope"), ocr("Submit")],
        }),
        Some("ocrText"),
    );
}

#[test]
fn a_chain_of_readable_layers_is_readable() {
    assert_eq!(
        unevaluable_form(&Selector::Fallback {
            fallback: vec![id("a"), id("b")],
        }),
        None,
    );
}

#[test]
fn nesting_does_not_hide_it() {
    assert_eq!(
        unevaluable_form(&Selector::Fallback {
            fallback: vec![Selector::Fallback {
                fallback: vec![id("a"), ocr("Submit")],
            }],
        }),
        Some("ocrText"),
    );
}

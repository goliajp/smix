//! No selector form is silently unresolvable.
//!
//! `Fallback` sat in `matches_base` answering `false`, under a comment
//! saying the adapter dispatched it. Three verbs did; every other one
//! had never heard of the arrangement, so `assertVisible` with a chain
//! matched nothing on either platform for as long as chains existed.
//! Nothing failed, because nothing asked.
//!
//! The anchor here is the exhaustive `match`: a variant added later
//! does not compile until someone says which side of the line it is on.
//! And the exemptions are checked from the other side too — a form
//! listed as unresolvable must actually be unresolvable, or the
//! exemption has expired and is now hiding a working feature from the
//! only test that would have covered it.

use smix_screen::{A11yNode, Rect, Role as NodeRole};
use smix_selector::{AnchorBox, IndexModifiers, Modifiers, Pattern, Role, Selector, True};
use smix_selector_resolver::resolve_selector;

fn leaf(id: &str, label: &str, y: f64) -> A11yNode {
    A11yNode {
        raw_type: "button".into(),
        element_type_raw: 9,
        role: Some(NodeRole::Button),
        identifier: Some(id.into()),
        label: Some(label.into()),
        title: None,
        placeholder_value: None,
        value: None,
        text: Some(label.into()),
        bounds: Rect {
            x: 0.0,
            y,
            w: 100.0,
            h: 20.0,
        },
        enabled: true,
        selected: false,
        has_focus: id == "focused-one",
        visible: true,
        children: vec![],
    }
}

fn tree() -> A11yNode {
    let mut root = leaf("root", "root", 0.0);
    root.bounds = Rect {
        x: 0.0,
        y: 0.0,
        w: 400.0,
        h: 800.0,
    };
    root.children = vec![
        leaf("target", "Submit", 100.0),
        leaf("focused-one", "Focused", 200.0),
        leaf("anchor", "Anchor", 300.0),
    ];
    root
}

/// Why a form cannot be resolved against an accessibility tree. Listing
/// one here is a claim that is checked: see `an_exemption_must_still_be
/// _true`.
struct Exempt {
    selector: Selector,
    why: &'static str,
}

/// Every variant, one arm each. Adding a variant to `Selector` breaks
/// this match, which is the point.
fn sample(discriminant: &Selector) -> Result<Selector, Exempt> {
    let plain = Modifiers::default();
    match discriminant {
        Selector::Text { .. } => Ok(Selector::Text {
            text: Pattern::Text("Submit".into()),
            modifiers: plain,
        }),
        Selector::Id { .. } => Ok(Selector::Id {
            id: "target".into(),
            modifiers: plain,
        }),
        Selector::Label { .. } => Ok(Selector::Label {
            label: "Submit".into(),
            modifiers: plain,
        }),
        Selector::Role { .. } => Ok(Selector::Role {
            role: Role::Button,
            name: Some(Pattern::Text("Submit".into())),
            modifiers: plain,
        }),
        Selector::Focused { .. } => Ok(Selector::Focused {
            focused: True(true),
        }),
        Selector::Anchor { .. } => Ok(Selector::Anchor {
            anchor: AnchorBox {
                below: Some(Box::new(Selector::Id {
                    id: "target".into(),
                    modifiers: Modifiers::default(),
                })),
                ..Default::default()
            },
            index: IndexModifiers::default(),
        }),
        Selector::LocalizedText { .. } => Err(Exempt {
            // Populated on purpose. An empty locale map matches nothing
            // whatever the rule is, so exempting it would prove only
            // that emptiness is empty.
            selector: Selector::LocalizedText {
                localized_text: [("en".to_string(), "Submit".to_string())]
                    .into_iter()
                    .collect(),
                modifiers: Modifiers::default(),
            },
            why: "a locale map is desugared to a Text selector by the adapter \
                  before any resolver sees it; there is no locale here to pick with",
        }),
        Selector::OcrText { .. } => Err(Exempt {
            selector: Selector::OcrText {
                ocr_text: "Submit".into(),
                locales: vec!["en-US".into()],
                modifiers: Modifiers::default(),
            },
            why: "pixels, not the accessibility tree — the resolver has no image \
                  to read and the verb that uses it says so",
        }),
        Selector::AnchorRelative { .. } => Err(Exempt {
            selector: Selector::AnchorRelative {
                anchor: Box::new(Selector::Id {
                    id: "target".into(),
                    modifiers: Modifiers::default(),
                }),
                dx: 0.0,
                dy: 30.0,
            },
            why: "resolves to a shifted coordinate rather than to a node; the \
                  callers that accept it act on the coordinate",
        }),
        Selector::Point { .. } => Err(Exempt {
            selector: Selector::Point { nx: 0.25, ny: 0.5 },
            why: "a coordinate is not a description of a node",
        }),
        Selector::Fallback { .. } => Ok(Selector::Fallback {
            fallback: vec![
                Selector::Id {
                    id: "no-such-thing".into(),
                    modifiers: Modifiers::default(),
                },
                Selector::Id {
                    id: "target".into(),
                    modifiers: Modifiers::default(),
                },
            ],
        }),
    }
}

fn every_variant() -> Vec<Selector> {
    let plain = Modifiers::default();
    vec![
        Selector::Text {
            text: Pattern::Text(String::new()),
            modifiers: plain.clone(),
        },
        Selector::Id {
            id: String::new(),
            modifiers: plain.clone(),
        },
        Selector::Label {
            label: String::new(),
            modifiers: plain.clone(),
        },
        Selector::Role {
            role: Role::Button,
            name: None,
            modifiers: plain.clone(),
        },
        Selector::Focused {
            focused: True(true),
        },
        Selector::Anchor {
            anchor: AnchorBox::default(),
            index: IndexModifiers::default(),
        },
        Selector::LocalizedText {
            localized_text: Default::default(),
            modifiers: plain.clone(),
        },
        Selector::OcrText {
            ocr_text: String::new(),
            locales: vec![],
            modifiers: plain.clone(),
        },
        Selector::AnchorRelative {
            anchor: Box::new(Selector::Id {
                id: String::new(),
                modifiers: Modifiers::default(),
            }),
            dx: 0.0,
            dy: 0.0,
        },
        Selector::Point { nx: 0.5, ny: 0.5 },
        Selector::Fallback { fallback: vec![] },
    ]
}

#[test]
fn every_form_either_resolves_or_says_why_not() {
    let screen = tree();
    for discriminant in every_variant() {
        match sample(&discriminant) {
            Ok(sel) => {
                assert!(
                    resolve_selector(&screen, &sel).is_some(),
                    "{sel:?} resolves nothing. If that is correct, it belongs in \
                     the exempt arm with a reason — silence is how `Fallback` \
                     spent its whole existence matching nothing."
                );
            }
            Err(Exempt { why, .. }) => {
                assert!(!why.is_empty(), "an exemption must give a reason");
            }
        }
    }
}

#[test]
fn an_exemption_must_still_be_true() {
    // The other direction. An exemption that has quietly started
    // working keeps the form out of the test above — it satisfies the
    // check, and the check stops looking at it.
    let screen = tree();
    for discriminant in every_variant() {
        if let Err(Exempt { selector, why }) = sample(&discriminant) {
            assert!(
                resolve_selector(&screen, &selector).is_none(),
                "{selector:?} is exempt on the grounds that {why} — but it \
                 resolved. The exemption has expired; move it to the resolving \
                 arm so it is covered."
            );
        }
    }
}

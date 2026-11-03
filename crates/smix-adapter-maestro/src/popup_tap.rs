//! A `tapOn` that names a button on a system popup.
//!
//! A permission prompt belongs to SpringBoard (or to Android's permission
//! controller), not to the app, so the app's tree never holds its buttons
//! and `tapOn: "Allow"` ended in ElementNotFound. When the app's tree has no
//! match, the popups on screen are asked for a button with the same name.
//! An iOS notification banner is one of them, its button named by the
//! notification's title.

use smix_sdk::{Modifiers, Pattern, Selector, SystemPopup};

use crate::runtime::{AppLike, RunError, RunStepReport};

/// What the popups on screen hold for one selector.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum PopupButton {
    /// Exactly one button answers to it.
    One {
        popup_id: String,
        button_id: String,
        /// For the step's note: what was pressed, and on whose popup.
        said: String,
    },
    /// Several do; pressing one would be a guess. Each is described.
    Several(Vec<String>),
    /// None does, or the selector is not a name a popup button can carry.
    Nothing,
}

/// The popup button `selector` names. Only a bare `text:` or `label:` is
/// looked up: a spatial or index modifier places the target in the app's
/// layout, which a popup is not part of, and an id is the app's own.
pub(crate) fn button_named(selector: &Selector, popups: &[SystemPopup]) -> PopupButton {
    let Some(answers) = words_matcher(selector) else {
        return PopupButton::Nothing;
    };

    let hits: Vec<(&SystemPopup, usize)> = popups
        .iter()
        .flat_map(|p| (0..p.buttons.len()).map(move |i| (p, i)))
        .filter(|(p, i)| answers(&p.buttons[*i].label))
        .collect();
    let describe = |(p, i): &(&SystemPopup, usize)| {
        format!(
            "\"{}\" on {}'s popup \"{}\"",
            p.buttons[*i].label, p.source, p.title
        )
    };
    match hits.as_slice() {
        [] => PopupButton::Nothing,
        [one] => PopupButton::One {
            popup_id: one.0.id.clone(),
            button_id: one.0.buttons[one.1].id.clone(),
            said: describe(one),
        },
        many => PopupButton::Several(many.iter().map(describe).collect()),
    }
}

/// What a bare `text:` or `label:` matches words on a popup by, or `None`
/// for any other selector: a spatial or index modifier places the target in
/// the app's layout, which a popup is not part of, and an id is the app's own.
fn words_matcher(selector: &Selector) -> Option<Box<dyn Fn(&str) -> bool>> {
    let answers: Box<dyn Fn(&str) -> bool> = match selector {
        Selector::Text { text, modifiers } if *modifiers == Modifiers::default() => match text {
            Pattern::Text(t) if t.is_empty() => return None,
            Pattern::Text(t) => {
                let t = straight(t);
                Box::new(move |l: &str| straight(l).eq_ignore_ascii_case(&t))
            }
            Pattern::Regex { .. } => match text.compile() {
                Ok(smix_selector::CompiledPattern::Regex(re)) => {
                    Box::new(move |l: &str| re.is_match(l))
                }
                _ => return None,
            },
        },
        Selector::Label { label, modifiers } if *modifiers == Modifiers::default() => {
            let label = label.clone();
            Box::new(move |l: &str| l == label)
        }
        _ => return None,
    };
    Some(answers)
}

/// An `assertVisible` that came back empty from the app's tree, given the
/// popups on screen — a notification banner among them — to find the words
/// on: a title, a body or a button. Found, the step passes and says where.
pub(crate) async fn or_a_popup_saying<A: AppLike + ?Sized>(
    app: &A,
    selector: &Selector,
    asserted: Result<(), smix_sdk::ExpectationFailure>,
    warnings: &mut Vec<String>,
) -> Result<(), smix_sdk::ExpectationFailure> {
    let Err(e) = asserted else { return Ok(()) };
    let Some(answers) = words_matcher(selector).filter(|_| e.code.may_be_outside_the_app()) else {
        return Err(e);
    };
    let Ok(popups) = app.system_popups().await else {
        return Err(e);
    };
    let hit = popups.iter().find(|p| {
        answers(&p.title) || answers(&p.body) || p.buttons.iter().any(|b| answers(&b.label))
    });
    match hit {
        Some(p) => {
            warnings.push(format!(
                "assertVisible {}: not in the app's tree; on {}'s {} \"{}\"",
                smix_sdk::describe_selector(selector),
                p.source,
                p.kind,
                p.title
            ));
            Ok(())
        }
        None => Err(e),
    }
}

/// A `tapOn` that came back empty from the app's tree, given one more place
/// to look. `selector` is the one the tap resolved (localized text already
/// turned into text). A press is reported in `warnings` so the run says
/// which popup it answered; a miss keeps the tap's own result, with what the
/// popups held added to its hint when that explains anything.
pub(crate) async fn or_a_popup_button<A: AppLike + ?Sized>(
    app: &A,
    selector: &Selector,
    tapped: Result<RunStepReport, RunError>,
    warnings: &mut Vec<String>,
) -> Result<RunStepReport, RunError> {
    let missed = match &tapped {
        Err(RunError::Sdk(e)) => e.code.may_be_outside_the_app(),
        Ok(RunStepReport::Skipped { .. }) => true,
        _ => false,
    };
    if !missed || !names_a_button(selector) {
        return tapped;
    }
    let also = match app.system_popups().await {
        Ok(popups) => match button_named(selector, &popups) {
            PopupButton::One {
                popup_id,
                button_id,
                said,
            } => {
                app.system_popup_action(&popup_id, &button_id).await?;
                warnings.push(format!(
                    "tapOn {}: not in the app's tree; pressed {said}",
                    smix_sdk::describe_selector(selector)
                ));
                return Ok(RunStepReport::Ok);
            }
            PopupButton::Several(all) => Some(format!(
                "System popups have {} buttons that answer to it, so none was pressed: {}",
                all.len(),
                all.join("; ")
            )),
            PopupButton::Nothing => None,
        },
        Err(e) => Some(format!(
            "The system popups could not be read to look there too: {}",
            e.message
        )),
    };
    match (tapped, also) {
        (Err(RunError::Sdk(mut e)), Some(also)) => {
            e.hint = Some(match e.hint.take() {
                Some(h) => format!("{h}\n{also}"),
                None => also,
            });
            Err(RunError::Sdk(e))
        }
        (tapped, _) => tapped,
    }
}

/// System popups write "Don\u{2019}t Allow" with a typographic apostrophe,
/// and a flow is typed with a straight one.
fn straight(s: &str) -> String {
    s.replace('\u{2019}', "'")
}

/// Whether `selector` is a bare `text:` / `label:`, the only forms a popup
/// button is looked up by.
fn names_a_button(selector: &Selector) -> bool {
    match selector {
        Selector::Text { text, modifiers } => {
            *modifiers == Modifiers::default() && !matches!(text, Pattern::Text(t) if t.is_empty())
        }
        Selector::Label { modifiers, .. } => *modifiers == Modifiers::default(),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn popup(id: &str, title: &str, buttons: &[(&str, &str)]) -> SystemPopup {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "type": "alert",
            "source": "com.apple.springboard",
            "title": title,
            "buttons": buttons.iter().map(|(bid, label)| serde_json::json!({
                "id": bid, "label": label, "role": "default"
            })).collect::<Vec<_>>(),
        }))
        .unwrap()
    }

    fn notifications() -> Vec<SystemPopup> {
        vec![popup(
            "popup-0",
            "\u{201c}Insight\u{201d} Would Like to Send You Notifications",
            &[("b-0", "Don\u{2019}t Allow"), ("b-1", "Allow")],
        )]
    }

    #[test]
    fn a_text_selector_finds_the_button_whatever_its_case() {
        let got = button_named(&smix_sdk::text("allow"), &notifications());
        let PopupButton::One {
            popup_id,
            button_id,
            said,
        } = got
        else {
            panic!("{got:?}")
        };
        assert_eq!((popup_id.as_str(), button_id.as_str()), ("popup-0", "b-1"));
        assert!(said.contains("com.apple.springboard"), "{said}");
    }

    #[test]
    fn a_straight_apostrophe_finds_the_typographic_one() {
        let PopupButton::One { button_id, .. } =
            button_named(&smix_sdk::text("Don't Allow"), &notifications())
        else {
            panic!("\"Don't Allow\" did not find \"Don\u{2019}t Allow\"")
        };
        assert_eq!(button_id, "b-0");
    }

    #[test]
    fn a_label_is_matched_exactly() {
        let exact = Selector::Label {
            label: "Allow".into(),
            modifiers: Modifiers::default(),
        };
        assert!(matches!(
            button_named(&exact, &notifications()),
            PopupButton::One { .. }
        ));
        let lower = Selector::Label {
            label: "allow".into(),
            modifiers: Modifiers::default(),
        };
        assert_eq!(button_named(&lower, &notifications()), PopupButton::Nothing);
    }

    #[test]
    fn two_buttons_that_answer_are_not_guessed_between() {
        let re = Selector::Text {
            text: Pattern::regex("allow"),
            modifiers: Modifiers::default(),
        };
        let PopupButton::Several(both) = button_named(&re, &notifications()) else {
            panic!("a regex both buttons match must not pick one")
        };
        assert_eq!(both.len(), 2);
    }

    #[test]
    fn an_id_or_a_placed_selector_is_not_looked_up() {
        assert_eq!(
            button_named(&smix_sdk::id("b-1"), &notifications()),
            PopupButton::Nothing
        );
        let placed = Selector::Text {
            text: Pattern::text("Allow"),
            modifiers: Modifiers {
                nth: Some(0),
                ..Modifiers::default()
            },
        };
        assert_eq!(
            button_named(&placed, &notifications()),
            PopupButton::Nothing
        );
    }

    #[test]
    fn no_popup_no_button() {
        assert_eq!(
            button_named(&smix_sdk::text("Allow"), &[]),
            PopupButton::Nothing
        );
    }
}

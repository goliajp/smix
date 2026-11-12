//! What is typed into a masked field is kept like a secret, named or not.
//!
//! `inputText: { secret: … }` keeps a value out of everything a run writes.
//! A password typed as plain `inputText` did not get that, and it still
//! surfaced: a keyboard offered it back in its suggestion strip, and the
//! tree saved with the next failure carried it. The field says it masks —
//! by its role, or by showing only bullets — so the run can tell without
//! being told.

use smix_sdk::{A11yNode, Role};

use super::{Adapter, IMPLICIT_WAIT, RunError, RunStepReport};
use crate::AppLike;
use smix_sdk::Selector;

/// What a masked field shows for each character: UIKit and Compose draw
/// `•`, some Android keyboards' fields `●`.
const BULLETS: [char; 2] = ['\u{2022}', '\u{25CF}'];

fn masks(n: &A11yNode) -> bool {
    n.role == Some(Role::SecureTextField)
        || [n.text.as_deref(), n.value.as_deref()]
            .into_iter()
            .flatten()
            .any(|t| !t.is_empty() && t.chars().all(|c| BULLETS.contains(&c)))
}

/// Whether the field holding the focus in `tree` masks what it holds.
pub(super) fn focus_masks(tree: &A11yNode) -> bool {
    (tree.has_focus && masks(tree)) || tree.children.iter().any(focus_masks)
}

impl<A: AppLike + ?Sized> Adapter<'_, A> {
    /// `inputText` and `setText`: into the focused field, or into `into`.
    ///
    /// A target the tree cannot read is a place rather than an element:
    /// tapped to take the focus, then typed where the focus went.
    pub(super) async fn input_text(
        &mut self,
        text: &str,
        into: Option<&Selector>,
        set: bool,
    ) -> Result<RunStepReport, RunError> {
        let expanded = self.expand_template(text)?;
        let target = into.map(|sel| self.desugar_localized_text(sel).into_owned());
        let place = match &target {
            Some(t) => self.point_for_unreadable(t, IMPLICIT_WAIT).await?,
            None => None,
        };
        if let Some((nx, ny)) = place {
            self.app.tap_at_coord(nx, ny).await?;
        }
        let at = match (target, place) {
            (Some(t), None) => t,
            _ => smix_sdk::focused(),
        };
        let done = if set {
            self.app.set_text(&at, &expanded).await
        } else {
            self.app.fill(&at, &expanded).await
        };
        self.keep_if_masked(&expanded).await;
        done?;
        Ok(RunStepReport::Ok)
    }

    /// After typing `value`: when the field holding the focus masks, keep
    /// `value` as one of this run's secrets.
    ///
    /// A tree that cannot be read keeps nothing, and the step's own answer
    /// stands: failing a fill that worked over one extra look would be the
    /// worse error, and a masked field's runner never echoes what it got.
    pub(super) async fn keep_if_masked(&mut self, value: &str) {
        if value.is_empty() || self.secrets.iter().any(|s| s == value) {
            return;
        }
        if self.app.tree().await.is_ok_and(|t| focus_masks(&t)) {
            self.secrets.push(value.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(json: serde_json::Value) -> A11yNode {
        let mut base = serde_json::json!({
            "rawType": "other", "elementTypeRaw": 0,
            "bounds": {"x": 0, "y": 0, "w": 10, "h": 10},
            "enabled": true, "selected": false, "hasFocus": false, "visible": true,
            "children": [],
        });
        base.as_object_mut()
            .expect("object")
            .extend(json.as_object().expect("object").clone());
        serde_json::from_value(base).expect("a node")
    }

    fn screen(field: serde_json::Value) -> A11yNode {
        let mut root = node(serde_json::json!({}));
        root.children = vec![node(serde_json::json!({"text": "Sign in"})), node(field)];
        root
    }

    #[test]
    fn a_focused_field_showing_only_bullets_masks() {
        assert!(focus_masks(&screen(
            serde_json::json!({"hasFocus": true, "text": "•••••••"})
        )));
        assert!(focus_masks(&screen(
            serde_json::json!({"hasFocus": true, "value": "●●●●"})
        )));
    }

    #[test]
    fn a_focused_secure_field_masks_even_empty() {
        assert!(focus_masks(&screen(
            serde_json::json!({"hasFocus": true, "role": "secureTextField"})
        )));
    }

    #[test]
    fn a_plain_focused_field_does_not() {
        assert!(!focus_masks(&screen(
            serde_json::json!({"hasFocus": true, "text": "alice"})
        )));
        assert!(!focus_masks(&screen(
            serde_json::json!({"hasFocus": true, "text": ""})
        )));
    }

    #[test]
    fn a_masked_field_without_the_focus_does_not() {
        // the value typed went to the focused field, not to this one
        assert!(!focus_masks(&screen(serde_json::json!({"text": "•••"}))));
    }
}

//! Naming an element over MCP.

use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::Deserialize;
use smix_sdk::Selector;
use smix_selector::{Modifiers, ROLE_NAMES, role_from_name};

/// How to find one element on screen. Give exactly one of `id`, `text`,
/// `label`, `role`, or `ocrText`.
///
/// Prefer `id` — it survives copy changes and localization. Fall back to
/// `text` or `label` when the app has no testID, and to `ocrText` only when
/// the element is invisible to the accessibility tree (a canvas, an image of
/// text), since that path costs an OCR pass.
///
/// Every doc comment in here is serialized into the tool schema and is the
/// only documentation the calling agent gets.
// camelCase, matching the yaml surface and the Selector wire — an agent that
// has read the yaml docs should not have to learn a second spelling.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SelectorParams {
    /// Accessibility identifier — the app's testID. The most robust way to
    /// name an element.
    #[serde(default)]
    pub id: Option<String>,
    /// Visible text, matched case-insensitively across the element's
    /// label / title / value.
    #[serde(default)]
    pub text: Option<String>,
    /// Accessibility label, matched exactly.
    #[serde(default)]
    pub label: Option<String>,
    /// Element kind: button, link, textField, staticText, and so on. Narrow
    /// it with `name` when there is more than one.
    #[serde(default)]
    pub role: Option<String>,
    /// Narrows `role` to the one with this name. Only meaningful with `role`.
    #[serde(default)]
    pub name: Option<String>,
    /// Text read off the screen by OCR, for elements the accessibility tree
    /// cannot see. Slower than the others; reach for it last.
    #[serde(default)]
    pub ocr_text: Option<String>,
}

impl SelectorParams {
    /// Turn what the agent wrote into a selector.
    ///
    /// Naming nothing, or naming two things, is an error. Picking one of two
    /// would be a coin flip the agent never sees.
    pub fn to_selector(&self) -> Result<Selector, McpError> {
        let given: Vec<&str> = [
            self.id.as_ref().map(|_| "id"),
            self.text.as_ref().map(|_| "text"),
            self.label.as_ref().map(|_| "label"),
            self.role.as_ref().map(|_| "role"),
            self.ocr_text.as_ref().map(|_| "ocrText"),
        ]
        .into_iter()
        .flatten()
        .collect();

        match given.len() {
            0 => {
                return Err(McpError::invalid_params(
                    "name the element with exactly one of: id, text, label, role, ocrText \
                     (prefer id)",
                    None,
                ));
            }
            1 => {}
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "name the element with exactly one of id, text, label, role, ocrText — got {}",
                        given.join(" and ")
                    ),
                    None,
                ));
            }
        }

        if self.name.is_some() && self.role.is_none() {
            return Err(McpError::invalid_params(
                "`name` narrows `role`, so it needs one — give role, or use `text` to match \
                 visible text on its own",
                None,
            ));
        }

        if let Some(id) = &self.id {
            return Ok(smix_sdk::id(id.clone()));
        }
        if let Some(text) = &self.text {
            return Ok(smix_sdk::text(text.clone()));
        }
        if let Some(label) = &self.label {
            return Ok(smix_sdk::label(label.clone()));
        }
        if let Some(role) = &self.role {
            let r = role_from_name(role).ok_or_else(|| {
                McpError::invalid_params(
                    format!("unknown role `{role}`; accepted: {ROLE_NAMES}"),
                    None,
                )
            })?;
            return Ok(match &self.name {
                Some(n) => smix_sdk::role_named(r, n.clone()),
                None => smix_sdk::role(r),
            });
        }
        if let Some(ocr) = &self.ocr_text {
            return Ok(Selector::OcrText {
                ocr_text: ocr.clone(),
                // Empty means the adapter fills in the session's locale.
                locales: Vec::new(),
                modifiers: Modifiers::default(),
            });
        }

        // The count check above already covered this.
        unreachable!("exactly one field was present")
    }
}

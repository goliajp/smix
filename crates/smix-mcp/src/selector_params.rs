//! Naming an element over MCP.

use rmcp::ErrorData as McpError;
use schemars::JsonSchema;
use serde::Deserialize;
use smix_sdk::Selector;
use smix_selector::{Modifiers, ROLE_NAMES, point_from_str, role_from_name};

/// How to find one element on screen. Give exactly one of `id`, `text`,
/// `label`, `role`, `ocrText`, `point`, or `fallback`.
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
    /// A place on screen rather than a thing on it: `"50%,80%"` or the same
    /// point written `"0.5,0.8"`. A fraction of the viewport, never pixels —
    /// a flow written in pixels runs on one screen size.
    ///
    /// The last resort, and only for tapping. Nothing here is named, so
    /// nothing can be found, asserted, or filled at a point — those tools
    /// refuse it rather than tapping where you did not ask. Use it when the
    /// target has no accessibility form at all: a canvas, a map, a video
    /// surface. If the element has an id, a label, or visible text, one of
    /// those will still be right after the next layout change and this will
    /// not.
    #[serde(default)]
    pub point: Option<String>,
    /// Ways to name the same thing, tried in order, first hit wins.
    ///
    /// Each entry is a whole selector — `[{"id":"submit"},{"text":"Send"}]`
    /// — so a chain survives a copy edit or a missing testID without the
    /// caller looking first and choosing. Nesting a chain inside a chain
    /// is allowed and means the same as writing it flat.
    ///
    /// `point` may only be the last entry. A coordinate always hits, so
    /// anything after it is unreachable, and a chain with a dead tail is
    /// worse than no chain: it looks like a plan and is not one.
    #[serde(default)]
    pub fallback: Option<Vec<SelectorParams>>,
}

// What an agent may write, form by form. Read by
// `scripts/dev/selector-surface-scan.py`; a `none` carries its reason.
// selector-surface: Text — the `text` field
// selector-surface: Id — the `id` field, and the one to prefer
// selector-surface: Label — the `label` field
// selector-surface: Role — the `role` field, narrowed by `name`
// selector-surface: Focused — none, an agent asks for what it can see; focus is not visible in a tree dump
// selector-surface: Anchor — none, no modifier has an MCP form yet
// selector-surface: LocalizedText — none, not yet wired — reachable from yaml, missing here
// selector-surface: OcrText — the `ocrText` field, dispatched past the resolver
// selector-surface: AnchorRelative — none, not yet wired — reachable from yaml, missing here
// selector-surface: Point — the `point` field, dispatched by smix_tap and refused by find and the asserts
// selector-surface: Fallback — the `fallback` field, a list of selectors walked in order by tap / find / the asserts

/// The OCR needle, when the selector is the OCR path.
///
/// The tree resolver's `matches_base` returns `false` for `OcrText` by
/// design — OCR is a live-vision op, not a tree predicate — so a selector
/// that reaches `App::find` / `App::tap` as `OcrText` can never match.
/// Every MCP tool splits dispatch on this answer: `Some` goes to
/// `App::find_by_text_ocr` (and its tap companion), `None` to the tree
/// path. Top-level match only: [`SelectorParams::to_selector`] never
/// builds a `Fallback` chain.
pub fn ocr_text_of(selector: &Selector) -> Option<&str> {
    match selector {
        Selector::OcrText { ocr_text, .. } => Some(ocr_text),
        _ => None,
    }
}

/// The layers, when the selector is a chain.
///
/// `Fallback` is dispatched by the caller like `Point` is, and for the
/// same reason: the resolver returns false for it — "reaching
/// matches_base means a caller forgot to dispatch". A tool that hands a
/// chain to `App::find` gets one no, not several tries.
///
/// Flat, because nesting is allowed and means the same as writing it out
/// — so every caller iterates one list rather than each rediscovering
/// that a layer may itself be a chain.
pub fn chain_of(selector: &Selector) -> Option<Vec<Selector>> {
    match selector {
        Selector::Fallback { fallback } => {
            let mut out = Vec::new();
            for layer in fallback {
                match chain_of(layer) {
                    Some(inner) => out.extend(inner),
                    None => out.push(layer.clone()),
                }
            }
            Some(out)
        }
        _ => None,
    }
}

/// The coordinate, when the selector is a place rather than a thing.
///
/// Point is dispatched by the caller, never resolved. The resolver says so
/// in as many words — "reaching matches_base means a caller forgot to
/// dispatch; no node matches" — so a tool that accepts `point` and hands it
/// to `App::tap` gets a miss on a screen where the touch was perfectly
/// deliverable. Every tool that takes a selector splits on this the way it
/// already splits on [`ocr_text_of`]; the ones that cannot tap refuse.
pub fn point_of(selector: &Selector) -> Option<(f64, f64)> {
    match selector {
        Selector::Point { nx, ny } => Some((*nx, *ny)),
        _ => None,
    }
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
            self.point.as_ref().map(|_| "point"),
            self.fallback.as_ref().map(|_| "fallback"),
        ]
        .into_iter()
        .flatten()
        .collect();

        match given.len() {
            0 => {
                return Err(McpError::invalid_params(
                    "name the element with exactly one of: id, text, label, role, ocrText, \
                     point, fallback (prefer id; point is a place, not a thing, and only taps)",
                    None,
                ));
            }
            1 => {}
            _ => {
                return Err(McpError::invalid_params(
                    format!(
                        "name the element with exactly one of id, text, label, role, ocrText, point, fallback — got {}",
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

        if let Some(chain) = &self.fallback {
            if chain.is_empty() {
                return Err(McpError::invalid_params(
                    "fallback is an empty chain — a list of no ways to name a thing \
                     is not a way to name it",
                    None,
                ));
            }
            let mut out = Vec::with_capacity(chain.len());
            for (i, entry) in chain.iter().enumerate() {
                let sel = entry.to_selector().map_err(|e| {
                    McpError::invalid_params(format!("fallback[{i}]: {}", e.message), None)
                })?;
                // A coordinate always hits, so a layer after one is
                // unreachable. Taking the chain anyway would hand back
                // something that reads as a plan and has a dead tail.
                if matches!(sel, Selector::Point { .. }) && i + 1 < chain.len() {
                    return Err(McpError::invalid_params(
                        format!(
                            "fallback[{i}] is a point and {} layer(s) follow it. A \
                             coordinate always hits, so nothing after it is ever \
                             tried — put the point last, or drop it",
                            chain.len() - i - 1
                        ),
                        None,
                    ));
                }
                out.push(sel);
            }
            return Ok(Selector::Fallback { fallback: out });
        }
        if let Some(p) = &self.point {
            let (nx, ny) = point_from_str(p)
                .map_err(|reason| McpError::invalid_params(format!("point: {reason}"), None))?;
            return Ok(Selector::Point { nx, ny });
        }

        // The count check above already covered this.
        unreachable!("exactly one field was present")
    }
}

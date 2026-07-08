#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! smix-selector — Selector enum + Modifiers + AnchorBox + `match_text`
//! (stone). Ported from now-retired TS source: `src/core/selector.ts` +
//! `src/core/resolve-selector.ts:153-184`.
//!
//! # Scope
//!
//! - Pure types (`Selector`, `Pattern`, `Modifiers`, `AnchorBox`,
//!   `IndexModifiers`) with serde wire compatibility (camelCase JSON,
//!   matching SDK / CLI / MCP / runner-client wire shape).
//! - Pure functions (`match_text`, `describe_selector`) — no I/O, no Tree
//!   traversal (resolver lives in `smix-selector-resolver`, c5+).
//!
//! # Selector base forms (跟 TS 同源, v1.5 C2 lock-in)
//!
//! 6 mutually exclusive base forms:
//! 1. Text (string / regex)
//! 2. Id (string strict equal vs node.identifier)
//! 3. Label (string strict equal vs node.label)
//! 4. Role { role, name?: pattern } (role enum + optional pattern name)
//! 5. Focused (focused: true — runtime-resolved current focus target)
//! 6. Anchor (anchor-only, no own base; spatial keys do the filtering)
//!
//! # Modifiers / AnchorBox (recursive)
//!
//! Base 1-4 stack `Modifiers` (6 spatial + 3 index). Base 6 (Anchor)
//! stacks only `IndexModifiers` (no nested spatial — anchor IS the
//! spatial intent already). Base 5 (Focused) stacks nothing.
//!
//! # `Pattern` wire form (string-or-regex union, serde-friendly)
//!
//! TS `string | RegExp` doesn't survive JSON serialization (RegExp →
//! `{}`). Wire form is:
//! - `"plain"` (untagged string) — string strict equal (case-insensitive,
//!   v1.5 c5i-d maestro parity)
//! - `{ "regex": "pattern", "flags": "i" }` (object tag) — regex form,
//!   auto-/i flag inject if absent
//!
//! Runtime compile happens inside `match_text` (no cache at the stone
//! layer; resolver / SDK callers may memoize externally if needed).

#![doc(html_root_url = "https://docs.smix.dev/smix-selector")]

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use smix_screen::{A11yNode, Role};

// -------------------- Pattern (string | regex wire form) -----------------

/// String-or-regex pattern. Wire-compatible: plain JSON string ↔
/// [`Pattern::Text`]; tagged object `{regex, flags}` ↔ [`Pattern::Regex`].
///
/// Case-insensitive 模式跟 TS v1.5 c5i-d 1:1: regex 缺 /i flag 自动注入;
/// text 走 `eq_ignore_ascii_case` 模拟 `toLowerCase` strict equal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Pattern {
    /// Plain literal string. Empty string never matches anything (TS
    /// resolve-selector.ts:178 `if (pattern === '') return false`).
    Text(String),
    /// Compiled-on-call regex. `flags` defaults to "i" when absent to
    /// satisfy v1.5 c5i-d maestro parity.
    Regex {
        /// Regex source string (PCRE-like syntax via the `regex` crate).
        regex: String,
        /// Flags string (currently only `"i"` is honored; defaulted to `"i"`).
        #[serde(default = "default_regex_flags")]
        flags: String,
    },
}

fn default_regex_flags() -> String {
    "i".to_string()
}

impl Pattern {
    /// Construct a plain text pattern.
    pub fn text<S: Into<String>>(s: S) -> Self {
        Pattern::Text(s.into())
    }

    /// Construct a regex pattern. Auto-injects `i` flag if absent.
    pub fn regex<P: Into<String>>(pattern: P) -> Self {
        Pattern::Regex {
            regex: pattern.into(),
            flags: "i".to_string(),
        }
    }

    /// Construct a regex pattern with explicit flags.
    pub fn regex_with_flags<P: Into<String>, F: Into<String>>(pattern: P, flags: F) -> Self {
        Pattern::Regex {
            regex: pattern.into(),
            flags: flags.into(),
        }
    }

    /// Compile the wire-form pattern into a hot-path-ready [`CompiledPattern`].
    /// One-time cost (~5-50 μs for regex compile, ~ns for text); the
    /// resulting `CompiledPattern` matches in `5-50 ns` per call — the
    /// regex compile gets fully amortized after just one re-use.
    ///
    /// Use [`match_text`] for one-shot SDK convenience; use [`match_text_compiled`]
    /// inside any hot resolver loop after `Pattern::compile()` once.
    ///
    /// Returns `Err(regex::Error)` only for malformed regex patterns;
    /// text patterns are infallible.
    pub fn compile(&self) -> Result<CompiledPattern, regex::Error> {
        match self {
            Pattern::Text(s) => Ok(CompiledPattern::Text(s.clone())),
            Pattern::Regex { regex, flags: _ } => {
                let final_pattern = if regex.starts_with("(?i)") {
                    regex.clone()
                } else {
                    format!("(?i){}", regex)
                };
                regex::Regex::new(&final_pattern).map(CompiledPattern::Regex)
            }
        }
    }
}

/// Compiled, hot-path-ready pattern. Caches the compiled `regex::Regex`
/// (跟 TS RegExp 对象内部 compile-once cache 同精神 — TS RegExp lazily
/// compiles on first `.test()` and re-uses the compiled DFA; we eager-
/// compile via [`Pattern::compile`] and store the result for the same
/// effect on Rust side).
///
/// Not serde-serializable (regex::Regex 不 derive Serialize/Deserialize).
/// Wire form is [`Pattern`]; resolver/SDK convert with `.compile()` once.
#[derive(Clone, Debug)]
pub enum CompiledPattern {
    /// Plain literal string (case-insensitive ASCII equality).
    Text(String),
    /// Compiled regex (case-insensitive via auto-injected `(?i)` prefix).
    Regex(regex::Regex),
}

// -------------------- Selector enum --------------------------------------

/// Modifier set stackable on Text / Id / Label / Role base forms.
/// Spatial keys (near/below/above/leftOf/rightOf/inside) carry recursive
/// selectors that resolve to an anchor node; index keys (nth/first/last)
/// pick from the surviving candidate list.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Modifiers {
    /// Anchor sub-selector — candidate must be geometrically near it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near: Option<Box<Selector>>,
    /// Anchor sub-selector — candidate must be below it (larger y center).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub below: Option<Box<Selector>>,
    /// Anchor sub-selector — candidate must be above it (smaller y center).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub above: Option<Box<Selector>>,
    /// Anchor sub-selector — candidate must be to its left.
    #[serde(rename = "leftOf", skip_serializing_if = "Option::is_none")]
    pub left_of: Option<Box<Selector>>,
    /// Anchor sub-selector — candidate must be to its right.
    #[serde(rename = "rightOf", skip_serializing_if = "Option::is_none")]
    pub right_of: Option<Box<Selector>>,
    /// Anchor sub-selector — candidate must be geometrically inside it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inside: Option<Box<Selector>>,
    /// Ancestor sub-selector — candidate's a11y tree parent chain
    /// (recursive ancestors, excluding self) must contain at least one
    /// node matching this sub-selector. Different from
    /// [`Modifiers::inside`] (geometric bounds-containment): `ancestor`
    /// walks the a11y tree parent chain — structural filter, not spatial.
    /// Use this to disambiguate same-label candidates living under
    /// different ancestor subtrees (e.g. a bottom tab and a sub-tab both
    /// labeled `"Tracking"`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ancestor: Option<Box<Selector>>,
    /// Pick the nth match (0-indexed) from the surviving candidates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nth: Option<usize>,
    /// Pick the first match (`true`) — overridden by `nth` if both set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<bool>,
    /// Pick the last match (`true`) — overridden by `nth` if both set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<bool>,
}

/// Anchor base form's spatial keys. Identical shape to the spatial half
/// of [`Modifiers`] but without nth/first/last (Anchor stacks
/// IndexModifiers separately).
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AnchorBox {
    /// Anchor sub-selector — see [`Modifiers::near`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub near: Option<Box<Selector>>,
    /// Anchor sub-selector — see [`Modifiers::below`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub below: Option<Box<Selector>>,
    /// Anchor sub-selector — see [`Modifiers::above`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub above: Option<Box<Selector>>,
    /// Anchor sub-selector — see [`Modifiers::left_of`].
    #[serde(rename = "leftOf", skip_serializing_if = "Option::is_none")]
    pub left_of: Option<Box<Selector>>,
    /// Anchor sub-selector — see [`Modifiers::right_of`].
    #[serde(rename = "rightOf", skip_serializing_if = "Option::is_none")]
    pub right_of: Option<Box<Selector>>,
    /// Anchor sub-selector — see [`Modifiers::inside`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inside: Option<Box<Selector>>,
}

/// IndexModifiers — the subset BaseAnchor can stack. Kept separate from
/// [`Modifiers`] so anchor callers can't accidentally write the top-level
/// spatial form (which would be two surfaces for the same intent).
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct IndexModifiers {
    /// Pick the nth match (0-indexed) from the surviving candidates.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub nth: Option<usize>,
    /// Pick the first match (`true`) — overridden by `nth` if both set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub first: Option<bool>,
    /// Pick the last match (`true`) — overridden by `nth` if both set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last: Option<bool>,
}

/// Selector — 6 mutually exclusive base forms.
///
/// Each variant carries its base-shape fields *and* the modifier set
/// stackable on that base. `serde(untagged)` ensures the JSON wire form
/// matches the existing TS Selector wire 1:1 (no `{type: "text", ...}`
/// discriminator — the presence of `text` / `id` / `label` / `role` /
/// `focused` / `anchor` is the discriminator, mirroring TS's
/// `isXSelector` type-guards).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Selector {
    /// `{ text: 'hello' | /^hi/, ...modifiers }`
    Text {
        /// Text pattern (literal or regex).
        text: Pattern,
        /// Stacked spatial + index modifiers.
        #[serde(flatten)]
        modifiers: Modifiers,
    },
    /// `{ id: 'btn-xyz', ...modifiers }`
    Id {
        /// Accessibility identifier (strict equal).
        id: String,
        /// Stacked spatial + index modifiers.
        #[serde(flatten)]
        modifiers: Modifiers,
    },
    /// `{ label: 'Settings', ...modifiers }`
    Label {
        /// Accessibility label (strict equal).
        label: String,
        /// Stacked spatial + index modifiers.
        #[serde(flatten)]
        modifiers: Modifiers,
    },
    /// `{ role: 'button', name?: pattern, ...modifiers }`
    Role {
        /// Semantic role.
        role: Role,
        /// Optional accessible-name pattern (label / title / text scan).
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<Pattern>,
        /// Stacked spatial + index modifiers.
        #[serde(flatten)]
        modifiers: Modifiers,
    },
    /// `{ focused: true }` — no modifiers (focus is runtime-resolved).
    Focused {
        /// Always `true` on the wire. Deserialized as a discriminator.
        focused: True,
    },
    /// `{ anchor: { ...spatial keys }, ...indexModifiers }`
    Anchor {
        /// Spatial-only anchor sub-selectors (AnchorBox has no nth/first/last).
        anchor: AnchorBox,
        /// Index pick applied after spatial filtering.
        #[serde(flatten)]
        index: IndexModifiers,
    },
    /// `{ localized_text: { en: "Submit", ja: "送信", es: "Enviar" }, ...modifiers }`
    /// — v5.18 c1 (a11y-i18n initiative L4 layer). Per-locale text table for
    /// elements whose visible text varies across locales when no stable
    /// accessibilityIdentifier is exposed. Adapter desugars this variant to
    /// `Selector::Text { text: <picked-by-current-locale> }` before passing
    /// to the resolver — the resolver itself never sees `LocalizedText`
    /// (unreachable arm in resolver match). Locale is read from the last
    /// `launchApp -AppleLanguages` argument the adapter parsed; fallback to
    /// "en" when current locale not in table (or no launchApp seen yet).
    LocalizedText {
        /// Locale code → text mapping. Locale codes are bare BCP-47 language
        /// subtag (e.g. "en", "ja", "es"), matching the `(xx)` form
        /// inside `-AppleLanguages "(xx)"`. BTreeMap for stable iteration
        /// + deterministic debug / test output.
        localized_text: BTreeMap<String, String>,
        /// Stacked spatial + index modifiers.
        #[serde(flatten)]
        modifiers: Modifiers,
    },
    /// `{ ocr_text: "Submit", locales: ["en"], ...modifiers }` — v5.19 c1
    /// (a11y-i18n initiative L5 layer). Apple Vision OCR keyword over the
    /// current XCUIScreen screenshot. For elements with no stable
    /// accessibilityIdentifier AND no a11y label (custom-drawn UI / 3rd
    /// party libs that bypass UIAccessibility). Adapter handles dispatch
    /// directly (find_by_text_ocr → IOHID synthesize at frame center),
    /// bypassing the standard resolver pipeline — `OcrText` never reaches
    /// resolver match arms. Locales are BCP-47 subtags; empty array
    /// defaults to adapter's `last_locale` (which itself defaults to "en").
    OcrText {
        /// Substring keyword to OCR-match against text observations.
        /// Case-insensitive substring match on `topCandidates(1)` of each
        /// `VNRecognizedTextObservation`.
        ocr_text: String,
        /// BCP-47 language subtags for Apple Vision `recognitionLanguages`
        /// (e.g. `["en"]` / `["ja"]`). Empty = adapter fills from
        /// `last_locale`.
        #[serde(default)]
        locales: Vec<String>,
        /// Stacked spatial + index modifiers (mostly nth for ambiguity
        /// disambiguation when multiple text observations match).
        #[serde(flatten)]
        modifiers: Modifiers,
    },
    /// `{ anchored: { anchor: <selector>, dx: -0.15, dy: 0 } }` — v5.20 c1
    /// (a11y-i18n initiative L6 layer). Anchor-relative coord for icon-only
    /// / pure-graphic elements (~15-20% of "lib without a11y" scenarios)
    /// where no sense layer can locate the target directly, but a STABLE
    /// nearby element (status label, header text, etc.) is sense-able.
    ///
    /// Resolution: adapter finds `anchor` via host-resolve → anchor center
    /// in normalized `[0, 1]` viewport coords → add `(dx, dy)` shift (also
    /// normalized — `dx: 0.15` = 15% of screen width to the right) → clamp
    /// to `[0, 1]` → IOHID synthesize via tap_at_norm_coord.
    ///
    /// Compared to `Selector::Anchor` (spatial-relation chain — "find a
    /// node near/below/leftOf `<anchor>`"), `AnchorRelative` is an escape
    /// hatch family (§9 #3 sibling to `tap_at_coord`) — the target itself
    /// has no a11y form, only the anchor does. Adapter dispatches directly
    /// (find_norm_coord(anchor) → tap_at_coord), bypassing resolver.
    AnchorRelative {
        /// Anchor selector — any other Selector form (Id / Text / Role /
        /// LocalizedText / etc.). Must resolve to exactly one node;
        /// adapter uses centroid.
        anchor: Box<Selector>,
        /// X shift from anchor center in viewport-normalized space
        /// (`dx: 0.15` = 15% of screen width to the right; `dx: -0.15`
        /// = 15% to the left).
        dx: f64,
        /// Y shift from anchor center in viewport-normalized space
        /// (`dy: 0.05` = 5% of screen height downward).
        dy: f64,
    },
    /// `{ point: [0.5, 0.9] }` — v5.20 c2 (a11y-i18n initiative L7 layer,
    /// last-resort within fallback chain). Direct viewport-normalized
    /// coord. Equivalent to `Step::TapAtPoint` for standalone use, but
    /// expressed as a Selector for uniform composition inside
    /// `Selector::Fallback { chain: Vec<Selector> }`. Adapter dispatches
    /// directly via tap_at_coord; never reaches resolver.
    Point {
        /// X in viewport-normalized [0, 1].
        nx: f64,
        /// Y in viewport-normalized [0, 1].
        ny: f64,
    },
    /// `{ fallback: [<selector1>, <selector2>, ...] }` — v5.20 c2 (a11y-i18n
    /// initiative L7 layer). Sequential resolution chain — adapter tries
    /// each sub-selector in order; first hit wins. Misses are recorded
    /// for AI-readable error report (`sense_layer` / `missing_id_hint` /
    /// `suggested_fixes`). On all-miss, surfaces ElementNotFound with a
    /// chain-trace hint listing every attempted layer.
    ///
    /// Composes L1-L7 into a single tapOn — typical pattern:
    /// ```yaml
    /// tapOn:
    ///   fallback:
    ///     - { id: "submit-btn" }                                # L1
    ///     - { localized_text: { en: "Submit", ja: "送信" } }   # L4
    ///     - { ocrText: "Submit" }                                # L5
    ///     - { anchored: { anchor: { id: "form-area" }, dx: 0.0, dy: 0.05 } }  # L6
    ///     - { point: [0.5, 0.9] }                                # L7
    /// ```
    /// Adapter dispatches each sub-selector through the standard run_tap
    /// path; chain element types are limited only by what run_tap can
    /// dispatch (any concrete Selector variant). Nesting Fallback inside
    /// Fallback is allowed but discouraged (no defined precedence beyond
    /// outer-then-inner).
    Fallback {
        /// Sequential chain of sub-selectors. First hit wins.
        fallback: Vec<Selector>,
    },
}

/// Newtype around `true` — used as the [`Selector::Focused`] discriminator.
/// Serializes/deserializes as the JSON literal `true`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct True(pub bool);

impl Default for True {
    fn default() -> Self {
        True(true)
    }
}

// -------------------- match_text -----------------------------------------

/// Text matching against an [`A11yNode`]. 1:1 with TS
/// `src/core/resolve-selector.ts:153-184` (matchText).
///
/// Scans 6 candidate fields in priority order — `label`, `title`, `value`,
/// `placeholder_value`, `identifier`, `text` (跟 maestro `IOSDriver.kt:192-210`
/// 同源 5 字段 + smix `text` backward-compat).
///
/// Case handling:
/// - `Pattern::Text("")` returns false (empty pattern matches nothing).
/// - `Pattern::Text("X")` does ASCII-case-insensitive strict equal
///   against each candidate (TS uses `toLowerCase()` strict equal; we
///   `eq_ignore_ascii_case` because RN labels are ASCII-dominant).
/// - `Pattern::Regex {regex, flags}` compiles the regex; if `flags` lacks
///   `i`, injects it (matches TS auto-/i v1.5 c5i-d).
///
/// Regex compile errors short-circuit to `false` (TS `new RegExp` throws —
/// we choose silent false to keep `match_text` infallible at the stone
/// layer; the resolver layer can wrap with explicit error reporting).
///
/// Pure / branch-only / one Vec scan — should be 5-50 ns text path, 50-200
/// ns regex path after compile-once amortization.
/// Hot-path match using a pre-compiled pattern. Use this inside any
/// resolver / DFS loop after calling [`Pattern::compile`] once.
///
/// Pure / branch-only / one Vec scan. Regex variant runs cached DFA, so
/// per-call cost is ~10-50 ns (vs ~17 μs for the compile-on-call
/// [`match_text`]).
#[must_use]
pub fn match_text_compiled(node: &A11yNode, pattern: &CompiledPattern) -> bool {
    let candidates: [Option<&str>; 6] = [
        node.label.as_deref(),
        node.title.as_deref(),
        node.value.as_deref(),
        node.placeholder_value.as_deref(),
        node.identifier.as_deref(),
        node.text.as_deref(),
    ];
    match pattern {
        CompiledPattern::Text(p) => {
            if p.is_empty() {
                return false;
            }
            for c in candidates.iter().flatten() {
                if c.eq_ignore_ascii_case(p) {
                    return true;
                }
            }
            false
        }
        CompiledPattern::Regex(re) => {
            for c in candidates.iter().flatten() {
                if re.is_match(c) {
                    return true;
                }
            }
            false
        }
    }
}

// -------------------- describe_selector ----------------------------------

/// Human / AI-readable rendering of a [`Selector`] for error prompts and
/// logs. Stable enough that AI can pattern-match against past failures.
/// Mirrors TS `describeSelector` in `src/core/selector.ts:118-160` 1:1.
#[must_use]
pub fn describe_selector(s: &Selector) -> String {
    let mut parts: Vec<String> = Vec::new();
    match s {
        Selector::Anchor { anchor, index } => {
            if let Some(v) = anchor.near.as_deref() {
                parts.push(format!("anchor.near=({})", describe_selector(v)));
            }
            if let Some(v) = anchor.below.as_deref() {
                parts.push(format!("anchor.below=({})", describe_selector(v)));
            }
            if let Some(v) = anchor.above.as_deref() {
                parts.push(format!("anchor.above=({})", describe_selector(v)));
            }
            if let Some(v) = anchor.left_of.as_deref() {
                parts.push(format!("anchor.leftOf=({})", describe_selector(v)));
            }
            if let Some(v) = anchor.right_of.as_deref() {
                parts.push(format!("anchor.rightOf=({})", describe_selector(v)));
            }
            if let Some(v) = anchor.inside.as_deref() {
                parts.push(format!("anchor.inside=({})", describe_selector(v)));
            }
            if parts.is_empty() {
                parts.push("anchor.empty".into());
            }
            if let Some(nth) = index.nth {
                parts.push(format!("nth={}", nth));
            }
            if index.first == Some(true) {
                parts.push("first".into());
            }
            if index.last == Some(true) {
                parts.push("last".into());
            }
            return format!("{{ {} }}", parts.join(", "));
        }
        Selector::Focused { .. } => return "{ focused }".into(),
        Selector::Text { text, modifiers } => {
            parts.push(format!("text={}", format_pattern(text)));
            append_modifiers(modifiers, &mut parts);
        }
        Selector::Id { id, modifiers } => {
            parts.push(format!("id=\"{}\"", id));
            append_modifiers(modifiers, &mut parts);
        }
        Selector::Label { label, modifiers } => {
            parts.push(format!("label=\"{}\"", label));
            append_modifiers(modifiers, &mut parts);
        }
        Selector::Role {
            role,
            name,
            modifiers,
        } => {
            match name {
                Some(n) => parts.push(format!(
                    "role={}, name={}",
                    role.as_str(),
                    format_pattern(n)
                )),
                None => parts.push(format!("role={}", role.as_str())),
            }
            append_modifiers(modifiers, &mut parts);
        }
        Selector::LocalizedText {
            localized_text,
            modifiers,
        } => {
            let entries: Vec<String> = localized_text
                .iter()
                .map(|(k, v)| format!("{}=\"{}\"", k, v))
                .collect();
            parts.push(format!("localized_text={{{}}}", entries.join(", ")));
            append_modifiers(modifiers, &mut parts);
        }
        Selector::OcrText {
            ocr_text,
            locales,
            modifiers,
        } => {
            if locales.is_empty() {
                parts.push(format!("ocr_text=\"{}\"", ocr_text));
            } else {
                parts.push(format!(
                    "ocr_text=\"{}\", locales=[{}]",
                    ocr_text,
                    locales.join(", ")
                ));
            }
            append_modifiers(modifiers, &mut parts);
        }
        Selector::AnchorRelative { anchor, dx, dy } => {
            parts.push(format!(
                "anchored.anchor=({}), dx={}, dy={}",
                describe_selector(anchor),
                dx,
                dy
            ));
        }
        Selector::Point { nx, ny } => {
            parts.push(format!("point=({}, {})", nx, ny));
        }
        Selector::Fallback { fallback } => {
            let chain: Vec<String> = fallback.iter().map(describe_selector).collect();
            parts.push(format!("fallback=[{}]", chain.join(", ")));
        }
    }
    format!("{{ {} }}", parts.join(", "))
}

fn append_modifiers(m: &Modifiers, parts: &mut Vec<String>) {
    if let Some(v) = m.near.as_deref() {
        parts.push(format!("near=({})", describe_selector(v)));
    }
    if let Some(v) = m.below.as_deref() {
        parts.push(format!("below=({})", describe_selector(v)));
    }
    if let Some(v) = m.above.as_deref() {
        parts.push(format!("above=({})", describe_selector(v)));
    }
    if let Some(v) = m.left_of.as_deref() {
        parts.push(format!("leftOf=({})", describe_selector(v)));
    }
    if let Some(v) = m.right_of.as_deref() {
        parts.push(format!("rightOf=({})", describe_selector(v)));
    }
    if let Some(v) = m.inside.as_deref() {
        parts.push(format!("inside=({})", describe_selector(v)));
    }
    if let Some(v) = m.ancestor.as_deref() {
        parts.push(format!("ancestor=({})", describe_selector(v)));
    }
    if let Some(nth) = m.nth {
        parts.push(format!("nth={}", nth));
    }
    if m.first == Some(true) {
        parts.push("first".into());
    }
    if m.last == Some(true) {
        parts.push("last".into());
    }
}

fn format_pattern(p: &Pattern) -> String {
    match p {
        Pattern::Text(s) => format!("{:?}", s),
        Pattern::Regex { regex, flags } => format!("/{}/{}", regex, flags),
    }
}

/// Match a [`Pattern`] against the six text-bearing fields of a node
/// (`label`, `title`, `value`, `placeholderValue`, `identifier`, `text`)
/// — OR semantics, case-insensitive. SDK convenience wrapper that
/// compiles the pattern on every call; for hot loops prefer
/// [`Pattern::compile`] once + [`match_text_compiled`] per node.
#[must_use]
pub fn match_text(node: &A11yNode, pattern: &Pattern) -> bool {
    let candidates: [Option<&str>; 6] = [
        node.label.as_deref(),
        node.title.as_deref(),
        node.value.as_deref(),
        node.placeholder_value.as_deref(),
        node.identifier.as_deref(),
        node.text.as_deref(),
    ];
    match pattern {
        Pattern::Text(p) => {
            if p.is_empty() {
                return false;
            }
            for c in candidates.iter().flatten() {
                if c.eq_ignore_ascii_case(p) {
                    return true;
                }
            }
            false
        }
        Pattern::Regex { regex, flags: _ } => {
            // v1.5 c5i-d — always case-insensitive (跟 TS resolve-selector.ts
            // line 170-172 同源: flags.includes('i') ? pattern : add /i).
            // The `flags` field is purely wire-shape for round-trip JSON
            // parity with TS RegExp.flags; on the matching side we
            // unconditionally prepend `(?i)` (or skip if user already
            // wrote it) because Rust `regex::Regex::new` doesn't accept
            // a separate flags arg — case folding goes in the pattern
            // as the `(?i)` inline group.
            let final_pattern = if regex.starts_with("(?i)") {
                regex.clone()
            } else {
                format!("(?i){}", regex)
            };
            let Ok(re) = regex::Regex::new(&final_pattern) else {
                return false;
            };
            for c in candidates.iter().flatten() {
                if re.is_match(c) {
                    return true;
                }
            }
            false
        }
    }
}

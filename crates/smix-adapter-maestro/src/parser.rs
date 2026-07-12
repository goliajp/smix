//! Hand-written maestro yaml → [`Flow`] parser.
//!
//! Why hand-written (vs `serde(untagged)` derive): each command key is
//! the discriminator, and the same key (e.g. `tapOn`) accepts both a
//! scalar and a map. `untagged` enums would either become ambiguous or
//! silently swallow malformed input — both unacceptable per CLAUDE.md
//! §13 (quality / arch clean > research cost). Walking the
//! `serde_norway::Value` tree by hand keeps the dispatch explicit,
//! lets us surface a precise [`ParseError`] for every malformed shape,
//! and mirrors the maestro Kotlin parser layout 1:1.

use crate::{Flow, MaestroPermissionAction, ParseError, RepeatMode, Step};
use serde::Deserialize;
use serde_norway::Value;
use smix_selector::{Modifiers, Pattern, Role, Selector};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

// v1.0.20 D2 — parse the yaml `role:` value into a [`Role`] variant.
//
// Docs (`docs/ai-guide/03-selectors.md §4 Role`) show docs-friendly
// lowercase forms (`role: button`, `role: textfield`, `role: checkbox`),
// but the wire schema uses camelCase (`textField`, `checkBox`).
// Accept BOTH — docs-friendly aliases are canonicalised to the
// camelCase wire form before dispatch. Unknown values return
// `ParseError::InvalidValue` with the full accepted list so the
// consumer sees exactly what smix speaks.
fn parse_role_yaml(v: &Value, ctx: &str) -> Result<Role, ParseError> {
    let s = v.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: format!("{ctx}.role"),
        reason: format!("expected role name string, got {v:?}"),
    })?;
    // Case-tolerant lookup. The wire is camelCase; docs are lowercase.
    // Snake_case (`text_field`) is also tolerated since insight uses it
    // in `.smix/config.yaml`-adjacent selectors.
    let normalized: String = s
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect();
    let role = match normalized.as_str() {
        "button" => Role::Button,
        "link" => Role::Link,
        "textfield" => Role::TextField,
        "securetextfield" => Role::SecureTextField,
        "searchfield" => Role::SearchField,
        "switch" => Role::Switch,
        "toggle" => Role::Toggle,
        "checkbox" => Role::CheckBox,
        "radio" | "radiobutton" => Role::Radio,
        "image" => Role::Image,
        // Docs mention `heading` as an accepted role. iOS/SwiftUI has no
        // `.header` XCUIElement type; heading semantics collapse to
        // static text with a heading trait, which the resolver treats
        // as `StaticText` since that is what the a11y tree emits.
        "heading" | "statictext" => Role::StaticText,
        "tab" => Role::Tab,
        "tabbar" => Role::TabBar,
        "navigationbar" => Role::NavigationBar,
        "cell" => Role::Cell,
        "alert" => Role::Alert,
        "dialog" => Role::Dialog,
        "slider" => Role::Slider,
        "progressbar" | "progressindicator" => Role::ProgressBar,
        "picker" => Role::Picker,
        "menu" => Role::Menu,
        "menuitem" => Role::MenuItem,
        "scrollview" => Role::ScrollView,
        "segmentedcontrol" => Role::SegmentedControl,
        "table" => Role::Table,
        "collectionview" => Role::CollectionView,
        "webview" => Role::WebView,
        "keyboard" => Role::Keyboard,
        _ => {
            return Err(ParseError::InvalidValue {
                field: format!("{ctx}.role"),
                reason: format!(
                    "unknown role `{s}`; accepted: button, link, textField, secureTextField, searchField, switch, toggle, checkBox, radio, image, staticText (or heading), tab, tabBar, navigationBar, cell, alert, dialog, slider, progressBar, picker, menu, menuItem, scrollView, segmentedControl, table, collectionView, webView, keyboard"
                ),
            });
        }
    };
    Ok(role)
}

// v1.0.23 D4 — env-var opt-in for the bare-string auto-OCR desugar.
// Reading the env at parse time (not at run time) keeps the emitted
// Selector shape stable across a flow — you can't have "sometimes
// this yaml parses to Text, sometimes to Fallback" depending on
// runtime state, which would violate the parser's determinism
// contract. Consumer sets `SMIX_AUTO_OCR_FALLBACK=1` in the shell
// environment before invoking `smix run`. Reset by unsetting the var
// or setting it to any value other than `1` / `true`.
fn auto_ocr_fallback_enabled() -> bool {
    matches!(
        std::env::var("SMIX_AUTO_OCR_FALLBACK").ok().as_deref(),
        Some("1" | "true" | "TRUE" | "yes")
    )
}

// v1.0.25 D1 — split a bare string on top-level `|` for D4's auto-
// OCR-fallback lift. Preserves user's regex-OR intent while giving
// Apple Vision literal strings it can actually match.
//
// "Top-level" means we skip `|` inside `[...]` character classes
// and after `\` escapes. Any string without `|` returns a
// singleton [s]. Empty alternatives are filtered out — `'||A'`
// yields ['A'], not `['', '', 'A']`.
fn split_top_level_pipe(s: &str) -> Vec<&str> {
    if !s.contains('|') {
        return vec![s];
    }
    let bytes = s.as_bytes();
    let mut alts: Vec<&str> = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    let mut i = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if i + 1 < bytes.len() => i += 1, // skip escaped char
            b'[' => depth += 1,
            b']' if depth > 0 => depth -= 1,
            b'|' if depth == 0 => {
                let slice = &s[start..i];
                if !slice.is_empty() {
                    alts.push(slice);
                }
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    let tail = &s[start..];
    if !tail.is_empty() {
        alts.push(tail);
    }
    if alts.is_empty() {
        // All-`|` string (e.g. `"||"`): degenerate. Return original
        // so callers still get a probe.
        alts.push(s);
    }
    alts
}

// v1.0.20 D2 — parse `name:` sub-key (companion to `role:`) into an
// optional [`Pattern`]. Accepts a scalar string (plain literal OR
// pipe-alternation regex, same rules as `text_to_pattern`).
fn parse_role_name_yaml(map: &serde_norway::Mapping, ctx: &str) -> Result<Option<Pattern>, ParseError> {
    let raw = match map.get(Value::String("name".into())) {
        Some(v) => v,
        None => return Ok(None),
    };
    let s = raw.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: format!("{ctx}.name"),
        reason: format!("expected pattern string, got {raw:?}"),
    })?;
    Ok(Some(text_to_pattern(s)))
}

// v5.20 c2 — parse a single chain element of `fallback: [...]`. Reuses
// the same map-shape dispatcher as parse_tap_on's selector arm — accepts
// `{id}`, `{text}`, `{localized_text}`, `{ocrText}`, `{anchored}`,
// `{point: "X%,Y%"}`. Returns the appropriate Selector variant
// (point → Selector::Point; others → their corresponding variant).
//
// Returns InvalidValue when shape is none of the above.
fn parse_fallback_element(v: &Value, field: &str) -> Result<Selector, ParseError> {
    let map = match v {
        Value::Mapping(m) => m,
        other => {
            return Err(ParseError::InvalidValue {
                field: field.into(),
                reason: format!("fallback chain element expected map, got {other:?}"),
            });
        }
    };
    // point: "X%,Y%" / [nx, ny]
    if let Some(p) = map.get(Value::String("point".into())) {
        let (nx, ny) = match p {
            Value::String(s) => parse_point(s)?,
            Value::Sequence(seq) if seq.len() == 2 => {
                let nx = seq[0].as_f64().ok_or_else(|| ParseError::InvalidValue {
                    field: format!("{field}.point[0]"),
                    reason: "point[0] expected f64".into(),
                })?;
                let ny = seq[1].as_f64().ok_or_else(|| ParseError::InvalidValue {
                    field: format!("{field}.point[1]"),
                    reason: "point[1] expected f64".into(),
                })?;
                (nx, ny)
            }
            other => {
                return Err(ParseError::InvalidValue {
                    field: format!("{field}.point"),
                    reason: format!(
                        "point expected 'X%,Y%' string or [nx, ny] array, got {other:?}"
                    ),
                });
            }
        };
        return Ok(Selector::Point { nx, ny });
    }
    if let Some(id) = map.get(Value::String("id".into())).and_then(Value::as_str) {
        return Ok(Selector::Id {
            id: id.to_string(),
            modifiers: Modifiers::default(),
        });
    }
    if let Some(text) = map
        .get(Value::String("text".into()))
        .and_then(Value::as_str)
    {
        return Ok(Selector::Text {
            text: text_to_pattern(text),
            modifiers: Modifiers::default(),
        });
    }
    if let Some(loc_map) = map
        .get(Value::String("localized_text".into()))
        .and_then(Value::as_mapping)
    {
        let table = parse_localized_table(loc_map, &format!("{field}.localized_text"))?;
        return Ok(Selector::LocalizedText {
            localized_text: table,
            modifiers: Modifiers::default(),
        });
    }
    if let Some(raw) = map.get(Value::String("ocrText".into())) {
        let (text, locales) = parse_ocr_text(raw, &format!("{field}.ocrText"))?;
        return Ok(Selector::OcrText {
            ocr_text: text,
            locales,
            modifiers: Modifiers::default(),
        });
    }
    if let Some(raw) = map.get(Value::String("anchored".into())) {
        let (anchor, dx, dy) = parse_anchored(raw, &format!("{field}.anchored"))?;
        return Ok(Selector::AnchorRelative {
            anchor: Box::new(anchor),
            dx,
            dy,
        });
    }
    Err(ParseError::InvalidValue {
        field: field.into(),
        reason: "fallback chain element expected one of: id / text / localized_text / ocrText / anchored / point".into(),
    })
}

// v5.20 c2 — parse `fallback: [...]` yaml value into Vec<Selector>.
// Each element is a single-selector map (id / text / localized_text /
// ocrText / anchored / point). Empty list → InvalidValue (no chain to
// try). Last element should be a stable fallback (typically point).
fn parse_fallback_chain(v: &Value, field: &str) -> Result<Vec<Selector>, ParseError> {
    let seq = match v {
        Value::Sequence(s) => s,
        other => {
            return Err(ParseError::InvalidValue {
                field: field.into(),
                reason: format!("fallback expected sequence, got {other:?}"),
            });
        }
    };
    if seq.is_empty() {
        return Err(ParseError::InvalidValue {
            field: field.into(),
            reason: "fallback chain must not be empty".into(),
        });
    }
    let mut chain = Vec::with_capacity(seq.len());
    for (i, elem) in seq.iter().enumerate() {
        chain.push(parse_fallback_element(elem, &format!("{field}[{i}]"))?);
    }
    Ok(chain)
}

// v5.20 c1 — parse `anchored: { anchor: <selector>, dx: <f>, dy: <f> }`
// yaml value into (anchor Selector, dx, dy). anchor sub-selector accepts
// `{id|text|label|role}` map (reuses visible_to_selector helper). dx/dy
// are required f64s in normalized [0,1] viewport space (negatives OK for
// left/up shift).
fn parse_anchored(v: &Value, field: &str) -> Result<(Selector, f64, f64), ParseError> {
    let map = match v {
        Value::Mapping(m) => m,
        other => {
            return Err(ParseError::InvalidValue {
                field: field.into(),
                reason: format!("anchored expected map, got {other:?}"),
            });
        }
    };
    let anchor_raw =
        map.get(Value::String("anchor".into()))
            .ok_or_else(|| ParseError::InvalidValue {
                field: format!("{field}.anchor"),
                reason: "anchored.anchor required".into(),
            })?;
    let anchor = visible_to_selector(anchor_raw)?;
    let dx = map
        .get(Value::String("dx".into()))
        .and_then(Value::as_f64)
        .ok_or_else(|| ParseError::InvalidValue {
            field: format!("{field}.dx"),
            reason: "anchored.dx required (f64)".into(),
        })?;
    let dy = map
        .get(Value::String("dy".into()))
        .and_then(Value::as_f64)
        .ok_or_else(|| ParseError::InvalidValue {
            field: format!("{field}.dy"),
            reason: "anchored.dy required (f64)".into(),
        })?;
    Ok((anchor, dx, dy))
}

// v5.19 c1 — parse `ocrText:` yaml value into (text, locales). Accepts
// short form `ocrText: "Submit"` and full form
// `ocrText: { text: "送信", locales: ["ja"] }`. Returns (text, locales)
// where locales is empty Vec when not specified (adapter fills from
// last_locale). text must be non-empty.
fn parse_ocr_text(v: &Value, field: &str) -> Result<(String, Vec<String>), ParseError> {
    match v {
        // short form
        Value::String(s) => {
            if s.is_empty() {
                return Err(ParseError::InvalidValue {
                    field: field.into(),
                    reason: "ocrText must be non-empty".into(),
                });
            }
            Ok((s.clone(), Vec::new()))
        }
        // full form { text, locales? }
        Value::Mapping(m) => {
            let text = m
                .get(Value::String("text".into()))
                .and_then(Value::as_str)
                .ok_or_else(|| ParseError::InvalidValue {
                    field: format!("{field}.text"),
                    reason: "ocrText.text must be string".into(),
                })?;
            if text.is_empty() {
                return Err(ParseError::InvalidValue {
                    field: format!("{field}.text"),
                    reason: "ocrText.text must be non-empty".into(),
                });
            }
            let locales = if let Some(arr) = m
                .get(Value::String("locales".into()))
                .and_then(Value::as_sequence)
            {
                arr.iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            } else {
                Vec::new()
            };
            Ok((text.to_string(), locales))
        }
        other => Err(ParseError::InvalidValue {
            field: field.into(),
            reason: format!("ocrText expected string or map, got {other:?}"),
        }),
    }
}

// v5.18 c1 — parse `{ en: "...", ja: "...", es: "..." }` per-locale text
// table into a BTreeMap<String, String>. Keys must be non-empty strings;
// values must be non-empty strings; empty table → InvalidValue.
fn parse_localized_table(
    map: &serde_norway::Mapping,
    field: &str,
) -> Result<BTreeMap<String, String>, ParseError> {
    let mut table = BTreeMap::new();
    for (k, v) in map {
        let key = k.as_str().ok_or_else(|| ParseError::InvalidValue {
            field: field.into(),
            reason: format!("locale key must be string, got {:?}", k),
        })?;
        let value = v.as_str().ok_or_else(|| ParseError::InvalidValue {
            field: format!("{field}.{key}"),
            reason: format!("locale value must be string, got {:?}", v),
        })?;
        if key.is_empty() || value.is_empty() {
            return Err(ParseError::InvalidValue {
                field: field.into(),
                reason: "locale key + value both must be non-empty".into(),
            });
        }
        table.insert(key.to_string(), value.to_string());
    }
    if table.is_empty() {
        return Err(ParseError::InvalidValue {
            field: field.into(),
            reason: "localized_text table must not be empty".into(),
        });
    }
    Ok(table)
}

// -------------------- Pattern / Selector helpers -------------------------

/// Infer a [`Pattern`] from a text body. Maestro treats `|` as regex
/// alternation (mirrors maestro Kotlin `TestRunner.kt` / `XPathSelector.kt`
/// `Regex(text)` semantics); plain bodies stay as case-insensitive
/// literals. Empty bodies stay as empty [`Pattern::Text`] for explicit
/// downstream rejection.
#[must_use]
pub fn text_to_pattern(s: &str) -> Pattern {
    if s.contains('|') {
        Pattern::Regex {
            regex: s.to_string(),
            flags: "i".to_string(),
        }
    } else {
        Pattern::Text(s.to_string())
    }
}

/// Convert a `visible:` value (scalar string or map with a selector
/// sub-key) into a [`Selector`].
///
/// Accepted map keys mirror the base selector table:
/// - `text` — literal or `|`-alternation regex
/// - `id` — accessibilityIdentifier
/// - `label` — accessibilityLabel (strict equal)
/// - `role` — semantic role (+ optional `name:` pattern)
/// - `ocrText` — Vision (iOS) / ML Kit (Android) OCR match
/// - `localized_text` — per-locale text table
/// - `fallback` — sequential selector chain, first hit wins
///
/// # Errors
///
/// Returns [`ParseError::InvalidValue`] if the value is neither a
/// scalar string nor a map containing one of the accepted keys.
///
/// v1.0.20 D1 — pre-v1.0.20 accepted only `text` and `id`, which
/// disagreed with `docs/ai-guide/03-selectors.md §9 OcrText` promising
/// `ocrText` as a first-class selector everywhere selectors appear.
/// Extending here fixes `extendedWaitUntil.visible: {ocrText}`,
/// `assertVisible: {role, name}`, `scrollUntilVisible: {label}`,
/// and every other verb that resolves through this helper (8 sites at
/// time of writing).
pub fn visible_to_selector(v: &Value) -> Result<Selector, ParseError> {
    match v {
        Value::String(s) => {
            // v1.0.23 D4 — bare-string `visible: 'X'` optionally
            // auto-lifts to `visible: fallback: [text: X, ocrText: X]`
            // when `SMIX_AUTO_OCR_FALLBACK=1`. Insight round-2 Ask 7:
            // every one of their 12 flows spelled out the 3-line
            // fallback form; env-opt-in lets them keep bare strings
            // and still get the OCR safety net. Tier order: text
            // first (cheap, hits when tree exposes text), OCR after
            // (~500 ms Vision call, hits when tree is degraded).
            //
            // v1.0.25 D1 — Insight round-4 Ask 11: when the bare
            // string contains `|`, `text_to_pattern` treats it as a
            // regex OR (`/A|B/i`), which is right for the tree tier
            // but WRONG for the OCR tier — Apple Vision does not
            // interpret pipes; the literal string `"A|B"` is never
            // on screen. Result before v1.0.25: `visible: 'A|B'`
            // under D4 silently missed OCR every time. Fix: split on
            // top-level `|` and emit one `OcrText` per alternative
            // AFTER the single regex text tier:
            //   `fallback: [Text('/A|B/i'), OcrText('A'), OcrText('B')]`
            // The tree tier still covers "either A or B" in one
            // probe; OCR now has real strings to search for.
            //
            // "Top-level `|`" means we don't try to parse
            // `[A|B]` character classes or `\|` escapes — those
            // stay as-is on the text tier and don't split. Any
            // string containing `|` under normal user yaml intent
            // is a simple `A|B|C` OR, and that's what we handle.
            //
            // Non-empty check: an empty selector doesn't get auto-
            // lifted — bubbles to the same Selector::Text as before
            // and gets rejected by validators downstream.
            if auto_ocr_fallback_enabled() && !s.is_empty() {
                let text_layer = Selector::Text {
                    text: text_to_pattern(s),
                    modifiers: Modifiers::default(),
                };
                let mut fallback = Vec::with_capacity(4);
                fallback.push(text_layer);
                for alt in split_top_level_pipe(s) {
                    fallback.push(Selector::OcrText {
                        ocr_text: alt.to_string(),
                        locales: Vec::new(),
                        modifiers: Modifiers::default(),
                    });
                }
                return Ok(Selector::Fallback { fallback });
            }
            Ok(Selector::Text {
                text: text_to_pattern(s),
                modifiers: Modifiers::default(),
            })
        },
        Value::Mapping(map) => {
            if let Some(text) = map
                .get(Value::String("text".into()))
                .and_then(Value::as_str)
            {
                return Ok(Selector::Text {
                    text: text_to_pattern(text),
                    modifiers: Modifiers::default(),
                });
            }
            if let Some(id) = map.get(Value::String("id".into())).and_then(Value::as_str) {
                return Ok(Selector::Id {
                    id: id.to_string(),
                    modifiers: Modifiers::default(),
                });
            }
            if let Some(label) = map
                .get(Value::String("label".into()))
                .and_then(Value::as_str)
            {
                return Ok(Selector::Label {
                    label: label.to_string(),
                    modifiers: Modifiers::default(),
                });
            }
            if let Some(role_raw) = map.get(Value::String("role".into())) {
                let role = parse_role_yaml(role_raw, "visible")?;
                let name = parse_role_name_yaml(map, "visible")?;
                return Ok(Selector::Role {
                    role,
                    name,
                    modifiers: Modifiers::default(),
                });
            }
            if let Some(raw) = map.get(Value::String("ocrText".into())) {
                let (text, locales) = parse_ocr_text(raw, "visible.ocrText")?;
                return Ok(Selector::OcrText {
                    ocr_text: text,
                    locales,
                    modifiers: Modifiers::default(),
                });
            }
            if let Some(loc_map) = map
                .get(Value::String("localized_text".into()))
                .and_then(Value::as_mapping)
            {
                let table = parse_localized_table(loc_map, "visible.localized_text")?;
                return Ok(Selector::LocalizedText {
                    localized_text: table,
                    modifiers: Modifiers::default(),
                });
            }
            if let Some(raw) = map.get(Value::String("fallback".into())) {
                let chain = parse_fallback_chain(raw, "visible.fallback")?;
                return Ok(Selector::Fallback { fallback: chain });
            }
            Err(ParseError::InvalidValue {
                field: "visible".into(),
                reason: "expected one of `text`, `id`, `label`, `role`, `ocrText`, `localized_text`, `fallback` keys".into(),
            })
        }
        other => Err(ParseError::InvalidValue {
            field: "visible".into(),
            reason: format!("expected string or map, got {other:?}"),
        }),
    }
}

// -------------------- per-command parsers --------------------------------

fn parse_tap_on(v: &Value) -> Result<Step, ParseError> {
    match v {
        // short form: `tapOn: "Counting"`
        Value::String(s) => Ok(Step::TapOn {
            selector: Selector::Text {
                text: text_to_pattern(s),
                modifiers: Modifiers::default(),
            },
            optional: false,
        }),
        // full form: `tapOn: { id|text|point, index?, optional? }`
        Value::Mapping(map) => {
            let optional = map
                .get(Value::String("optional".into()))
                .and_then(Value::as_bool)
                .unwrap_or(false);

            let index = map
                .get(Value::String("index".into()))
                .and_then(Value::as_u64)
                .map(|n| n as usize);

            // point form is escape hatch — independent variant
            if let Some(point) = map
                .get(Value::String("point".into()))
                .and_then(Value::as_str)
            {
                let (nx, ny) = parse_point(point)?;
                return Ok(Step::TapAtPoint { nx, ny });
            }

            let modifiers = Modifiers {
                nth: index,
                ..Modifiers::default()
            };

            if let Some(text) = map
                .get(Value::String("text".into()))
                .and_then(Value::as_str)
            {
                return Ok(Step::TapOn {
                    selector: Selector::Text {
                        text: text_to_pattern(text),
                        modifiers,
                    },
                    optional,
                });
            }
            if let Some(id) = map.get(Value::String("id".into())).and_then(Value::as_str) {
                return Ok(Step::TapOn {
                    selector: Selector::Id {
                        id: id.to_string(),
                        modifiers,
                    },
                    optional,
                });
            }
            // v5.18 c1 — `localized_text: { en: "...", ja: "...", es: "..." }`
            // per-locale text table. Adapter desugars to Selector::Text
            // before dispatch based on last_locale state.
            if let Some(loc_map) = map
                .get(Value::String("localized_text".into()))
                .and_then(Value::as_mapping)
            {
                let table = parse_localized_table(loc_map, "tapOn.localized_text")?;
                return Ok(Step::TapOn {
                    selector: Selector::LocalizedText {
                        localized_text: table,
                        modifiers,
                    },
                    optional,
                });
            }
            // v5.19 c1 — `ocrText: "Submit"` (short) or
            // `ocrText: { text: "送信", locales: ["ja"] }` (full). Apple Vision
            // OCR sense layer (L5). Adapter dispatches directly via
            // App::find_by_text_ocr + tap_at_norm_coord, bypassing resolver.
            if let Some(raw) = map.get(Value::String("ocrText".into())) {
                let (text, locales) = parse_ocr_text(raw, "tapOn.ocrText")?;
                return Ok(Step::TapOn {
                    selector: Selector::OcrText {
                        ocr_text: text,
                        locales,
                        modifiers,
                    },
                    optional,
                });
            }
            // v5.20 c1 — `anchored: { anchor: {<selector>}, dx: <f>, dy: <f> }`
            // (escape hatch family L6). Resolve anchor centroid + (dx, dy)
            // normalized shift → tap_at_norm_coord. Adapter dispatches
            // directly; resolver never sees AnchorRelative.
            if let Some(raw) = map.get(Value::String("anchored".into())) {
                let (anchor, dx, dy) = parse_anchored(raw, "tapOn.anchored")?;
                return Ok(Step::TapOn {
                    selector: Selector::AnchorRelative {
                        anchor: Box::new(anchor),
                        dx,
                        dy,
                    },
                    optional,
                });
            }
            // v5.20 c2 — `fallback: [<selector1>, <selector2>, ...]` (L7
            // sequential chain). Adapter iterates chain, first hit wins.
            if let Some(raw) = map.get(Value::String("fallback".into())) {
                let chain = parse_fallback_chain(raw, "tapOn.fallback")?;
                return Ok(Step::TapOn {
                    selector: Selector::Fallback { fallback: chain },
                    optional,
                });
            }
            // v1.0.20 D2 — `role: <role>` (+ optional `name:` pattern).
            // Wire type `Selector::Role` has existed since v5.x for the
            // resolver path; docs promised the yaml shape but the
            // parser did not accept it. Now it does.
            if let Some(role_raw) = map.get(Value::String("role".into())) {
                let role = parse_role_yaml(role_raw, "tapOn")?;
                let name = parse_role_name_yaml(map, "tapOn")?;
                return Ok(Step::TapOn {
                    selector: Selector::Role {
                        role,
                        name,
                        modifiers,
                    },
                    optional,
                });
            }
            // v1.0.20 D2 — `label: <string>` for accessibilityLabel (iOS)
            // / contentDescription (Android) strict equal. Same
            // documented-but-unimplemented gap as `role:`.
            if let Some(label) = map
                .get(Value::String("label".into()))
                .and_then(Value::as_str)
            {
                return Ok(Step::TapOn {
                    selector: Selector::Label {
                        label: label.to_string(),
                        modifiers,
                    },
                    optional,
                });
            }

            Err(ParseError::InvalidValue {
                field: "tapOn".into(),
                reason: "expected `text`, `id`, `label`, `role`, `point`, `localized_text`, `ocrText`, `anchored`, or `fallback` key".into(),
            })
        }
        other => Err(ParseError::InvalidValue {
            field: "tapOn".into(),
            reason: format!("expected string or map, got {other:?}"),
        }),
    }
}

fn parse_point(s: &str) -> Result<(f64, f64), ParseError> {
    let parts: Vec<&str> = s.split(',').map(str::trim).collect();
    if parts.len() != 2 {
        return Err(ParseError::InvalidValue {
            field: "tapOn.point".into(),
            reason: format!("expected `X%,Y%`, got `{s}`"),
        });
    }
    let nx = parse_percent(parts[0], "tapOn.point.x")?;
    let ny = parse_percent(parts[1], "tapOn.point.y")?;
    Ok((nx, ny))
}

fn parse_percent(s: &str, field: &str) -> Result<f64, ParseError> {
    let trimmed = s.trim_end_matches('%');
    trimmed
        .parse::<f64>()
        .map(|v| v / 100.0)
        .map_err(|e| ParseError::InvalidValue {
            field: field.into(),
            reason: format!("not a number ({e}): `{s}`"),
        })
}

fn parse_run_flow(v: &Value) -> Result<Step, ParseError> {
    match v {
        // short form: `runFlow: ../path.yaml`
        Value::String(s) => Ok(Step::RunFlow(s.clone())),
        // full forms:
        //   - `runFlow: { when: { visible }, file, as }`        → RunFlowConditional
        //   - `runFlow: { when: { visible }, commands: [...] }` → RunFlowInline (v6.8 c1)
        Value::Mapping(map) => {
            let has_file = map.get(Value::String("file".into())).is_some();
            let has_commands = map.get(Value::String("commands".into())).is_some();
            if has_file && has_commands {
                return Err(ParseError::InvalidValue {
                    field: "runFlow".into(),
                    reason: "expected exactly one of `file` or `commands`, got both".into(),
                });
            }

            let when_visible = parse_run_flow_when_visible(map)?;
            let when_not_visible = parse_run_flow_when_not_visible(map)?;
            // v1.0.24 D2 — both gates set at once is ambiguous;
            // reject at parse time with a clear message so consumers
            // don't accidentally combine.
            if when_visible.is_some() && when_not_visible.is_some() {
                return Err(ParseError::InvalidValue {
                    field: "runFlow.when".into(),
                    reason: "`visible` and `notVisible` are mutually exclusive; use one".into(),
                });
            }

            if has_commands {
                // v6.8 c1 — inline commands form (maestro YamlRunFlow's
                // `commands:` alternative). `as:` is rejected here —
                // alias capture is tied to subflow pasteboard handoff,
                // which inline body has no boundary for.
                if map.get(Value::String("as".into())).is_some() {
                    return Err(ParseError::InvalidValue {
                        field: "runFlow.as".into(),
                        reason:
                            "`as` alias is only valid with `runFlow.file`, not inline `commands`"
                                .into(),
                    });
                }
                let commands_val = map
                    .get(Value::String("commands".into()))
                    .expect("has_commands true");
                let steps = parse_step_sequence(commands_val, "runFlow.commands")?;
                return Ok(Step::RunFlowInline {
                    when_visible,
                    when_not_visible,
                    steps,
                });
            }

            let file = map
                .get(Value::String("file".into()))
                .and_then(Value::as_str)
                .ok_or_else(|| ParseError::MissingField("runFlow.file OR runFlow.commands".into()))?
                .to_string();

            // v5.6 c5 — `as: <name>` outputs alias capture.
            let as_name = match map.get(Value::String("as".into())) {
                None => None,
                Some(Value::String(s)) => Some(s.clone()),
                Some(other) => {
                    return Err(ParseError::InvalidValue {
                        field: "runFlow.as".into(),
                        reason: format!("expected string output alias name, got {other:?}"),
                    });
                }
            };

            Ok(Step::RunFlowConditional {
                file,
                when_visible,
                when_not_visible,
                as_name,
            })
        }
        other => Err(ParseError::InvalidValue {
            field: "runFlow".into(),
            reason: format!("expected string or map, got {other:?}"),
        }),
    }
}

// v6.8 c1 — shared `when.visible` parser for both `runFlow` arms
// (file → RunFlowConditional, commands → RunFlowInline).
fn parse_run_flow_when_visible(
    map: &serde_norway::Mapping,
) -> Result<Option<Selector>, ParseError> {
    let Some(when) = map.get(Value::String("when".into())) else {
        return Ok(None);
    };
    let when_map = when.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "runFlow.when".into(),
        reason: "expected a mapping".into(),
    })?;
    match when_map.get(Value::String("visible".into())) {
        Some(visible) => Ok(Some(visible_to_selector(visible)?)),
        None => Ok(None),
    }
}

// v1.0.24 D2 — parse `runFlow.when.notVisible`. Same shape as
// `when.visible` but the runtime gate fires when the selector is
// NOT visible. Sibling helper to `parse_run_flow_when_visible` so
// the parser dispatches both from one `when:` block.
fn parse_run_flow_when_not_visible(
    map: &serde_norway::Mapping,
) -> Result<Option<Selector>, ParseError> {
    let Some(when) = map.get(Value::String("when".into())) else {
        return Ok(None);
    };
    let when_map = when.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "runFlow.when".into(),
        reason: "expected a mapping".into(),
    })?;
    match when_map.get(Value::String("notVisible".into())) {
        Some(not_visible) => Ok(Some(visible_to_selector(not_visible)?)),
        None => Ok(None),
    }
}

fn parse_extended_wait_until(v: &Value) -> Result<Step, ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "extendedWaitUntil".into(),
        reason: "expected a mapping".into(),
    })?;
    let timeout_ms = map
        .get(Value::String("timeout".into()))
        .and_then(Value::as_u64)
        .ok_or_else(|| ParseError::MissingField("extendedWaitUntil.timeout".into()))?;
    // v5.2 c2 — visible XOR notVisible 编译期 dispatch.
    if let Some(visible) = map.get(Value::String("visible".into())) {
        let selector = visible_to_selector(visible)?;
        Ok(Step::ExtendedWaitUntil {
            selector,
            timeout_ms,
            expect_visible: true,
        })
    } else if let Some(not_visible) = map.get(Value::String("notVisible".into())) {
        let selector = visible_to_selector(not_visible)?;
        Ok(Step::ExtendedWaitUntil {
            selector,
            timeout_ms,
            expect_visible: false,
        })
    } else {
        Err(ParseError::MissingField(
            "extendedWaitUntil.visible OR extendedWaitUntil.notVisible".into(),
        ))
    }
}

fn parse_assert_visible(v: &Value) -> Result<Step, ParseError> {
    let selector = match v {
        Value::String(s) => Selector::Text {
            text: text_to_pattern(s),
            modifiers: Modifiers::default(),
        },
        Value::Mapping(_) => visible_to_selector(v)?,
        other => {
            return Err(ParseError::InvalidValue {
                field: "assertVisible".into(),
                reason: format!("expected string or map, got {other:?}"),
            });
        }
    };
    Ok(Step::AssertVisible { selector })
}

fn parse_input_text(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(s) => Ok(Step::InputText(s.clone())),
        // yaml `inputText: 12345` — coerce non-string scalars
        Value::Number(n) => Ok(Step::InputText(n.to_string())),
        other => Err(ParseError::InvalidValue {
            field: "inputText".into(),
            reason: format!("expected scalar, got {other:?}"),
        }),
    }
}

fn parse_press_key(v: &Value) -> Result<Step, ParseError> {
    let s = v.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: "pressKey".into(),
        reason: "expected a string".into(),
    })?;
    Ok(Step::PressKey(s.to_string()))
}

fn parse_erase_text(v: &Value) -> Result<Step, ParseError> {
    let n = v.as_u64().ok_or_else(|| ParseError::InvalidValue {
        field: "eraseText".into(),
        reason: "expected an unsigned integer".into(),
    })?;
    Ok(Step::EraseText(n as u32))
}

fn parse_scroll_until_visible(v: &Value) -> Result<Step, ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "scrollUntilVisible".into(),
        reason: "expected a mapping".into(),
    })?;
    let element = map
        .get(Value::String("element".into()))
        .ok_or_else(|| ParseError::MissingField("scrollUntilVisible.element".into()))?;
    let selector = visible_to_selector(element)?;
    let direction = map
        .get(Value::String("direction".into()))
        .and_then(Value::as_str)
        .unwrap_or("down")
        .to_string();
    Ok(Step::ScrollUntilVisible {
        selector,
        direction,
    })
}

fn parse_swipe(v: &Value) -> Result<Step, ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "swipe".into(),
        reason: "expected a mapping".into(),
    })?;
    let from = parse_xy(
        map.get(Value::String("from".into()))
            .ok_or_else(|| ParseError::MissingField("swipe.from".into()))?,
        "swipe.from",
    )?;
    let to = parse_xy(
        map.get(Value::String("to".into()))
            .ok_or_else(|| ParseError::MissingField("swipe.to".into()))?,
        "swipe.to",
    )?;
    Ok(Step::Swipe { from, to })
}

fn parse_xy(v: &Value, field: &str) -> Result<(f64, f64), ParseError> {
    // accept either "X%,Y%" string or { x, y } map
    if let Some(s) = v.as_str() {
        return parse_point(s);
    }
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: field.into(),
        reason: "expected string `X%,Y%` or `{x,y}` map".into(),
    })?;
    let x = map
        .get(Value::String("x".into()))
        .and_then(Value::as_f64)
        .ok_or_else(|| ParseError::MissingField(format!("{field}.x")))?;
    let y = map
        .get(Value::String("y".into()))
        .and_then(Value::as_f64)
        .ok_or_else(|| ParseError::MissingField(format!("{field}.y")))?;
    Ok((x, y))
}

fn parse_launch_app(v: &Value) -> Result<Step, ParseError> {
    // v5.3 c3 — bare `- launchApp` (Null) inherits the flow header appId
    // (filled in by parse_flow_yaml post-pass). Matches maestro CLI semantics.
    if v.is_null() {
        return Ok(Step::LaunchApp {
            app_id: String::new(),
            clear_state: false,
            clear_keychain: false,
            permissions: Vec::new(),
            arguments: Vec::new(),
            stop_app: true,
            wait_for_interactive_ms: None,
        });
    }
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "launchApp".into(),
        reason: "expected a mapping".into(),
    })?;
    let app_id = map
        .get(Value::String("appId".into()))
        .and_then(Value::as_str)
        .map_or_else(String::new, str::to_string);
    let clear_state = map
        .get(Value::String("clearState".into()))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let clear_keychain = map
        .get(Value::String("clearKeychain".into()))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    // v5.2 c2 — stopApp default true (maestro 文档明示).
    let stop_app = map
        .get(Value::String("stopApp".into()))
        .and_then(Value::as_bool)
        .unwrap_or(true);
    // v5.2 c2 — arguments: sequence form `[a, b, c]` (default).
    // v5.5 c6 — also accept mapping form `{ key: value, ... }` (maestro CLI
    // accepts the mapping form syntactically but does NOT forward it to the
    // launched app's argv because its IDB path drops them silently; smix
    // bypasses IDB and uses simctl launch directly so mapping form can now
    // actually fire `-key value` pairs through to the app — fixes a real
    // capability gap in maestro on iOS sim).
    let arguments = match map.get(Value::String("arguments".into())) {
        None => Vec::new(),
        Some(Value::Sequence(seq)) => seq
            .iter()
            .map(|v| {
                v.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| ParseError::InvalidValue {
                        field: "launchApp.arguments[]".into(),
                        reason: format!("expected string, got {v:?}"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(Value::Mapping(m)) => {
            let mut argv = Vec::with_capacity(m.len() * 2);
            for (k, v) in m {
                let key = k.as_str().ok_or_else(|| ParseError::InvalidValue {
                    field: "launchApp.arguments".into(),
                    reason: format!("mapping key must be string, got {k:?}"),
                })?;
                // Mapping value coerces to argv string. Booleans / numbers
                // turn into their literal text (mirrors maestro's docs for
                // how a yaml scalar gets written to argv).
                let val = match v {
                    Value::String(s) => s.clone(),
                    Value::Bool(b) => {
                        if *b {
                            "YES".into()
                        } else {
                            "NO".into()
                        }
                    }
                    Value::Number(n) => n.to_string(),
                    Value::Null => String::new(),
                    other => {
                        return Err(ParseError::InvalidValue {
                            field: format!("launchApp.arguments.{key}"),
                            reason: format!(
                                "mapping value must be a scalar (string/bool/number/null), got {other:?}"
                            ),
                        });
                    }
                };
                argv.push(key.to_string());
                argv.push(val);
            }
            argv
        }
        Some(other) => {
            return Err(ParseError::InvalidValue {
                field: "launchApp.arguments".into(),
                reason: format!("expected array of strings or mapping (key→scalar), got {other:?}"),
            });
        }
    };
    // v5.2 c2 — permissions: map<string, "allow"|"deny"|"unset">, default empty.
    let permissions = match map.get(Value::String("permissions".into())) {
        None => Vec::new(),
        Some(Value::Mapping(m)) => m
            .iter()
            .map(|(k, v)| {
                let name = k
                    .as_str()
                    .ok_or_else(|| ParseError::InvalidValue {
                        field: "launchApp.permissions".into(),
                        reason: format!("permission name must be string, got {k:?}"),
                    })?
                    .to_string();
                let action = match v.as_str() {
                    Some("allow") => MaestroPermissionAction::Allow,
                    Some("deny") => MaestroPermissionAction::Deny,
                    Some("unset") => MaestroPermissionAction::Unset,
                    Some(other) => {
                        return Err(ParseError::InvalidValue {
                            field: format!("launchApp.permissions.{name}"),
                            reason: format!("expected 'allow'|'deny'|'unset', got {other:?}"),
                        });
                    }
                    None => {
                        return Err(ParseError::InvalidValue {
                            field: format!("launchApp.permissions.{name}"),
                            reason: "action must be string".into(),
                        });
                    }
                };
                Ok((name, action))
            })
            .collect::<Result<Vec<_>, _>>()?,
        Some(other) => {
            return Err(ParseError::InvalidValue {
                field: "launchApp.permissions".into(),
                reason: format!("expected map, got {other:?}"),
            });
        }
    };
    // v1.0.16 — accept `waitForInteractiveMs: <ms>` on the map form.
    // Threads through to the runner's `SessionAppLifecycleRequest`
    // when `stop_app == true` (cooperative launch pathway). Bare
    // form (`launchApp: null`) can't have opts; that path stays at
    // pre-v1.0.16 semantics.
    let wait_for_interactive_ms = map
        .get(Value::String("waitForInteractiveMs".into()))
        .and_then(Value::as_u64);
    Ok(Step::LaunchApp {
        app_id,
        clear_state,
        clear_keychain,
        permissions,
        arguments,
        stop_app,
        wait_for_interactive_ms,
    })
}

// v1.0.18 D2 — accept bare `- waitForAnimationToEnd` (400 ms default)
// or numeric `- waitForAnimationToEnd: 500` (integer = ms sleep).
// SmixQuiescenceSwizzle.m no-ops XCTest's idle-wait for performance,
// so this verb is a FIXED sleep in smix, not an XCTest quiescence
// wait. Insight round-4 clarification: the swizzle only touches
// XCTest's internal idle wait; this verb never went through it in
// the first place. maestro-compat default preserved.
fn parse_wait_for_animation_to_end(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::Null => Ok(Step::WaitForAnimationToEnd { duration_ms: 400 }),
        Value::Number(_) => {
            let ms = v.as_u64().ok_or_else(|| ParseError::InvalidValue {
                field: "waitForAnimationToEnd".into(),
                reason: format!("expected u64 milliseconds, got {v:?}"),
            })?;
            Ok(Step::WaitForAnimationToEnd { duration_ms: ms })
        }
        other => Err(ParseError::InvalidValue {
            field: "waitForAnimationToEnd".into(),
            reason: format!(
                "expected null (bare form → 400 ms default) or integer ms, got {other:?}"
            ),
        }),
    }
}

fn parse_open_link(v: &Value) -> Result<Step, ParseError> {
    let s = v.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: "openLink".into(),
        reason: "expected a string url".into(),
    })?;
    Ok(Step::OpenLink(s.to_string()))
}

// v5.2 c1 — assertNotVisible accepts the same selector shapes as
// assertVisible (short string = text shortcut, full mapping = id / text
// / point disambiguation).
fn parse_assert_not_visible(v: &Value) -> Result<Step, ParseError> {
    let selector = match v {
        Value::String(s) => Selector::Text {
            text: text_to_pattern(s),
            modifiers: Modifiers::default(),
        },
        Value::Mapping(_) => visible_to_selector(v)?,
        other => {
            return Err(ParseError::InvalidValue {
                field: "assertNotVisible".into(),
                reason: format!("expected string or map, got {other:?}"),
            });
        }
    };
    Ok(Step::AssertNotVisible { selector })
}

// v5.2 c1 — killApp accepts `killApp: "com.x"` (string short form) or
// `killApp: { appId: "com.x" }` (full form, for forward-compat with
// future scope fields). maestro currently only documents the bare
// string form but accepts either at parser level.
// v1.0.11 §D2 — accept either bare `- clearAppData` (unit) or the
// map form with `launchArgs` / `launchEnv`. Bare form is a valid
// yaml value in serde_norway sense: the parent step-list entry is
// `- clearAppData` where the value under the step-name key is
// `null`. Map form is `- clearAppData: { launchArgs: [...], ... }`.
fn parse_clear_app_data(v: &Value) -> Result<Step, ParseError> {
    let mut launch_args: Vec<String> = Vec::new();
    let mut launch_env: std::collections::BTreeMap<String, String> = Default::default();
    match v {
        Value::Null => {}
        Value::Mapping(_) => {
            let map = v.as_mapping().expect("just matched");
            if let Some(args_v) = map.get(Value::String("launchArgs".into()))
                .or_else(|| map.get(Value::String("args".into())))
            {
                let arr = args_v.as_sequence().ok_or_else(|| ParseError::InvalidValue {
                    field: "clearAppData.launchArgs".into(),
                    reason: format!("expected sequence, got {args_v:?}"),
                })?;
                for item in arr {
                    let s = item.as_str().ok_or_else(|| ParseError::InvalidValue {
                        field: "clearAppData.launchArgs[]".into(),
                        reason: format!("expected string, got {item:?}"),
                    })?;
                    launch_args.push(s.to_string());
                }
            }
            if let Some(env_v) = map.get(Value::String("launchEnv".into()))
                .or_else(|| map.get(Value::String("env".into())))
            {
                let em = env_v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
                    field: "clearAppData.launchEnv".into(),
                    reason: format!("expected mapping, got {env_v:?}"),
                })?;
                for (k, v) in em {
                    let ks = k.as_str().ok_or_else(|| ParseError::InvalidValue {
                        field: "clearAppData.launchEnv key".into(),
                        reason: format!("expected string key, got {k:?}"),
                    })?;
                    let vs = v.as_str().ok_or_else(|| ParseError::InvalidValue {
                        field: format!("clearAppData.launchEnv.{ks}"),
                        reason: format!("expected string value, got {v:?}"),
                    })?;
                    launch_env.insert(ks.to_string(), vs.to_string());
                }
            }
        }
        other => {
            return Err(ParseError::InvalidValue {
                field: "clearAppData".into(),
                reason: format!("expected null (bare) or mapping, got {other:?}"),
            });
        }
    }
    Ok(Step::ClearAppData {
        launch_args,
        launch_env,
    })
}

// v1.0.14 Cluster A — resetAppData shape:
//   - resetAppData: 'insight://dev-mutate?action=reset'   # short
//   - resetAppData:                                          # map
//       via: url-scheme          # future-proofing; only 'url-scheme' today
//       url: 'insight://dev-mutate?action=reset'
//       waitFor:
//         logLinePattern: '\[insight-dev\] reset-complete token='
//         # OR: sleepMs: 500
//       timeoutMs: 5000
fn parse_reset_app_data(v: &Value) -> Result<Step, ParseError> {
    use smix_sdk::ResetAppDataWaitFor;
    match v {
        Value::String(url) => Ok(Step::ResetAppData {
            url: url.clone(),
            wait_for: None,
            timeout_ms: 5000,
        }),
        Value::Mapping(_) => {
            let map = v.as_mapping().expect("just matched");
            let url = map
                .get(Value::String("url".into()))
                .and_then(Value::as_str)
                .ok_or_else(|| ParseError::MissingField("resetAppData.url".into()))?
                .to_string();
            let timeout_ms = map
                .get(Value::String("timeoutMs".into()))
                .and_then(Value::as_u64)
                .unwrap_or(5000);
            let wait_for = if let Some(wf_v) = map.get(Value::String("waitFor".into())) {
                let wf_map = wf_v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
                    field: "resetAppData.waitFor".into(),
                    reason: format!("expected mapping, got {wf_v:?}"),
                })?;
                if let Some(pattern) = wf_map
                    .get(Value::String("logLinePattern".into()))
                    .and_then(Value::as_str)
                {
                    Some(ResetAppDataWaitFor::LogLinePattern(pattern.to_string()))
                } else if let Some(sleep_ms) = wf_map
                    .get(Value::String("sleepMs".into()))
                    .and_then(Value::as_u64)
                {
                    Some(ResetAppDataWaitFor::Sleep(sleep_ms))
                } else {
                    return Err(ParseError::InvalidValue {
                        field: "resetAppData.waitFor".into(),
                        reason: "expected either `logLinePattern` or `sleepMs`".into(),
                    });
                }
            } else {
                None
            };
            Ok(Step::ResetAppData {
                url,
                wait_for,
                timeout_ms,
            })
        }
        other => Err(ParseError::InvalidValue {
            field: "resetAppData".into(),
            reason: format!("expected string (short-form URL) or map, got {other:?}"),
        }),
    }
}

fn parse_kill_app(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(s) => Ok(Step::KillApp { app_id: s.clone() }),
        Value::Mapping(_) => {
            let map = v.as_mapping().expect("just matched");
            let app_id = map
                .get(Value::String("appId".into()))
                .and_then(Value::as_str)
                .ok_or_else(|| ParseError::MissingField("killApp.appId".into()))?
                .to_string();
            Ok(Step::KillApp { app_id })
        }
        other => Err(ParseError::InvalidValue {
            field: "killApp".into(),
            reason: format!("expected string or map, got {other:?}"),
        }),
    }
}

// v5.2 c1 — clearState as an independent command (vs the launchApp child
// field that already exists). Accepts `clearState: { appId: "com.x" }`.
// maestro variant also accepts a bare `- clearState` when used inside
// `launchApp` (handled by parse_launch_app); the top-level form requires
// the appId.
fn parse_clear_state(v: &Value) -> Result<Step, ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "clearState".into(),
        reason: "expected a mapping with `appId`".into(),
    })?;
    let app_id = map
        .get(Value::String("appId".into()))
        .and_then(Value::as_str)
        .ok_or_else(|| ParseError::MissingField("clearState.appId".into()))?
        .to_string();
    Ok(Step::ClearState { app_id })
}

// v5.2 c4 — setClipboard accepts string literal含 `${expr}` 模板; runtime
// `expand_template` 调 expr engine 替换. (v5.2 c3 deferred-to-c4 guard 已 sweep)
fn parse_set_clipboard(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(s) => Ok(Step::SetClipboard(s.clone())),
        other => Err(ParseError::InvalidValue {
            field: "setClipboard".into(),
            reason: format!("expected string, got {other:?}"),
        }),
    }
}

// v5.2 c4 — pasteText 双形态: 裸 `- pasteText` (None) / `pasteText: "literal"`
// (Some). literal 含 `${expr}` 模板; runtime expand_template 处理. (v5.2 c3
// deferred-to-c4 guard 已 sweep)
fn parse_paste_text(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::Null => Ok(Step::PasteText { text: None }),
        Value::String(s) => Ok(Step::PasteText {
            text: Some(s.clone()),
        }),
        other => Err(ParseError::InvalidValue {
            field: "pasteText".into(),
            reason: format!("expected null or string, got {other:?}"),
        }),
    }
}

// v5.2 c5 — `setLocation: { latitude, longitude }`. Both required f64.
fn parse_set_location(v: &Value) -> Result<Step, ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "setLocation".into(),
        reason: "expected a mapping with latitude / longitude".into(),
    })?;
    let latitude = map
        .get(Value::String("latitude".into()))
        .and_then(Value::as_f64)
        .ok_or_else(|| ParseError::MissingField("setLocation.latitude".into()))?;
    let longitude = map
        .get(Value::String("longitude".into()))
        .and_then(Value::as_f64)
        .ok_or_else(|| ParseError::MissingField("setLocation.longitude".into()))?;
    Ok(Step::SetLocation {
        latitude,
        longitude,
    })
}

// v5.2 c5 — `travel: { points: [{ latitude, longitude }, ...], speed_mps?: f64 }`.
// ≥2 waypoints required (simctl location start 语义).
fn parse_travel(v: &Value) -> Result<Step, ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "travel".into(),
        reason: "expected a mapping with points (and optional speed_mps)".into(),
    })?;
    let points_val = map
        .get(Value::String("points".into()))
        .ok_or_else(|| ParseError::MissingField("travel.points".into()))?;
    let seq = match points_val {
        Value::Sequence(s) => s,
        other => {
            return Err(ParseError::InvalidValue {
                field: "travel.points".into(),
                reason: format!("expected sequence, got {other:?}"),
            });
        }
    };
    let mut points: Vec<(f64, f64)> = Vec::with_capacity(seq.len());
    for (i, pt) in seq.iter().enumerate() {
        let pm = pt.as_mapping().ok_or_else(|| ParseError::InvalidValue {
            field: format!("travel.points[{i}]"),
            reason: format!("expected mapping {{latitude, longitude}}, got {pt:?}"),
        })?;
        let lat = pm
            .get(Value::String("latitude".into()))
            .and_then(Value::as_f64)
            .ok_or_else(|| ParseError::MissingField(format!("travel.points[{i}].latitude")))?;
        let lng = pm
            .get(Value::String("longitude".into()))
            .and_then(Value::as_f64)
            .ok_or_else(|| ParseError::MissingField(format!("travel.points[{i}].longitude")))?;
        points.push((lat, lng));
    }
    if points.len() < 2 {
        return Err(ParseError::InvalidValue {
            field: "travel.points".into(),
            reason: format!(
                "requires at least 2 waypoints, got {} (simctl location start 语义)",
                points.len()
            ),
        });
    }
    let speed_mps = map
        .get(Value::String("speed_mps".into()))
        .and_then(Value::as_f64);
    Ok(Step::Travel { points, speed_mps })
}

// v5.2 c5 — top-level `setPermissions: { camera: allow, location: deny, ... }`.
// 跟 c2 launchApp.permissions 子参同款 inner parse, app_id 留 None 由
// runtime 解析 last_bundle.
fn parse_set_permissions(v: &Value) -> Result<Step, ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "setPermissions".into(),
        reason: "expected a mapping (permission name → allow|deny|unset)".into(),
    })?;
    let mut permissions: Vec<(String, MaestroPermissionAction)> = Vec::with_capacity(map.len());
    for (k, val) in map.iter() {
        let name = k
            .as_str()
            .ok_or_else(|| ParseError::InvalidValue {
                field: "setPermissions".into(),
                reason: format!("permission name must be string, got {k:?}"),
            })?
            .to_string();
        let action = match val.as_str() {
            Some("allow") => MaestroPermissionAction::Allow,
            Some("deny") => MaestroPermissionAction::Deny,
            Some("unset") => MaestroPermissionAction::Unset,
            Some(other) => {
                return Err(ParseError::InvalidValue {
                    field: format!("setPermissions.{name}"),
                    reason: format!("expected 'allow'|'deny'|'unset', got {other:?}"),
                });
            }
            None => {
                return Err(ParseError::InvalidValue {
                    field: format!("setPermissions.{name}"),
                    reason: "action must be string".into(),
                });
            }
        };
        permissions.push((name, action));
    }
    if permissions.is_empty() {
        return Err(ParseError::InvalidValue {
            field: "setPermissions".into(),
            reason: "expected at least one permission entry".into(),
        });
    }
    Ok(Step::SetPermissions {
        app_id: None,
        permissions,
    })
}

// v5.2 c6 — `assertScreenshot: "path"` scalar.
// v5.5 c5 lifted mapping form `{ path, threshold?, mask? }` (surface-only:
// threshold passes to dhash max_hamming, mask carried but runtime
// warn-and-ignored — algorithm-level region exclusion is R2-tier, deferred
// to v6+ when SSIM/pHash backbone replaces dhash; cold plan §scope).
fn parse_assert_screenshot(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(s) => Ok(Step::AssertScreenshot {
            path: s.clone(),
            max_hamming: None,
            mask: Vec::new(),
        }),
        Value::Mapping(map) => {
            let path = map
                .get(Value::String("path".into()))
                .and_then(Value::as_str)
                .ok_or_else(|| ParseError::MissingField("assertScreenshot.path".into()))?
                .to_string();
            let max_hamming = match map.get(Value::String("threshold".into())) {
                None => None,
                Some(t) => match t.as_u64() {
                    Some(n) if n <= u32::MAX as u64 => Some(n as u32),
                    _ => {
                        return Err(ParseError::InvalidValue {
                            field: "assertScreenshot.threshold".into(),
                            reason: format!(
                                "expected unsigned integer ≤ u32::MAX (dhash hamming distance), got {t:?}"
                            ),
                        });
                    }
                },
            };
            let mask = match map.get(Value::String("mask".into())) {
                None => Vec::new(),
                Some(Value::Sequence(seq)) => {
                    let mut regions = Vec::with_capacity(seq.len());
                    for (i, item) in seq.iter().enumerate() {
                        let r = item.as_mapping().ok_or_else(|| ParseError::InvalidValue {
                            field: format!("assertScreenshot.mask[{i}]"),
                            reason: format!(
                                "expected mapping {{x, y, width, height}}, got {item:?}"
                            ),
                        })?;
                        let f = |k: &str| -> Result<f64, ParseError> {
                            r.get(Value::String(k.into()))
                                .and_then(Value::as_f64)
                                .ok_or_else(|| ParseError::InvalidValue {
                                    field: format!("assertScreenshot.mask[{i}].{k}"),
                                    reason: "expected float (0..1 fraction)".into(),
                                })
                        };
                        regions.push(crate::MaskRegion {
                            x: f("x")?,
                            y: f("y")?,
                            width: f("width")?,
                            height: f("height")?,
                        });
                    }
                    regions
                }
                Some(other) => {
                    return Err(ParseError::InvalidValue {
                        field: "assertScreenshot.mask".into(),
                        reason: format!("expected sequence of bbox mappings, got {other:?}"),
                    });
                }
            };
            Ok(Step::AssertScreenshot {
                path,
                max_hamming,
                mask,
            })
        }
        other => Err(ParseError::InvalidValue {
            field: "assertScreenshot".into(),
            reason: format!("expected string or mapping form, got {other:?}"),
        }),
    }
}

// v5.2 c5 — `startRecording: <path>` scalar string only.
fn parse_start_recording(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(s) => Ok(Step::StartRecording { path: s.clone() }),
        other => Err(ParseError::InvalidValue {
            field: "startRecording".into(),
            reason: format!("expected output file path string, got {other:?}"),
        }),
    }
}

// v5.2 c5 — `setOrientation: <portrait|portraitUpsideDown|landscapeLeft|
// landscapeRight|landscape>`. `landscape` alias → LandscapeLeft (maestro 同源).
fn parse_set_orientation(v: &Value) -> Result<Step, ParseError> {
    let s = v.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: "setOrientation".into(),
        reason: "expected a string literal".into(),
    })?;
    // v5.3 c3 — accept both camelCase (maestro doc) and UPPER_SNAKE_CASE
    // (community style) aliases. maestro CLI accepts both.
    let orientation = match s {
        "portrait" | "PORTRAIT" => smix_sdk::MaestroOrientation::Portrait,
        "portraitUpsideDown" | "PORTRAIT_UPSIDE_DOWN" => {
            smix_sdk::MaestroOrientation::PortraitUpsideDown
        }
        "landscapeLeft" | "LANDSCAPE_LEFT" => smix_sdk::MaestroOrientation::LandscapeLeft,
        "landscapeRight" | "LANDSCAPE_RIGHT" => smix_sdk::MaestroOrientation::LandscapeRight,
        "landscape" | "LANDSCAPE" => smix_sdk::MaestroOrientation::LandscapeLeft,
        other => {
            return Err(ParseError::InvalidValue {
                field: "setOrientation".into(),
                reason: format!(
                    "unknown orientation '{other}' — expected portrait | portraitUpsideDown | landscapeLeft | landscapeRight (alias: landscape)"
                ),
            });
        }
    };
    Ok(Step::SetOrientation { orientation })
}

// v5.2 c5 — `addMedia: <path>` (scalar) or `addMedia: [paths]` (array).
fn parse_add_media(v: &Value) -> Result<Step, ParseError> {
    let paths = match v {
        Value::String(s) => vec![s.clone()],
        Value::Sequence(seq) => seq
            .iter()
            .enumerate()
            .map(|(i, item)| {
                item.as_str()
                    .map(str::to_string)
                    .ok_or_else(|| ParseError::InvalidValue {
                        field: format!("addMedia[{i}]"),
                        reason: format!("expected string path, got {item:?}"),
                    })
            })
            .collect::<Result<Vec<_>, _>>()?,
        other => {
            return Err(ParseError::InvalidValue {
                field: "addMedia".into(),
                reason: format!("expected string or sequence of strings, got {other:?}"),
            });
        }
    };
    if paths.is_empty() {
        return Err(ParseError::InvalidValue {
            field: "addMedia".into(),
            reason: "expected at least one path".into(),
        });
    }
    Ok(Step::AddMedia { paths })
}

// v5.2 c4 — `repeat: { while|times, commands }`.
// v5.5 c5 — selector-style while (`while: { visible: <sel> }`) is now
// accepted as `RepeatMode::WhileVisible` (was parser-rejected pre-v5.5).
fn parse_repeat(v: &Value) -> Result<Step, ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "repeat".into(),
        reason: "expected a mapping".into(),
    })?;
    let commands_val = map
        .get(Value::String("commands".into()))
        .ok_or_else(|| ParseError::MissingField("repeat.commands".into()))?;
    let commands = parse_step_sequence(commands_val, "repeat.commands")?;
    let has_times = map.get(Value::String("times".into())).is_some();
    let has_while = map.get(Value::String("while".into())).is_some();
    let mode = match (has_times, has_while) {
        (true, true) => {
            return Err(ParseError::InvalidValue {
                field: "repeat".into(),
                reason: "expected exactly one of `times` or `while`, got both".into(),
            });
        }
        (false, false) => {
            return Err(ParseError::InvalidValue {
                field: "repeat".into(),
                reason: "expected exactly one of `times` or `while`".into(),
            });
        }
        (true, false) => {
            let n = map
                .get(Value::String("times".into()))
                .and_then(Value::as_u64)
                .ok_or_else(|| ParseError::InvalidValue {
                    field: "repeat.times".into(),
                    reason: "expected unsigned integer".into(),
                })?;
            if n > u32::MAX as u64 {
                return Err(ParseError::InvalidValue {
                    field: "repeat.times".into(),
                    reason: format!("times {n} exceeds u32::MAX"),
                });
            }
            RepeatMode::Times(n as u32)
        }
        (false, true) => {
            let w = map.get(Value::String("while".into())).expect("checked");
            match w {
                Value::String(s) => RepeatMode::While {
                    condition_expr: s.clone(),
                },
                Value::Mapping(while_map) => {
                    let has_visible = while_map.get(Value::String("visible".into())).is_some();
                    let has_not_visible =
                        while_map.get(Value::String("notVisible".into())).is_some();
                    match (has_visible, has_not_visible) {
                        (true, true) => return Err(ParseError::InvalidValue {
                            field: "repeat.while".into(),
                            reason: "mapping form must contain exactly one of `visible` or `notVisible`, got both".into(),
                        }),
                        (false, false) => return Err(ParseError::InvalidValue {
                            field: "repeat.while".into(),
                            reason: "mapping form must contain `visible: <selector>` or `notVisible: <selector>`".into(),
                        }),
                        (true, false) => {
                            let visible = while_map.get(Value::String("visible".into())).expect("checked");
                            let selector = visible_to_selector(visible).map_err(|e| match e {
                                ParseError::InvalidValue { field, reason } => ParseError::InvalidValue {
                                    field: format!("repeat.while.visible.{field}"),
                                    reason,
                                },
                                other => other,
                            })?;
                            RepeatMode::WhileVisible { selector }
                        }
                        (false, true) => {
                            let not_visible = while_map.get(Value::String("notVisible".into())).expect("checked");
                            let selector = visible_to_selector(not_visible).map_err(|e| match e {
                                ParseError::InvalidValue { field, reason } => ParseError::InvalidValue {
                                    field: format!("repeat.while.notVisible.{field}"),
                                    reason,
                                },
                                other => other,
                            })?;
                            RepeatMode::WhileNotVisible { selector }
                        }
                    }
                }
                other => {
                    return Err(ParseError::InvalidValue {
                        field: "repeat.while".into(),
                        reason: format!(
                            "expected string expression or `{{ visible: <selector> }}` mapping, got {other:?}"
                        ),
                    });
                }
            }
        }
    };
    Ok(Step::Repeat { mode, commands })
}

// v5.2 c4 — `retry: { maxRetries, commands }`.
fn parse_retry(v: &Value) -> Result<Step, ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "retry".into(),
        reason: "expected a mapping".into(),
    })?;
    let max_retries = map
        .get(Value::String("maxRetries".into()))
        .and_then(Value::as_u64)
        .ok_or_else(|| ParseError::MissingField("retry.maxRetries".into()))?;
    if max_retries > u32::MAX as u64 {
        return Err(ParseError::InvalidValue {
            field: "retry.maxRetries".into(),
            reason: format!("maxRetries {max_retries} exceeds u32::MAX"),
        });
    }
    let commands_val = map
        .get(Value::String("commands".into()))
        .ok_or_else(|| ParseError::MissingField("retry.commands".into()))?;
    let commands = parse_step_sequence(commands_val, "retry.commands")?;
    Ok(Step::Retry {
        max_retries: max_retries as u32,
        commands,
    })
}

// v5.2 c4 — `runScript: <inline literal or path>`. Parser 接受, runtime
// explicit DriverError "complete JS runtime not supported in v5.2".
fn parse_run_script(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(s) => Ok(Step::RunScript { source: s.clone() }),
        other => Err(ParseError::InvalidValue {
            field: "runScript".into(),
            reason: format!("expected string (inline source or file path), got {other:?}"),
        }),
    }
}

// v5.21 c1b — `webview_eval: "<js>"` (short) or
// `webview_eval: { js: "...", assert_eq: <json value> }` (full).
// Evals JS against fixture-side WKWebView bridge (Option A, a11y-i18n
// master plan §1).
fn parse_webview_eval(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(s) => {
            if s.is_empty() {
                return Err(ParseError::InvalidValue {
                    field: "webview_eval".into(),
                    reason: "js must be non-empty".into(),
                });
            }
            Ok(Step::WebViewEval {
                js: s.clone(),
                assert_eq: None,
            })
        }
        Value::Mapping(m) => {
            let js = m
                .get(Value::String("js".into()))
                .and_then(Value::as_str)
                .ok_or_else(|| ParseError::InvalidValue {
                    field: "webview_eval.js".into(),
                    reason: "js field required (string)".into(),
                })?;
            if js.is_empty() {
                return Err(ParseError::InvalidValue {
                    field: "webview_eval.js".into(),
                    reason: "js must be non-empty".into(),
                });
            }
            // assert_eq optional — accept any yaml value, convert to JSON
            // via serde_json::Value round-trip (yaml scalar / map / seq).
            let assert_eq = if let Some(raw) = m.get(Value::String("assert_eq".into())) {
                let json_str =
                    serde_norway::to_string(raw).map_err(|e| ParseError::InvalidValue {
                        field: "webview_eval.assert_eq".into(),
                        reason: format!("yaml→string conversion failed: {e}"),
                    })?;
                let json_val: serde_json::Value =
                    serde_norway::from_str(&json_str).map_err(|e| ParseError::InvalidValue {
                        field: "webview_eval.assert_eq".into(),
                        reason: format!("yaml→json parse failed: {e}"),
                    })?;
                Some(json_val)
            } else {
                None
            };
            Ok(Step::WebViewEval {
                js: js.to_string(),
                assert_eq,
            })
        }
        other => Err(ParseError::InvalidValue {
            field: "webview_eval".into(),
            reason: format!("expected string or map, got {other:?}"),
        }),
    }
}

// v5.2 c4 — `evalScript: <inline expression>`. 同 runScript graceful unsupported.
fn parse_eval_script(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(s) => Ok(Step::EvalScript { source: s.clone() }),
        other => Err(ParseError::InvalidValue {
            field: "evalScript".into(),
            reason: format!("expected string expression, got {other:?}"),
        }),
    }
}

// v5.2 c4 — recursive helper: parse a yaml sequence of step items
// (used by repeat.commands / retry.commands).
fn parse_step_sequence(v: &Value, field: &str) -> Result<Vec<Step>, ParseError> {
    let seq = match v {
        Value::Sequence(s) => s,
        other => {
            return Err(ParseError::InvalidValue {
                field: field.into(),
                reason: format!("expected sequence of steps, got {other:?}"),
            });
        }
    };
    let mut out = Vec::with_capacity(seq.len());
    for item in seq {
        out.push(parse_step_item(item)?);
    }
    Ok(out)
}

// v5.2 c4 — assertTrue accepts a string literal (raw expression source).
// runtime walks expand_template + expr engine + truthy check.
fn parse_assert_true(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(s) => Ok(Step::AssertTrue { expr: s.clone() }),
        Value::Bool(b) => Ok(Step::AssertTrue {
            expr: b.to_string(),
        }),
        other => Err(ParseError::InvalidValue {
            field: "assertTrue".into(),
            reason: format!("expected string or bool, got {other:?}"),
        }),
    }
}

// v5.2 c3 — maestro `longPressOn` 默认 duration (ms).
// cli-2.2.0 文档明示 0.5s + XCUIElement.press(forDuration:) 标准 0.5s.
const LONG_PRESS_DEFAULT_MS: u64 = 500;

// v5.2 c3 — doubleTapOn 单 arm: scalar string → text selector;
// mapping → 全 selector (id/text/label/...). 不接 maestro 暂未文档化的子参.
fn parse_double_tap_on(v: &Value) -> Result<Step, ParseError> {
    let selector = match v {
        Value::String(s) => Selector::Text {
            text: text_to_pattern(s),
            modifiers: Modifiers::default(),
        },
        Value::Mapping(_) => visible_to_selector(v)?,
        other => {
            return Err(ParseError::InvalidValue {
                field: "doubleTapOn".into(),
                reason: format!("expected string or map, got {other:?}"),
            });
        }
    };
    Ok(Step::DoubleTapOn { selector })
}

// v5.2 c3 — longPressOn 双形态: scalar (default duration) 或 mapping
// (selector 字段 + optional `duration: <ms>`). maestro 文档字段名 `duration`
// 单位 ms.
fn parse_long_press_on(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(s) => Ok(Step::LongPressOn {
            selector: Selector::Text {
                text: text_to_pattern(s),
                modifiers: Modifiers::default(),
            },
            duration_ms: LONG_PRESS_DEFAULT_MS,
        }),
        Value::Mapping(m) => {
            let selector = visible_to_selector(v)?;
            let duration_ms = m
                .get(Value::String("duration".into()))
                .and_then(Value::as_u64)
                .unwrap_or(LONG_PRESS_DEFAULT_MS);
            Ok(Step::LongPressOn {
                selector,
                duration_ms,
            })
        }
        other => Err(ParseError::InvalidValue {
            field: "longPressOn".into(),
            reason: format!("expected string or map, got {other:?}"),
        }),
    }
}

// v5.2 c3 — copyTextFrom 同 assertVisible / tapOn 的 selector 抽取模式.
fn parse_copy_text_from(v: &Value) -> Result<Step, ParseError> {
    let selector = match v {
        Value::String(s) => Selector::Text {
            text: text_to_pattern(s),
            modifiers: Modifiers::default(),
        },
        Value::Mapping(_) => visible_to_selector(v)?,
        other => {
            return Err(ParseError::InvalidValue {
                field: "copyTextFrom".into(),
                reason: format!("expected string or map, got {other:?}"),
            });
        }
    };
    Ok(Step::CopyTextFrom { selector })
}

// v5.2 c1 / v1.0 Phase C2 — takeScreenshot accepts three shapes:
//   `takeScreenshot: "name"`          — string path (byte-compat v0.3.x)
//   `- takeScreenshot`                 — bare (None ⇒ discard bytes)
//   `takeScreenshot: { name, annotate }` — long form with annotations
fn parse_take_screenshot(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::Null => Ok(Step::TakeScreenshot {
            path: None,
            annotations: Vec::new(),
        }),
        Value::String(s) => Ok(Step::TakeScreenshot {
            path: Some(s.clone()),
            annotations: Vec::new(),
        }),
        Value::Mapping(m) => {
            // Long form: { name?, annotate: [...] }
            let path = m
                .get(Value::String("name".into()))
                .or_else(|| m.get(Value::String("path".into())))
                .and_then(|x| x.as_str())
                .map(String::from);
            let annotations = match m.get(Value::String("annotate".into())) {
                Some(Value::Sequence(seq)) => {
                    let mut out = Vec::with_capacity(seq.len());
                    for (idx, item) in seq.iter().enumerate() {
                        // Each item is a single-key mapping like
                        // `- circle: { at: ..., color: ..., radius: ... }`.
                        let field_prefix = format!("takeScreenshot.annotate[{idx}]");
                        let item_map =
                            item.as_mapping().ok_or_else(|| ParseError::InvalidValue {
                                field: field_prefix.clone(),
                                reason: format!("expected single-key mapping, got {item:?}"),
                            })?;
                        if item_map.len() != 1 {
                            return Err(ParseError::InvalidValue {
                                field: field_prefix.clone(),
                                reason: format!(
                                    "expected exactly one key (circle/arrow/text/box/line), got {} keys",
                                    item_map.len()
                                ),
                            });
                        }
                        let (kind_key, body) = item_map.iter().next().unwrap();
                        let kind = kind_key.as_str().ok_or_else(|| ParseError::InvalidValue {
                            field: field_prefix.clone(),
                            reason: "annotation kind must be a string".into(),
                        })?;
                        let spec = crate::parse_annotation_from_kind(kind, body).map_err(|e| {
                            ParseError::InvalidValue {
                                field: format!("{field_prefix}.{kind}"),
                                reason: e,
                            }
                        })?;
                        out.push(spec);
                    }
                    out
                }
                Some(other) => {
                    return Err(ParseError::InvalidValue {
                        field: "takeScreenshot.annotate".into(),
                        reason: format!("expected list, got {other:?}"),
                    });
                }
                None => Vec::new(),
            };
            Ok(Step::TakeScreenshot { path, annotations })
        }
        other => Err(ParseError::InvalidValue {
            field: "takeScreenshot".into(),
            reason: format!("expected string, mapping, or null, got {other:?}"),
        }),
    }
}

// -------------------- top-level entry ------------------------------------

/// v1.0 Phase A3 — normalize verb name to maestro-canonical form
/// before dispatching. Consumer yaml may use either the maestro form
/// (`tapOn`, `assertVisible`) or the smix-canonical form (`tap`,
/// `expect`); this fn maps smix names back to maestro names so
/// `dispatch_step`'s existing match arms don't need modification.
///
/// Reviewer invariant: every new dispatch arm below must correspond to
/// a `smix_verbs::VERB_TABLE` entry (maestro name matches the arm).
/// Grep this fn for hardcoded strings; any hit that's not in
/// VERB_TABLE is a regression.
fn normalize_verb_name(key: &str) -> &str {
    // Fast path: maestro-canonical → identity
    if smix_verbs::find_by_maestro(key).is_some() {
        return key;
    }
    // smix-canonical → maestro-canonical
    if let Some(entry) = smix_verbs::find_by_smix(key) {
        // Skip identity entries (smix_name == maestro_name) — already
        // covered by the fast path above; falling here means the key
        // was actually smix-only. Return the maestro form.
        if entry.smix_name != entry.maestro_name {
            return entry.maestro_name;
        }
    }
    key // unknown; leave verbatim → dispatch_step will emit UnsupportedCommand
}

fn dispatch_step(key: &str, value: &Value) -> Result<Step, ParseError> {
    // v1.0 Phase A3 — polymorphic-dispatch keys must route BEFORE
    // verb-name normalization. `expect` maps to `assertVisible` in
    // smix_verbs but the parser routes it based on subkeys
    // (signal / signals / logClean / else assertVisible fallback);
    // if we normalize first we lose the routing.
    if key == "expect" {
        return parse_expect(value);
    }
    let key = normalize_verb_name(key);
    match key {
        "tapOn" => parse_tap_on(value),
        "waitForAnimationToEnd" => parse_wait_for_animation_to_end(value),
        "extendedWaitUntil" => parse_extended_wait_until(value),
        "assertVisible" => parse_assert_visible(value),
        "inputText" => parse_input_text(value),
        "pressKey" => parse_press_key(value),
        "runFlow" => parse_run_flow(value),
        "scrollUntilVisible" => parse_scroll_until_visible(value),
        "eraseText" => parse_erase_text(value),
        "swipe" => parse_swipe(value),
        "launchApp" => parse_launch_app(value),
        "openLink" => parse_open_link(value),
        "stopApp" => Ok(Step::StopApp),
        // v1.0.8 §D2 + v1.0.11 §D2 — session-scoped in-place data clear.
        // Accepts either:
        //   - clearAppData                         (bare — legacy)
        //   - clearAppData:
        //       launchArgs: ["-EXInternalMetroPort", "8081"]
        //       launchEnv:
        //         EX_DEV_CLIENT_METRO_URL: "http://localhost:8081"
        //
        // v1.0.11 launchArgs/launchEnv are forwarded to the cooperative
        // runner-side launch step inside clearAppData. Unblocks Expo
        // SDK 57 dev-launcher server picker (which stopped auto-
        // navigating on URL scheme, per insight's v1.0.10 followup).
        "clearAppData" => parse_clear_app_data(value),
        // v1.0.14 Cluster A — URL-scheme JS-wipe. Bare short-form
        // (`resetAppData: 'url'`) OR map-form (with waitFor + timeout).
        "resetAppData" => parse_reset_app_data(value),
        // v5.2 c1 — 7 ⊘ adapter-only-gap wires.
        "scroll" => Ok(Step::Scroll),
        "hideKeyboard" => Ok(Step::HideKeyboard),
        "assertNotVisible" => parse_assert_not_visible(value),
        "killApp" => parse_kill_app(value),
        "clearState" => parse_clear_state(value),
        "clearKeychain" => Ok(Step::ClearKeychain),
        "takeScreenshot" => parse_take_screenshot(value),
        // v5.2 c3 — clipboard surface + interaction (doubleTap / longPress).
        "setClipboard" => parse_set_clipboard(value),
        "pasteText" => parse_paste_text(value),
        "copyTextFrom" => parse_copy_text_from(value),
        "doubleTapOn" => parse_double_tap_on(value),
        "longPressOn" => parse_long_press_on(value),
        // v5.2 c4 — Flow gap.
        "assertTrue" => parse_assert_true(value),
        "repeat" => parse_repeat(value),
        "retry" => parse_retry(value),
        "runScript" => parse_run_script(value),
        "evalScript" => parse_eval_script(value),
        // v5.21 c1b — webview JS eval via fixture-side debug bridge.
        "webview_eval" | "webviewEval" => parse_webview_eval(value),
        // v5.2 c5 — Device + Media gap.
        "setLocation" => parse_set_location(value),
        "travel" => parse_travel(value),
        "setPermissions" => parse_set_permissions(value),
        "addMedia" => parse_add_media(value),
        "setOrientation" => parse_set_orientation(value),
        "startRecording" => parse_start_recording(value),
        "stopRecording" => Ok(Step::StopRecording),
        // v5.2 c6 — visual regression.
        "assertScreenshot" => parse_assert_screenshot(value),
        // v0.3.0 Phase A — expect.signal / expect.signals / expect.logClean.
        // The `expect` key here means the maestro `expect:` verb (currently
        // aliased to assertVisible in maestro). We route to the signal
        // parser when the value has `signal:` / `signals:` / `logClean:`
        // subkeys; otherwise fall through to assertVisible for backward
        // compat.
        "expect" => parse_expect(value),
        "expectLogClean" => Ok(Step::ExpectLogClean),
        // Fixture chip verb.
        "fixture" => parse_fixture(value),
        other => Err(ParseError::UnsupportedCommand(other.to_string())),
    }
}

/// `expect:` verb dispatcher. Recognizes:
///
/// - `expect: { signal: {...} }` → [`Step::ExpectSignal`]
/// - `expect: { signals: [...], order: strict, ... }` → [`Step::ExpectSignals`]
/// - `expect: { logClean: true }` → [`Step::ExpectLogClean`]
/// - `expect: { visible: <selector>, timeoutMs?: N }` → [`Step::ExtendedWaitUntil`]
///   (or [`Step::AssertVisible`] when `timeoutMs` is absent) — this is the
///   canonical form emitted by `smix migrate` for `extendedWaitUntil` and
///   the `expect: { visible: ... }` shorthand
/// - `expect: { notVisible: <selector>, timeoutMs?: N }` → [`Step::ExtendedWaitUntil`]
///   with `expect_visible: false` (or [`Step::AssertNotVisible`] when
///   `timeoutMs` is absent)
///
/// Anything else falls through to `assertVisible` for maestro's
/// `expect: <selector>` alias (bare string or top-level `{text|id}`).
fn parse_expect(value: &Value) -> Result<Step, ParseError> {
    if let Value::Mapping(m) = value {
        if let Some(signal) = m.get(Value::String("signal".into())) {
            return parse_expect_signal(signal, m);
        }
        if let Some(signals) = m.get(Value::String("signals".into())) {
            return parse_expect_signals(signals, m);
        }
        if let Some(Value::Bool(true)) = m.get(Value::String("logClean".into())) {
            return Ok(Step::ExpectLogClean);
        }
        let timeout_ms = m
            .get(Value::String("timeoutMs".into()))
            .and_then(Value::as_u64);
        if let Some(visible) = m.get(Value::String("visible".into())) {
            let selector = visible_to_selector(visible)?;
            return Ok(match timeout_ms {
                Some(timeout_ms) => Step::ExtendedWaitUntil {
                    selector,
                    timeout_ms,
                    expect_visible: true,
                },
                None => Step::AssertVisible { selector },
            });
        }
        if let Some(not_visible) = m.get(Value::String("notVisible".into())) {
            let selector = visible_to_selector(not_visible)?;
            return Ok(match timeout_ms {
                Some(timeout_ms) => Step::ExtendedWaitUntil {
                    selector,
                    timeout_ms,
                    expect_visible: false,
                },
                None => Step::AssertNotVisible { selector },
            });
        }
    }
    parse_assert_visible(value)
}

fn parse_expect_signal(signal: &Value, outer: &serde_norway::Mapping) -> Result<Step, ParseError> {
    // signal can be a bare string (short shorthand) or a mapping
    // {regex, level?, timeoutMs?, window?}.
    let (regex, level, sig_timeout, sig_window) = match signal {
        Value::String(s) => (s.clone(), None, None, None),
        Value::Mapping(mm) => {
            let regex = mm
                .get(Value::String("regex".into()))
                .and_then(|v| v.as_str().map(String::from))
                .ok_or_else(|| ParseError::MissingField("expect.signal.regex".into()))?;
            let level = mm
                .get(Value::String("level".into()))
                .and_then(|v| v.as_str().map(String::from));
            let timeout = mm
                .get(Value::String("timeoutMs".into()))
                .and_then(|v| v.as_u64());
            let window = mm
                .get(Value::String("window".into()))
                .and_then(parse_signal_window);
            (regex, level, timeout, window)
        }
        _ => {
            return Err(ParseError::InvalidValue {
                field: "expect.signal".into(),
                reason: "expected string or mapping".into(),
            });
        }
    };
    // Outer-level `timeoutMs` overrides inner (matches maestro style
    // for `extendedWaitUntil`).
    let outer_timeout = outer
        .get(Value::String("timeoutMs".into()))
        .and_then(|v| v.as_u64());
    let timeout_ms = outer_timeout.or(sig_timeout).unwrap_or(8000);
    let window = outer
        .get(Value::String("window".into()))
        .and_then(parse_signal_window)
        .or(sig_window)
        .unwrap_or_default();
    Ok(Step::ExpectSignal {
        regex,
        level,
        timeout_ms,
        window,
    })
}

fn parse_expect_signals(
    signals: &Value,
    outer: &serde_norway::Mapping,
) -> Result<Step, ParseError> {
    let list = signals
        .as_sequence()
        .ok_or_else(|| ParseError::InvalidValue {
            field: "expect.signals".into(),
            reason: "expected a list".into(),
        })?;
    let mut matchers = Vec::with_capacity(list.len());
    for (i, item) in list.iter().enumerate() {
        let m = match item {
            Value::String(s) => crate::SignalMatch {
                regex: s.clone(),
                level: None,
            },
            Value::Mapping(mm) => {
                let regex = mm
                    .get(Value::String("regex".into()))
                    .and_then(|v| v.as_str().map(String::from))
                    .ok_or_else(|| {
                        ParseError::MissingField(format!("expect.signals[{i}].regex"))
                    })?;
                let level = mm
                    .get(Value::String("level".into()))
                    .and_then(|v| v.as_str().map(String::from));
                crate::SignalMatch { regex, level }
            }
            _ => {
                return Err(ParseError::InvalidValue {
                    field: format!("expect.signals[{i}]"),
                    reason: "expected string or mapping".into(),
                });
            }
        };
        matchers.push(m);
    }
    let order = outer
        .get(Value::String("order".into()))
        .and_then(|v| v.as_str())
        .map(|s| match s.to_lowercase().as_str() {
            "strict" => crate::SignalOrderKind::Strict,
            _ => crate::SignalOrderKind::Any,
        })
        .unwrap_or_default();
    let timeout_ms = outer
        .get(Value::String("timeoutMs".into()))
        .and_then(|v| v.as_u64())
        .unwrap_or(30_000);
    let window = outer
        .get(Value::String("window".into()))
        .and_then(parse_signal_window)
        .unwrap_or_default();
    Ok(Step::ExpectSignals {
        signals: matchers,
        order,
        timeout_ms,
        window,
    })
}

/// v0.3.0 Phase B B3 — `- fixture: <id>` short form OR
/// `- fixture: {id: <>, timeoutMs: <>}` long form.
fn parse_fixture(value: &Value) -> Result<Step, ParseError> {
    match value {
        Value::String(id) => Ok(Step::Fixture {
            id: id.clone(),
            timeout_ms: None,
        }),
        Value::Mapping(m) => {
            let id = m
                .get(Value::String("id".into()))
                .and_then(|v| v.as_str().map(String::from))
                .ok_or_else(|| ParseError::MissingField("fixture.id".into()))?;
            let timeout_ms = m
                .get(Value::String("timeoutMs".into()))
                .and_then(|v| v.as_u64());
            Ok(Step::Fixture { id, timeout_ms })
        }
        _ => Err(ParseError::InvalidValue {
            field: "fixture".into(),
            reason: "expected string (id) or mapping {id, timeoutMs}".into(),
        }),
    }
}

fn parse_signal_window(value: &Value) -> Option<crate::SignalWindow> {
    if let Value::Mapping(m) = value {
        if let Some(n) = m
            .get(Value::String("sinceStep".into()))
            .and_then(|v| v.as_u64())
        {
            return Some(crate::SignalWindow::SinceStep {
                since_step: n as usize,
            });
        }
        if let Some(n) = m
            .get(Value::String("lastMs".into()))
            .and_then(|v| v.as_u64())
        {
            return Some(crate::SignalWindow::LastMs { last_ms: n });
        }
        if m.get(Value::String("sinceRun".into())).is_some() {
            return Some(crate::SignalWindow::SinceRun);
        }
    } else if let Value::String(s) = value {
        // Shorthand: `window: sinceRun`
        return match s.as_str() {
            "sinceRun" => Some(crate::SignalWindow::SinceRun),
            _ => None,
        };
    }
    None
}

/// Parse a maestro YAML string into a [`Flow`].
///
/// The yaml must consist of (1) a top-level `appId: <bundle-id>` header,
/// (2) a `---` document separator, and (3) a sequence of single-key
/// mappings, one per command. Each command's value is dispatched to the
/// per-command parser; unknown commands surface as
/// [`ParseError::UnsupportedCommand`].
///
/// # Errors
///
/// See [`ParseError`].
pub fn parse_flow_yaml(yaml: &str) -> Result<Flow, ParseError> {
    // serde_norway::Deserializer::from_str yields each `---`-separated
    // document; the maestro convention is `header --- steps`.
    let mut docs = Vec::new();
    for doc in serde_norway::Deserializer::from_str(yaml) {
        let value = Value::deserialize(doc)?;
        docs.push(value);
    }
    if docs.is_empty() {
        return Err(ParseError::MissingField("appId".into()));
    }

    let (app_id, app) = extract_app_header(&docs[0])?;

    // Steps live in the document(s) after the header. If the header doc
    // already contains the steps (single-doc form), use those; otherwise
    // concatenate every following doc's sequence.
    let mut steps = Vec::new();
    let header_steps = extract_steps_from_doc(&docs[0]);
    if let Some(seq) = header_steps {
        for item in seq {
            steps.push(parse_step_item(item)?);
        }
    }
    for doc in &docs[1..] {
        if let Some(seq) = doc.as_sequence() {
            for item in seq {
                steps.push(parse_step_item(item)?);
            }
        } else if !doc.is_null() {
            return Err(ParseError::InvalidValue {
                field: "<document>".into(),
                reason: "expected a sequence of step mappings".into(),
            });
        }
    }

    // v5.3 c3 — bare `- launchApp` steps inherit the flow header appId here
    // (parser layer; runtime stays appId-agnostic).
    for step in &mut steps {
        if let Step::LaunchApp { app_id: sid, .. } = step
            && sid.is_empty()
        {
            *sid = app_id.clone();
        }
    }
    Ok(Flow { app_id, app, steps })
}

/// v6.0 c4 — extract `(app_id, app)` from yaml header. Backward-compatible:
/// accepts legacy `appId:` literal, new `app:` logical key, or both. At
/// least one field key must be PRESENT (explicit empty `appId: ""` is
/// allowed for v5 tests probing the "no launch" boundary).
fn extract_app_header(doc: &Value) -> Result<(String, Option<String>), ParseError> {
    let map = doc.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "<header>".into(),
        reason: "expected a mapping with `appId` or `app`".into(),
    })?;
    let has_app_id = map.contains_key(Value::String("appId".into()));
    let has_app = map.contains_key(Value::String("app".into()));
    if !has_app_id && !has_app {
        return Err(ParseError::MissingField("app or appId".into()));
    }
    let app_id = map
        .get(Value::String("appId".into()))
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_default();
    let app = map
        .get(Value::String("app".into()))
        .and_then(Value::as_str)
        .map(str::to_string);
    Ok((app_id, app))
}

fn extract_steps_from_doc(doc: &Value) -> Option<&Vec<Value>> {
    // If the header is a `{appId, steps?}` map with an explicit `steps`
    // key (rare but valid), surface that.
    doc.as_mapping()
        .and_then(|m| m.get(Value::String("steps".into())))
        .and_then(Value::as_sequence)
}

fn parse_step_item(item: &Value) -> Result<Step, ParseError> {
    // Bare scalar like `- waitForAnimationToEnd` is a single-key with
    // null value once parsed.
    if let Some(s) = item.as_str() {
        return dispatch_step(s, &Value::Null);
    }
    let map = item.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "<step>".into(),
        reason: format!("expected a single-key mapping or scalar, got {item:?}"),
    })?;
    if map.len() != 1 {
        return Err(ParseError::InvalidValue {
            field: "<step>".into(),
            reason: format!("expected exactly one command key, got {} keys", map.len()),
        });
    }
    let (k, v) = map.iter().next().expect("len == 1 above");
    let key = k.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: "<step>".into(),
        reason: "command key must be a string".into(),
    })?;
    dispatch_step(key, v)
}

// -------------------- recursive parse_flow_file --------------------------

/// Parse a maestro YAML file and recursively expand unconditional
/// `runFlow` references inline.
///
/// Conditional `runFlow: { when, file }` steps stay as
/// [`Step::RunFlowConditional`] leaves — the parser does not evaluate
/// `when.visible` (that is the runtime adapter's job in c3).
///
/// Cycle detection: a `Vec<PathBuf>` stack tracks the in-progress
/// canonicalized paths; re-entering a path returns
/// [`ParseError::RunFlowCycle`] with the full stack.
///
/// # Errors
///
/// See [`ParseError`].
pub fn parse_flow_file(path: &Path) -> Result<Flow, ParseError> {
    let mut stack: Vec<PathBuf> = Vec::new();
    parse_flow_file_inner(path, &mut stack)
}

fn parse_flow_file_inner(path: &Path, stack: &mut Vec<PathBuf>) -> Result<Flow, ParseError> {
    let abs = path
        .canonicalize()
        .map_err(|e| ParseError::Io(format!("canonicalize {}: {e}", path.display())))?;

    if stack.contains(&abs) {
        let mut stack_snapshot = stack.clone();
        stack_snapshot.push(abs.clone());
        return Err(ParseError::RunFlowCycle {
            path: abs,
            stack: stack_snapshot,
        });
    }

    stack.push(abs.clone());
    let result = parse_flow_file_body(&abs, stack);
    stack.pop();
    result
}

fn parse_flow_file_body(abs: &Path, stack: &mut Vec<PathBuf>) -> Result<Flow, ParseError> {
    let yaml = std::fs::read_to_string(abs)
        .map_err(|e| ParseError::Io(format!("read {}: {e}", abs.display())))?;
    let flow = parse_flow_yaml(&yaml)?;

    let dir = abs
        .parent()
        .ok_or_else(|| ParseError::Io(format!("{} has no parent directory", abs.display())))?;

    let mut expanded: Vec<Step> = Vec::with_capacity(flow.steps.len());
    for step in flow.steps {
        match step {
            Step::RunFlow(rel) => {
                let child = dir.join(&rel);
                let inner = parse_flow_file_inner(&child, stack)?;
                expanded.extend(inner.steps);
            }
            // `RunFlowConditional` paths are preserved (runtime evaluates
            // `when.visible` before deciding to expand). Rewrite the
            // relative path against THIS file's dir so the runtime can
            // resolve it regardless of which outer flow has expanded us
            // (`flow_a` runFlow-ing `flow_b` would otherwise leak its
            // own base dir into `flow_b`'s conditional paths).
            Step::RunFlowConditional {
                file,
                when_visible,
                when_not_visible,
                as_name,
            } => {
                let resolved = dir.join(&file);
                let resolved_str = resolved.to_string_lossy().to_string();
                expanded.push(Step::RunFlowConditional {
                    file: resolved_str,
                    when_visible,
                    when_not_visible,
                    as_name,
                });
            }
            other => expanded.push(other),
        }
    }

    Ok(Flow {
        app_id: flow.app_id,
        app: flow.app,
        steps: expanded,
    })
}

//! `IRAction[]` → maestro yaml flow.

use smix_authoring_ir::{IRAction, RecorderError, RecorderErrorReason};
use smix_input::SwipeDirection;
use smix_selector::{Modifiers, Pattern, Selector};

const DEFAULT_WAIT_TIMEOUT_MS: u64 = 5000;

/// Emit a maestro-compatible yaml flow from an action stream.
///
/// Mapping per `IRAction` kind:
/// - tap → `tapOn` (selector inline or as map)
/// - fill → `tapOn` + `inputText`
/// - clear → `eraseText: 100`
/// - press_key → `pressKey: <CapitalizedName>` (same convention as the maestro docs)
/// - swipe → `swipe: { direction: UP|DOWN|LEFT|RIGHT }`
/// - go_back → `back`
/// - wait_for → `extendedWaitUntil: { visible: <sel>, timeout: 5000 }`
/// - hide_keyboard → `hideKeyboard`
///
/// Empty action stream returns `RecorderError(EmptySession)` — same
/// discipline as TS source.
pub fn generate_maestro_yaml(actions: &[IRAction], app_id: &str) -> Result<String, RecorderError> {
    if actions.is_empty() {
        return Err(RecorderError::new(
            RecorderErrorReason::EmptySession,
            "cannot emit maestro yaml from zero actions",
        ));
    }
    let mut steps: Vec<serde_norway::Value> = Vec::with_capacity(actions.len() + 4);
    for a in actions {
        if let Some(selector) = action_selector(a)
            && let Some(form) = maestro_unsupported(selector)
        {
            return Err(RecorderError::new(
                RecorderErrorReason::MalformedAction,
                format!(
                    "recorded {} uses a `{form}` selector, which maestro yaml cannot express; \
                     generate Rust instead",
                    a.kind()
                ),
            ));
        }
        append_step(&mut steps, a);
    }

    // Header: `appId: <id>\n---\n` then list of steps.
    let mut header = serde_norway::Mapping::new();
    header.insert(
        serde_norway::Value::String("appId".to_string()),
        serde_norway::Value::String(app_id.to_string()),
    );
    let header_str =
        serde_norway::to_string(&serde_norway::Value::Mapping(header)).map_err(|e| {
            RecorderError::new(
                RecorderErrorReason::MalformedAction,
                format!("yaml header serialize failed: {e}"),
            )
        })?;
    let header_str = header_str.trim_end();
    let body = serde_norway::to_string(&steps).map_err(|e| {
        RecorderError::new(
            RecorderErrorReason::MalformedAction,
            format!("yaml steps serialize failed: {e}"),
        )
    })?;
    let body = body.trim_end();

    Ok(format!("{header_str}\n---\n{body}\n"))
}

/// The selector a step will serialize, if it serializes one. Exhaustive
/// so a new `IRAction` carrying a selector cannot skip the gate below.
fn action_selector(action: &IRAction) -> Option<&Selector> {
    match action {
        IRAction::Tap { selector, .. }
        | IRAction::Fill { selector, .. }
        | IRAction::WaitFor { selector, .. } => Some(selector),
        // Clear emits `eraseText: <count>` and Swipe emits a direction;
        // neither puts its selector on the wire.
        IRAction::Clear { .. }
        | IRAction::Swipe { .. }
        | IRAction::PressKey { .. }
        | IRAction::GoBack { .. }
        | IRAction::HideKeyboard { .. } => None,
    }
}

/// Selector forms maestro yaml has no spelling for. Emitting the flow
/// anyway hands the caller yaml that dies on that step — the exact
/// failure this generator already shipped once with `swipeOnce`.
fn maestro_unsupported(selector: &Selector) -> Option<&'static str> {
    match selector {
        Selector::Focused { .. } => Some("focused"),
        Selector::Fallback { fallback } => fallback.iter().find_map(maestro_unsupported),
        _ => None,
    }
}

fn append_step(steps: &mut Vec<serde_norway::Value>, action: &IRAction) {
    use serde_norway::{Mapping, Value};
    match action {
        IRAction::Tap { selector, .. } => {
            let mut m = Mapping::new();
            m.insert(Value::String("tapOn".into()), serialize_selector(selector));
            steps.push(Value::Mapping(m));
        }
        IRAction::Fill { selector, text, .. } => {
            let mut tap_map = Mapping::new();
            tap_map.insert(Value::String("tapOn".into()), serialize_selector(selector));
            steps.push(Value::Mapping(tap_map));
            let mut fill_map = Mapping::new();
            fill_map.insert(
                Value::String("inputText".into()),
                Value::String(text.clone()),
            );
            steps.push(Value::Mapping(fill_map));
        }
        IRAction::Clear { .. } => {
            // maestro eraseText takes a char count; 100 is a sensible
            // upper bound (mirrors maestro docs example).
            let mut m = Mapping::new();
            m.insert(
                Value::String("eraseText".into()),
                Value::Number(100u64.into()),
            );
            steps.push(Value::Mapping(m));
        }
        IRAction::PressKey { key, .. } => {
            // maestro convention: key names start uppercase (Return,
            // Backspace, etc.). smix KeyName is camelCase lowercase.
            let mut m = Mapping::new();
            m.insert(
                Value::String("pressKey".into()),
                Value::String(capitalize_first(key.as_str())),
            );
            steps.push(Value::Mapping(m));
        }
        IRAction::Swipe { direction, .. } => {
            // `swipe`, not `swipeOnce`: the latter is in no verb table and
            // the maestro parser has never dispatched it, so every flow
            // recorded with a swipe in it died on UnsupportedCommand.
            //
            // A captured `from` anchor is dropped here — maestro's swipe
            // takes a direction or a coordinate pair, never an element,
            // and the coordinates are only known at replay time. The Rust
            // generator keeps the anchor; the yaml one cannot.
            let mut inner = Mapping::new();
            inner.insert(
                Value::String("direction".into()),
                Value::String(swipe_to_maestro(*direction).into()),
            );
            let mut m = Mapping::new();
            m.insert(Value::String("swipe".into()), Value::Mapping(inner));
            steps.push(Value::Mapping(m));
        }
        IRAction::GoBack { .. } => {
            steps.push(Value::String("back".into()));
        }
        IRAction::WaitFor { selector, .. } => {
            let mut inner = Mapping::new();
            inner.insert(
                Value::String("visible".into()),
                serialize_selector(selector),
            );
            inner.insert(
                Value::String("timeout".into()),
                Value::Number(DEFAULT_WAIT_TIMEOUT_MS.into()),
            );
            let mut m = Mapping::new();
            m.insert(
                Value::String("extendedWaitUntil".into()),
                Value::Mapping(inner),
            );
            steps.push(Value::Mapping(m));
        }
        IRAction::HideKeyboard { .. } => {
            steps.push(Value::String("hideKeyboard".into()));
        }
    }
}

fn capitalize_first(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    if let Some(c) = chars.next() {
        for u in c.to_uppercase() {
            out.push(u);
        }
        out.push_str(chars.as_str());
    }
    out
}

fn swipe_to_maestro(d: SwipeDirection) -> &'static str {
    match d {
        SwipeDirection::Up => "UP",
        SwipeDirection::Down => "DOWN",
        SwipeDirection::Left => "LEFT",
        SwipeDirection::Right => "RIGHT",
    }
}

/// Serialize Selector → maestro yaml node:
/// - text-only string selector w/o modifiers → bare string shorthand
/// - text-only regex selector → `{ text: { regex: "..." } }`
/// - other base forms → map with each field
fn serialize_selector(selector: &Selector) -> serde_norway::Value {
    use serde_norway::{Mapping, Value};
    match selector {
        // Both patterns emit a bare string. maestro yaml has no
        // `{regex: ...}` selector form — `text:` takes a string and the
        // parser infers a regex from it (`|` alternation) via
        // `text_to_pattern`. The map form this used to emit parsed
        // nowhere, so any recorded regex selector killed the flow.
        //
        // Lossy on purpose for a regex without `|`: it re-reads as a
        // literal, because maestro cannot say otherwise. The Rust
        // generator keeps the distinction.
        Selector::Text { text, modifiers } if is_empty_modifiers(modifiers) => match text {
            Pattern::Text(s) => Value::String(s.clone()),
            Pattern::Regex { regex, .. } => Value::String(regex.clone()),
        },
        Selector::Id { id, .. } => {
            let mut m = Mapping::new();
            m.insert(Value::String("id".into()), Value::String(id.clone()));
            Value::Mapping(m)
        }
        Selector::Label { label, .. } => {
            let mut m = Mapping::new();
            m.insert(Value::String("label".into()), Value::String(label.clone()));
            Value::Mapping(m)
        }
        // Other / multi-field selectors — emit a generic map via
        // serde_json round-trip then re-parse as yaml. Maestro's
        // selector surface is narrower than smix's; complex selectors
        // are rare in recorded streams (recorder writes plain text/id
        // selectors per the host-side capture sites).
        _ => {
            let json = serde_json::to_value(selector).unwrap_or(serde_json::Value::Null);
            json_to_yaml(&json)
        }
    }
}

fn is_empty_modifiers(m: &Modifiers) -> bool {
    m.is_empty()
}

fn json_to_yaml(v: &serde_json::Value) -> serde_norway::Value {
    use serde_norway::{Mapping, Number, Value};
    match v {
        serde_json::Value::Null => Value::Null,
        serde_json::Value::Bool(b) => Value::Bool(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Number(Number::from(i))
            } else if let Some(f) = n.as_f64() {
                Value::Number(Number::from(f))
            } else {
                Value::Null
            }
        }
        serde_json::Value::String(s) => Value::String(s.clone()),
        serde_json::Value::Array(a) => Value::Sequence(a.iter().map(json_to_yaml).collect()),
        serde_json::Value::Object(o) => {
            let mut m = Mapping::new();
            for (k, v) in o {
                m.insert(Value::String(k.clone()), json_to_yaml(v));
            }
            Value::Mapping(m)
        }
    }
}

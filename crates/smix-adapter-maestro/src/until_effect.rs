//! `tapOn` / `pressKey` with an effect to wait for: `until`, `within`,
//! `retries`.
//!
//! ```yaml
//! - tapOn:
//!     id: btn-open-menu
//!     until: { visible: { id: btn-close-menu } }
//!     within: 2000          # ms the effect has to show in (default 2000)
//!     retries: 2            # presses again, at most this often (default 1)
//! - pressKey: { key: enter, until: { notVisible: { id: input-email } }, retries: 1 }
//! ```
//!
//! A consumer's flows put a fixed wait after a tap and then tapped again
//! "in case the first was swallowed" — which taps twice when the first
//! one only took a moment to show, and a second back pops one screen too
//! many. Here the action is repeated only while the effect has not shown
//! AND the target is still where it was (for a key, the same field still
//! holds focus): an action that did something else moved or removed it.

use serde_norway::{Mapping, Value};
use smix_sdk::{ExpectationFailure, FailureCode, FailureInit, Rect, Selector};

use crate::parser::{reject_unknown_keys, selector_under};
use crate::{ParseError, Step};

/// How long the effect has to show when `within` is not given.
pub(crate) const DEFAULT_WITHIN_MS: u64 = 2000;
/// How many more times the action is done when `retries` is not given.
pub(crate) const DEFAULT_RETRIES: u32 = 1;
const EFFECT_KEYS: [&str; 3] = ["until", "within", "retries"];

/// Whether `map` asks for an effect to be waited for.
pub(crate) fn asks_for_effect(map: &Mapping) -> bool {
    EFFECT_KEYS
        .iter()
        .any(|k| map.contains_key(Value::String((*k).into())))
}

/// `map` without the effect keys, for the action's own parser.
pub(crate) fn action_part(map: &Mapping) -> Mapping {
    map.iter()
        .filter(|(k, _)| !k.as_str().is_some_and(|k| EFFECT_KEYS.contains(&k)))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect()
}

/// Wrap `action` (already parsed from [`action_part`]) in the effect `map`
/// asks for.
pub(crate) fn wrap(verb: &str, action: Step, map: &Mapping) -> Result<Step, ParseError> {
    let field = |k: &str| format!("{verb}.{k}");
    let until = map
        .get(Value::String("until".into()))
        .ok_or_else(|| ParseError::InvalidValue {
            field: field("until"),
            reason:
                "`within` and `retries` say how to wait for an effect; `until: { visible: … }` \
                     or `{ notVisible: … }` says which"
                    .into(),
        })?;
    let until_map = until.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: field("until"),
        reason: format!(
            "expected `{{ visible: <selector> }}` or `{{ notVisible: <selector> }}`, got {until:?}"
        ),
    })?;
    reject_unknown_keys(
        until_map,
        &field("until"),
        "until",
        &["visible", "notVisible"],
        &[],
    )?;
    let (selector, visible) = match (
        until_map.get(Value::String("visible".into())),
        until_map.get(Value::String("notVisible".into())),
    ) {
        (Some(v), None) => (selector_under(v, &field("until.visible"))?, true),
        (None, Some(v)) => (selector_under(v, &field("until.notVisible"))?, false),
        _ => {
            return Err(ParseError::InvalidValue {
                field: field("until"),
                reason: "give one of `visible:` or `notVisible:`".into(),
            });
        }
    };
    let number = |k: &str, default: u64, max: u64| -> Result<u64, ParseError> {
        match map.get(Value::String(k.into())) {
            None => Ok(default),
            Some(v) => v
                .as_u64()
                .filter(|n| (1..=max).contains(n) || (k == "retries" && *n == 0))
                .ok_or_else(|| ParseError::InvalidValue {
                    field: field(k),
                    reason: format!("expected a whole number up to {max}, got {v:?}"),
                }),
        }
    };
    let within_ms = number("within", DEFAULT_WITHIN_MS, 60_000)?;
    let retries = number("retries", u64::from(DEFAULT_RETRIES), 10)? as u32;
    Ok(Step::UntilEffect {
        action: Box::new(action),
        until: selector,
        until_visible: visible,
        within_ms,
        retries,
    })
}

/// Where the action's target stands: the element a tap names, or the field
/// that holds focus for a key. `None` when it is not on screen.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Anchor {
    pub id: Option<String>,
    pub bounds: Rect,
}

/// The selector that finds `action`'s target: what a tap names, or the
/// focused field for a key.
pub(crate) fn target_of(action: &Step) -> Selector {
    match action {
        Step::TapOn { selector, .. } => selector.clone(),
        _ => smix_sdk::focused(),
    }
}

/// The failure when the effect never showed.
pub(crate) fn not_seen(
    verb: &str,
    until: &Selector,
    visible: bool,
    within_ms: u64,
    times: u32,
    moved: bool,
) -> ExpectationFailure {
    let what = smix_sdk::describe_selector(until);
    let effect = if visible {
        format!("{what} appear")
    } else {
        format!("{what} go")
    };
    let why = if moved {
        "its target moved or left after the last one, so it was not done again"
    } else {
        "the target is still where it was, so the action landed and the screen did not change"
    };
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::AssertionFailed),
        message: format!(
            "{verb} was done {times} time{s} and {effect} did not happen within {within_ms} ms of \
             any of them; {why}",
            s = if times == 1 { "" } else { "s" },
        ),
        selector: Some(until.clone()),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn map(s: &str) -> Mapping {
        serde_norway::from_str::<Value>(s)
            .unwrap()
            .as_mapping()
            .unwrap()
            .clone()
    }

    #[test]
    fn the_effect_keys_are_split_from_the_action() {
        let m = map("{ id: menu, until: { visible: { id: close } }, within: 1500, retries: 2 }");
        assert!(asks_for_effect(&m));
        let a = action_part(&m);
        assert_eq!(a.len(), 1);
        assert!(!asks_for_effect(&map("{ id: menu, optional: true }")));
    }

    #[test]
    fn an_effect_names_one_condition_and_sane_numbers() {
        let tap = Step::TapOn {
            selector: smix_sdk::focused(),
            optional: false,
            dispatch: None,
        };
        let s = wrap(
            "tapOn",
            tap.clone(),
            &map("{ until: { notVisible: { id: x } } }"),
        )
        .unwrap();
        let Step::UntilEffect {
            until_visible,
            within_ms,
            retries,
            ..
        } = s
        else {
            panic!()
        };
        assert_eq!((until_visible, within_ms, retries), (false, 2000, 1));
        let e = wrap("tapOn", tap.clone(), &map("{ within: 100 }"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("until"), "{e}");
        let e = wrap(
            "tapOn",
            tap.clone(),
            &map("{ until: { visible: a, notVisible: b } }"),
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("one of"), "{e}");
        let e = wrap("tapOn", tap, &map("{ until: { visible: a }, retries: 99 }"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("up to 10"), "{e}");
    }
}

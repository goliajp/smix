//! `drag:` — press, hold, move, rest, lift, as one touch.
//!
//! ```yaml
//! - drag:
//!     from: { id: weather_widget }   # an element, or a point "50%,80%"
//!     to: "50%,8%"                   # same forms
//!     hold: 500                      # ms still before moving
//!     duration: 800                  # ms moving
//!     release: 300                   # ms resting before the lift
//! ```

use std::time::Duration;

use serde_norway::{Mapping, Value};
use smix_input::Drag;
use smix_selector::Selector;

use crate::parser::{millis, parse_point, reject_unknown_keys, visible_to_selector};
use crate::runtime::AppLike;
use crate::{ParseError, Step};
use smix_sdk::{ExpectationFailure, FailureCode, FailureInit};

const KEYS: &[&str] = &["from", "to", "hold", "duration", "release"];

fn get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.get(Value::String(key.into()))
}

/// One end: a string is a point, a mapping is a selector (`point:` in it
/// included).
fn end(v: &Value, field: &str) -> Result<Selector, ParseError> {
    let point = |s: &str| {
        parse_point(s)
            .map(|(nx, ny)| Selector::Point { nx, ny })
            .map_err(|e| match e {
                ParseError::InvalidValue { reason, .. } => ParseError::InvalidValue {
                    field: field.into(),
                    reason,
                },
                other => other,
            })
    };
    match v {
        Value::String(s) => point(s),
        Value::Mapping(m) => match get(m, "point") {
            Some(Value::String(s)) if m.len() == 1 => point(s),
            Some(_) => Err(ParseError::InvalidValue {
                field: format!("{field}.point"),
                reason: "expected `point: \"X%,Y%\"` and nothing beside it".into(),
            }),
            None => visible_to_selector(v),
        },
        other => Err(ParseError::InvalidValue {
            field: field.into(),
            reason: format!(
                "expected an element like `{{ id: … }}` or a point \"X%,Y%\", got {other:?}"
            ),
        }),
    }
}

fn ms(map: &Mapping, key: &str, default: u64) -> Result<u64, ParseError> {
    get(map, key).map_or(Ok(default), |v| millis(v, &format!("drag.{key}")))
}

pub(crate) fn parse(v: &Value) -> Result<Step, ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "drag".into(),
        reason: "expected a mapping with `from:` and `to:`".into(),
    })?;
    reject_unknown_keys(map, "drag", "drag", KEYS, &[])?;
    let from = end(
        get(map, "from").ok_or_else(|| ParseError::MissingField("drag.from".into()))?,
        "drag.from",
    )?;
    let to = end(
        get(map, "to").ok_or_else(|| ParseError::MissingField("drag.to".into()))?,
        "drag.to",
    )?;
    let timing = Drag::new((0.5, 0.5), (0.5, 0.5))
        .with_hold_ms(ms(map, "hold", smix_input::DRAG_DEFAULT_HOLD_MS)?)
        .with_duration_ms(ms(map, "duration", smix_input::DRAG_DEFAULT_DURATION_MS)?)
        .with_release_ms(ms(map, "release", smix_input::DRAG_DEFAULT_RELEASE_MS)?);
    timing.check().map_err(|why| ParseError::InvalidValue {
        field: "drag".into(),
        reason: why,
    })?;
    Ok(Step::Drag {
        from,
        to,
        hold_ms: timing.hold_ms,
        duration_ms: timing.duration_ms,
        release_ms: timing.release_ms,
    })
}

/// The centre of the element `sel` names, looked for until `budget` runs
/// out, as every tree-based verb waits.
pub(crate) async fn element_centre<A: AppLike + ?Sized>(
    app: &A,
    sel: &Selector,
    end: &str,
    budget: Duration,
) -> Result<(f64, f64), ExpectationFailure> {
    match sel {
        // Read as points before this is reached; here, nothing matched.
        Selector::OcrText { .. } | Selector::AnchorRelative { .. } => {
            return Err(not_found(sel, end, "nothing on screen read as it"));
        }
        Selector::Text { .. }
        | Selector::Id { .. }
        | Selector::Label { .. }
        | Selector::Role { .. }
        | Selector::Focused { .. }
        | Selector::Anchor { .. }
        | Selector::LocalizedText { .. }
        | Selector::Point { .. }
        | Selector::Fallback { .. } => {}
    }
    let budget = smix_driver::poll::Budget::new(budget);
    loop {
        let look = budget.look();
        let tree = app.tree().await?;
        match smix_host_coord_resolver::resolve_to_norm_point_in_box(&tree, sel, (0.5, 0.5)) {
            Ok(at) => return Ok(at),
            Err(e) if budget.spent_by(look) => return Err(not_found(sel, end, &format!("{e:?}"))),
            Err(_) => tokio::time::sleep(Duration::from_millis(250)).await,
        }
    }
}

fn not_found(sel: &Selector, end: &str, why: &str) -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::ElementNotFound),
        message: format!("drag {end} {}: {why}", smix_sdk::describe_selector(sel)),
        selector: Some(sel.clone()),
        hint: Some(
            "each end of a drag is the centre of the named element, so it has to be on \
             screen; a place with nothing nameable takes a point, e.g. `to: \"50%,8%\"`"
                .into(),
        ),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(yaml: &str) -> Result<Step, ParseError> {
        parse(&serde_norway::from_str::<Value>(yaml).unwrap())
    }

    fn reason(e: ParseError) -> String {
        match e {
            ParseError::InvalidValue { field, reason } => format!("{field}: {reason}"),
            other => other.to_string(),
        }
    }

    #[test]
    fn an_element_to_a_point_with_the_default_timing() {
        let Step::Drag {
            from,
            to,
            hold_ms,
            duration_ms,
            release_ms,
        } = step("from: { id: widget }\nto: \"50%,8%\"").unwrap()
        else {
            panic!("not a drag")
        };
        assert!(matches!(from, Selector::Id { .. }), "{from:?}");
        assert_eq!(to, Selector::Point { nx: 0.5, ny: 0.08 });
        assert_eq!((hold_ms, duration_ms, release_ms), (500, 800, 300));
    }

    #[test]
    fn a_point_may_be_written_as_a_point_key_and_timings_are_read() {
        let Step::Drag {
            from,
            hold_ms,
            duration_ms,
            release_ms,
            ..
        } = step(
            "from: { point: \"20%,30%\" }\nto: \"80%,90%\"\nhold: 1500\nduration: 400\nrelease: 0",
        )
        .unwrap()
        else {
            panic!("not a drag")
        };
        assert_eq!(from, Selector::Point { nx: 0.2, ny: 0.3 });
        assert_eq!((hold_ms, duration_ms, release_ms), (1500, 400, 0));
    }

    #[test]
    fn both_ends_are_required_and_unknown_keys_refused() {
        assert!(
            step("from: \"50%,50%\"")
                .unwrap_err()
                .to_string()
                .contains("drag.to")
        );
        assert!(
            step("to: \"50%,50%\"")
                .unwrap_err()
                .to_string()
                .contains("drag.from")
        );
        assert!(
            reason(step("from: \"1%,1%\"\nto: \"2%,2%\"\nspeed: 3").unwrap_err())
                .contains("unknown key `speed`")
        );
    }

    #[test]
    fn a_timing_the_runners_refuse_is_refused_here() {
        let e = reason(step("from: \"1%,1%\"\nto: \"2%,2%\"\nduration: 10").unwrap_err());
        assert!(e.starts_with("drag: duration 10 ms"), "{e}");
        let e = reason(step("from: \"1%,1%\"\nto: \"2%,2%\"\nhold: 20000").unwrap_err());
        assert!(e.starts_with("drag: hold"), "{e}");
    }

    #[test]
    fn a_point_off_the_screen_names_its_end() {
        let e = reason(step("from: \"150%,50%\"\nto: \"2%,2%\"").unwrap_err());
        assert!(e.starts_with("drag.from"), "{e}");
    }
}

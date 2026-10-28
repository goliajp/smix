//! `pinch:` — two fingers about an element's centre or a point.
//!
//! ```yaml
//! - pinch:
//!     over: { id: player }   # or  at: "50%,50%"
//!     from: 1                # zoom factors, or fromSpan / toSpan
//!     to: 3
//!     duration: 500          # ms
//!     pan: "0%,-20%"         # optional: drag before lifting
//!     panDuration: 300       # ms
//! ```

use serde_norway::{Mapping, Value};
use smix_input::{Pan, Pinch, PinchSpans, RunUp};

use crate::parser::{millis, parse_point, reject_unknown_keys, visible_to_selector};
use crate::runtime::AppLike;
use crate::{ParseError, PinchAt, Step};
use smix_sdk::{ExpectationFailure, FailureCode, FailureInit};

const KEYS: &[&str] = &[
    "over",
    "at",
    "from",
    "to",
    "fromSpan",
    "toSpan",
    "duration",
    "pan",
    "panDuration",
    "runUp",
];

fn get<'a>(map: &'a Mapping, key: &str) -> Option<&'a Value> {
    map.get(Value::String(key.into()))
}

fn number(map: &Mapping, key: &str) -> Result<Option<f64>, ParseError> {
    get(map, key)
        .map(|v| {
            v.as_f64().ok_or_else(|| ParseError::InvalidValue {
                field: format!("pinch.{key}"),
                reason: format!("expected a number, got {v:?}"),
            })
        })
        .transpose()
}

fn invalid(reason: impl Into<String>) -> ParseError {
    ParseError::InvalidValue {
        field: "pinch".into(),
        reason: reason.into(),
    }
}

pub(crate) fn parse(v: &Value) -> Result<Step, ParseError> {
    let map = v
        .as_mapping()
        .ok_or_else(|| invalid("expected a mapping with `over:` or `at:` and `from`/`to`"))?;
    reject_unknown_keys(map, "pinch", "pinch", KEYS, &[])?;
    let at = match (get(map, "over"), get(map, "at")) {
        (Some(over), None) => PinchAt::Over(visible_to_selector(over)?),
        (None, Some(at)) => PinchAt::Point(point(at)?),
        (Some(_), Some(_)) => {
            return Err(invalid(
                "give `over:` (an element) or `at:` (a point), not both",
            ));
        }
        (None, None) => {
            return Err(invalid(
                "say where to pinch: `over: { id: … }` names the element whose centre the \
                 fingers meet at; `at: \"50%,50%\"` is a point on the screen",
            ));
        }
    };
    let spans = match (
        number(map, "from")?,
        number(map, "to")?,
        number(map, "fromSpan")?,
        number(map, "toSpan")?,
    ) {
        (Some(from), Some(to), None, None) => PinchSpans::Scales { from, to },
        (None, None, Some(from), Some(to)) => PinchSpans::Spans { from, to },
        _ => {
            return Err(invalid(
                "give the zoom as `from` and `to` (factors, e.g. 1 and 3), or the finger \
                 distances as `fromSpan` and `toSpan` (shares of the screen's shorter side) — \
                 one pair, both ends",
            ));
        }
    };
    spans.resolve().map_err(invalid)?;
    let duration_ms = match get(map, "duration") {
        Some(d) => millis(d, "pinch.duration")?,
        None => smix_input::PINCH_DEFAULT_DURATION_MS,
    };
    if !smix_input::PINCH_DURATION_RANGE.contains(&duration_ms) {
        return Err(ParseError::InvalidValue {
            field: "pinch.duration".into(),
            reason: format!("{duration_ms} ms is outside 50..=10000 ms"),
        });
    }
    let pan_ms = get(map, "panDuration")
        .map(|d| millis(d, "pinch.panDuration"))
        .transpose()?;
    let pan = match (get(map, "pan"), pan_ms) {
        (Some(p), ms) => {
            let (dx, dy) = offset(p)?;
            Some(Pan::new(
                dx,
                dy,
                ms.unwrap_or(smix_input::PINCH_DEFAULT_PAN_MS),
            ))
        }
        (None, Some(_)) => return Err(invalid("`panDuration` without `pan` drags nowhere")),
        (None, None) => None,
    };
    let run_up =
        match get(map, "runUp") {
            None => RunUp::default(),
            Some(v) => v.as_str().and_then(RunUp::from_name).ok_or_else(|| {
                ParseError::InvalidValue {
                    field: "pinch.runUp".into(),
                    reason: format!(
                        "expected spread, slide or none, got {v:?} — spread suits UIKit, Android's \
                 ScaleGestureDetector and most apps; slide suits a recognizer that counts from \
                 the touch or begins on any movement"
                    ),
                }
            })?,
        };
    Ok(Step::Pinch {
        at,
        spans,
        duration_ms,
        pan,
        run_up,
    })
}

fn point(v: &Value) -> Result<(f64, f64), ParseError> {
    let rename = |e: ParseError| match e {
        ParseError::InvalidValue { reason, .. } => ParseError::InvalidValue {
            field: "pinch.at".into(),
            reason,
        },
        other => other,
    };
    if let Some(s) = v.as_str() {
        return parse_point(s).map_err(rename);
    }
    let (x, y) = pair(v, "pinch.at")?;
    if !((0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)) {
        return Err(ParseError::InvalidValue {
            field: "pinch.at".into(),
            reason: format!("({x}, {y}) is off the screen; both must be shares within 0..1"),
        });
    }
    Ok((x, y))
}

/// A drag distance: `"dx%,dy%"` or `{x, y}` shares, either sign.
fn offset(v: &Value) -> Result<(f64, f64), ParseError> {
    let field = "pinch.pan";
    let (x, y) = match v.as_str() {
        Some(s) => {
            let parts: Vec<&str> = s.split(',').map(str::trim).collect();
            let [a, b] = parts.as_slice() else {
                return Err(ParseError::InvalidValue {
                    field: field.into(),
                    reason: format!("expected `dx%,dy%`, got {s:?}"),
                });
            };
            (share(a, field)?, share(b, field)?)
        }
        None => pair(v, field)?,
    };
    if !((-1.0..=1.0).contains(&x) && (-1.0..=1.0).contains(&y)) {
        return Err(ParseError::InvalidValue {
            field: field.into(),
            reason: format!("({x}, {y}) is more than a screen; both must be within -100%..100%"),
        });
    }
    Ok((x, y))
}

fn share(s: &str, field: &str) -> Result<f64, ParseError> {
    let bad = || ParseError::InvalidValue {
        field: field.into(),
        reason: format!("expected a percentage like `-20%` or a share like `-0.2`, got {s:?}"),
    };
    match s.strip_suffix('%') {
        Some(p) => p
            .trim()
            .parse::<f64>()
            .map(|n| n / 100.0)
            .map_err(|_| bad()),
        None => s.parse::<f64>().map_err(|_| bad()),
    }
}

fn pair(v: &Value, field: &str) -> Result<(f64, f64), ParseError> {
    let map = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: field.into(),
        reason: "expected a string `X%,Y%` or a `{x, y}` map".into(),
    })?;
    let axis = |k: &str| {
        get(map, k)
            .and_then(Value::as_f64)
            .ok_or_else(|| ParseError::MissingField(format!("{field}.{k}")))
    };
    Ok((axis("x")?, axis("y")?))
}

/// Run a parsed `pinch:` step: find the centre, then send the gesture.
pub(crate) async fn run<A: AppLike + ?Sized>(
    app: &A,
    at: &PinchAt,
    spans: PinchSpans,
    duration_ms: u64,
    pan: Option<Pan>,
    run_up: RunUp,
) -> Result<(), ExpectationFailure> {
    let centre = match at {
        PinchAt::Point(p) => *p,
        PinchAt::Over(selector) => {
            let tree = app.tree().await?;
            smix_host_coord_resolver::resolve_to_norm_point_in_box(&tree, selector, (0.5, 0.5))
                .map_err(|e| {
                    ExpectationFailure::new(FailureInit {
                        code: Some(FailureCode::ElementNotFound),
                        message: format!(
                            "pinch over {}: {e:?}",
                            smix_sdk::describe_selector(selector)
                        ),
                        selector: Some(selector.clone()),
                        hint: Some(
                            "`over:` pinches about the named element's centre, so it needs that \
                             element on screen. A screen with nothing nameable there takes \
                             `at: \"50%,50%\"` instead."
                                .into(),
                        ),
                        ..Default::default()
                    })
                })?
        }
    };
    let (from_span, to_span) = spans.resolve().map_err(|why| {
        ExpectationFailure::new(FailureInit {
            code: Some(FailureCode::DriverError),
            message: format!("pinch: {why}"),
            ..Default::default()
        })
    })?;
    app.pinch(
        &Pinch::new(centre, from_span, to_span)
            .with_duration_ms(duration_ms)
            .with_pan(pan)
            .with_run_up(run_up),
    )
    .await
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
    fn over_an_element_with_scales_and_defaults() {
        let Step::Pinch {
            at,
            spans,
            duration_ms,
            pan,
            run_up,
        } = step("over: { id: player }\nfrom: 1\nto: 3").unwrap()
        else {
            panic!("not a pinch")
        };
        assert!(matches!(at, PinchAt::Over(_)), "{at:?}");
        assert_eq!(spans, PinchSpans::Scales { from: 1.0, to: 3.0 });
        assert_eq!(duration_ms, 500);
        assert_eq!(pan, None);
        assert_eq!(run_up, RunUp::Spread);
    }

    #[test]
    fn at_a_point_with_spans_and_a_pan() {
        let s = step(
            "at: \"50%,40%\"\nfromSpan: 0.2\ntoSpan: 0.6\nduration: 800\npan: \"0%,-20%\"\npanDuration: 250",
        )
        .unwrap();
        assert_eq!(
            s,
            Step::Pinch {
                at: PinchAt::Point((0.5, 0.4)),
                spans: PinchSpans::Spans { from: 0.2, to: 0.6 },
                duration_ms: 800,
                pan: Some(Pan::new(0.0, -0.2, 250)),
                run_up: RunUp::Spread,
            }
        );
    }

    #[test]
    fn a_pan_without_a_duration_takes_the_default() {
        let Step::Pinch { pan, .. } =
            step("at: { x: 0.5, y: 0.5 }\nfrom: 3\nto: 1\npan: { x: 0.1, y: 0 }").unwrap()
        else {
            panic!("not a pinch")
        };
        assert_eq!(pan, Some(Pan::new(0.1, 0.0, 300)));
    }

    #[test]
    fn run_up_is_read_by_name_and_a_stranger_is_refused() {
        let Step::Pinch { run_up, .. } =
            step("at: \"50%,50%\"\nfrom: 1\nto: 3\nrunUp: slide").unwrap()
        else {
            panic!("not a pinch")
        };
        assert_eq!(run_up, RunUp::Slide);
        let e = reason(step("at: \"50%,50%\"\nfrom: 1\nto: 3\nrunUp: wiggle").unwrap_err());
        assert!(e.starts_with("pinch.runUp"), "{e}");
    }

    #[test]
    fn where_to_pinch_is_required_and_single() {
        assert!(reason(step("from: 1\nto: 3").unwrap_err()).contains("say where to pinch"));
        assert!(
            reason(step("over: { id: a }\nat: \"50%,50%\"\nfrom: 1\nto: 3").unwrap_err())
                .contains("not both")
        );
    }

    #[test]
    fn the_distances_are_one_whole_pair() {
        for yaml in [
            "at: \"50%,50%\"\nfrom: 1",
            "at: \"50%,50%\"\nfrom: 1\ntoSpan: 0.6",
            "at: \"50%,50%\"\nfrom: 1\nto: 3\nfromSpan: 0.2\ntoSpan: 0.6",
        ] {
            assert!(
                reason(step(yaml).unwrap_err()).contains("one pair"),
                "{yaml}"
            );
        }
        assert!(reason(step("at: \"50%,50%\"\nfrom: 2\nto: 2").unwrap_err()).contains("no pinch"));
    }

    #[test]
    fn out_of_range_values_name_their_field() {
        assert!(
            reason(step("at: { x: 1.5, y: 0.5 }\nfrom: 1\nto: 3").unwrap_err())
                .starts_with("pinch.at")
        );
        assert!(
            reason(step("at: \"50%,50%\"\nfrom: 1\nto: 3\nduration: 10").unwrap_err())
                .starts_with("pinch.duration")
        );
        assert!(
            reason(step("at: \"50%,50%\"\nfrom: 1\nto: 3\npan: \"0%,-150%\"").unwrap_err())
                .starts_with("pinch.pan")
        );
        assert!(
            reason(step("at: \"50%,50%\"\nfrom: 1\nto: 3\npanDuration: 100").unwrap_err())
                .contains("drags nowhere")
        );
        assert!(
            reason(step("at: \"50%,50%\"\nfrom: 1\nto: 3\nscale: 2").unwrap_err())
                .contains("unknown key `scale`")
        );
    }
}

//! `rightClickOn:` / `mouseWheel:` / `mouseDrag:` — a mouse's buttons and
//! wheel on elements, for apps in a desktop window.
//!
//! ```yaml
//! - rightClickOn: { id: row }                  # opens a context menu
//! - mouseWheel: { on: { id: list }, direction: down, notches: 3 }
//! - mouseWheel: { on: { id: map }, direction: up, ctrl: true }   # zoom
//! - mouseDrag: { from: { id: card }, to: { id: done_column } }
//! - mouseDrag: { from: { id: divider }, by: { x: 96, y: 0 } }   # dp
//! ```

use std::time::Duration;

use serde_norway::Value;
use smix_input::Mouse;
use smix_sdk::{ExpectationFailure, FailureCode, FailureInit, Selector};

use crate::parser::{reject_unknown_keys, selector_under, visible_to_selector};
use crate::runtime::AppLike;
use crate::{MouseDragTo, ParseError, Step};

/// How long each element waits to appear, as `hoverOn` waits.
const APPEAR_WITHIN: Duration = Duration::from_secs(5);

pub(crate) fn parse_right_click(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(_) | Value::Mapping(_) => Ok(Step::RightClickOn {
            selector: visible_to_selector(v)?,
        }),
        other => Err(ParseError::InvalidValue {
            field: "rightClickOn".into(),
            reason: format!(
                "expected the element to right-click — a text or a selector like \
                 `{{ id: row }}` — got {other:?}"
            ),
        }),
    }
}

pub(crate) fn parse_wheel(v: &Value) -> Result<Step, ParseError> {
    let m = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "mouseWheel".into(),
        reason: format!(
            "expected `{{ on: <selector>, direction: up|down, notches?, ctrl? }}`, got {v:?}"
        ),
    })?;
    reject_unknown_keys(
        m,
        "mouseWheel",
        "mouseWheel",
        &["on", "direction", "notches", "ctrl"],
        &[],
    )?;
    let on = m
        .get(Value::String("on".into()))
        .ok_or_else(|| ParseError::InvalidValue {
            field: "mouseWheel.on".into(),
            reason: "the element the pointer turns the wheel over is required".into(),
        })?;
    let selector = selector_under(on, "mouseWheel.on")?;
    let up = match m
        .get(Value::String("direction".into()))
        .and_then(Value::as_str)
    {
        Some("up") => true,
        Some("down") => false,
        other => {
            return Err(ParseError::InvalidValue {
                field: "mouseWheel.direction".into(),
                reason: format!("expected up or down, got {other:?}"),
            });
        }
    };
    let notches = match m.get(Value::String("notches".into())) {
        None => 1,
        Some(Value::Number(n)) => match n.as_u64() {
            Some(k @ 1..=100) => k as u32,
            _ => {
                return Err(ParseError::InvalidValue {
                    field: "mouseWheel.notches".into(),
                    reason: format!("expected a whole number from 1 to 100, got {n}"),
                });
            }
        },
        Some(other) => {
            return Err(ParseError::InvalidValue {
                field: "mouseWheel.notches".into(),
                reason: format!("expected a whole number from 1 to 100, got {other:?}"),
            });
        }
    };
    let ctrl = bool_key(m, "ctrl", "mouseWheel.ctrl")?;
    Ok(Step::MouseWheel {
        selector,
        up,
        notches,
        ctrl,
    })
}

pub(crate) fn parse_drag(v: &Value) -> Result<Step, ParseError> {
    let m = v.as_mapping().ok_or_else(|| ParseError::InvalidValue {
        field: "mouseDrag".into(),
        reason: format!(
            "expected `{{ from: <selector>, to: <selector> }}` or \
             `{{ from: <selector>, by: {{ x, y }} }}`, got {v:?}"
        ),
    })?;
    reject_unknown_keys(m, "mouseDrag", "mouseDrag", &["from", "to", "by"], &[])?;
    let from = m
        .get(Value::String("from".into()))
        .ok_or_else(|| ParseError::InvalidValue {
            field: "mouseDrag.from".into(),
            reason: "the element the button goes down on is required".into(),
        })?;
    let from = selector_under(from, "mouseDrag.from")?;
    let to = match (
        m.get(Value::String("to".into())),
        m.get(Value::String("by".into())),
    ) {
        (Some(to), None) => MouseDragTo::Element(selector_under(to, "mouseDrag.to")?),
        (None, Some(by)) => MouseDragTo::ByDp(dp_offset(by)?),
        _ => {
            return Err(ParseError::InvalidValue {
                field: "mouseDrag".into(),
                reason: "give where the drag ends as one of `to: <selector>` or \
                         `by: { x, y }` (dp)"
                    .into(),
            });
        }
    };
    Ok(Step::MouseDrag { from, to })
}

fn dp_offset(v: &Value) -> Result<(f64, f64), ParseError> {
    let bad = |reason: String| ParseError::InvalidValue {
        field: "mouseDrag.by".into(),
        reason,
    };
    let m = v
        .as_mapping()
        .ok_or_else(|| bad(format!("expected `{{ x, y }}` in dp, got {v:?}")))?;
    reject_unknown_keys(m, "mouseDrag.by", "mouseDrag.by", &["x", "y"], &[])?;
    let side = |k: &str| match m.get(Value::String(k.into())) {
        None => Ok(0.0),
        Some(Value::Number(n)) => n
            .as_f64()
            .ok_or_else(|| bad(format!("{k} must be a number of dp, got {n}"))),
        Some(other) => Err(bad(format!("{k} must be a number of dp, got {other:?}"))),
    };
    let by = (side("x")?, side("y")?);
    if by == (0.0, 0.0) {
        return Err(bad("a drag by 0 dp moves nothing; give x, y or both".into()));
    }
    Ok(by)
}

fn bool_key(m: &serde_norway::Mapping, key: &str, field: &str) -> Result<bool, ParseError> {
    match m.get(Value::String(key.into())) {
        None => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(other) => Err(ParseError::InvalidValue {
            field: field.into(),
            reason: format!("expected true or false, got {other:?}"),
        }),
    }
}

/// The centre of the element `selector` names, once it is there.
async fn centre<A: AppLike + ?Sized>(
    app: &A,
    verb: &str,
    selector: &Selector,
) -> Result<(f64, f64), ExpectationFailure> {
    app.wait_for(selector, APPEAR_WITHIN).await?;
    let tree = app.tree().await?;
    smix_host_coord_resolver::resolve_to_norm_point_in_box(&tree, selector, (0.5, 0.5)).map_err(
        |e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!("{verb} {}: {e:?}", smix_sdk::describe_selector(selector)),
                selector: Some(selector.clone()),
                ..Default::default()
            })
        },
    )
}

pub(crate) async fn run_right_click<A: AppLike + ?Sized>(
    app: &A,
    selector: &Selector,
) -> Result<(), ExpectationFailure> {
    let at = centre(app, "rightClickOn", selector).await?;
    app.mouse(&Mouse::SecondaryClick { at }).await
}

pub(crate) async fn run_wheel<A: AppLike + ?Sized>(
    app: &A,
    selector: &Selector,
    up: bool,
    notches: u32,
    ctrl: bool,
) -> Result<(), ExpectationFailure> {
    let at = centre(app, "mouseWheel", selector).await?;
    let turn = notches as f32;
    app.mouse(&Mouse::Wheel {
        at,
        notches: if up { turn } else { -turn },
        ctrl,
    })
    .await
}

pub(crate) async fn run_drag<A: AppLike + ?Sized>(
    app: &A,
    from: &Selector,
    to: &MouseDragTo,
) -> Result<(), ExpectationFailure> {
    let start = centre(app, "mouseDrag", from).await?;
    let mouse = match to {
        MouseDragTo::Element(sel) => Mouse::Drag {
            from: start,
            to: centre(app, "mouseDrag", sel).await?,
        },
        MouseDragTo::ByDp(by_dp) => Mouse::DragBy {
            from: start,
            by_dp: *by_dp,
        },
    };
    app.mouse(&mouse).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(s: &str) -> Value {
        serde_norway::from_str(s).unwrap()
    }

    fn err(r: Result<Step, ParseError>) -> String {
        r.unwrap_err().to_string()
    }

    #[test]
    fn right_click_reads_a_selector_or_a_bare_text() {
        let Step::RightClickOn { selector } = parse_right_click(&yaml("{ id: row }")).unwrap()
        else {
            panic!("not a right click")
        };
        assert_eq!(smix_sdk::describe_selector(&selector), "{ id=\"row\" }");
        assert!(err(parse_right_click(&yaml("[1]"))).contains("right-click"));
    }

    #[test]
    fn a_wheel_turn_names_its_element_and_direction() {
        let s = parse_wheel(&yaml("{ on: { id: list }, direction: down, notches: 3 }")).unwrap();
        let Step::MouseWheel {
            up, notches, ctrl, ..
        } = s
        else {
            panic!("{s:?}")
        };
        assert_eq!((up, notches, ctrl), (false, 3, false));
        let s = parse_wheel(&yaml("{ on: map, direction: up, ctrl: true }")).unwrap();
        assert!(matches!(
            s,
            Step::MouseWheel {
                up: true,
                notches: 1,
                ctrl: true,
                ..
            }
        ));
        assert!(err(parse_wheel(&yaml("{ direction: up }"))).contains("mouseWheel.on"));
        assert!(err(parse_wheel(&yaml("{ on: x, direction: left }"))).contains("up or down"));
        assert!(
            err(parse_wheel(&yaml("{ on: x, direction: up, notches: 0 }"))).contains("1 to 100")
        );
        assert!(err(parse_wheel(&yaml("{ on: x, direction: up, speed: 2 }"))).contains("`speed`"));
    }

    #[test]
    fn a_drag_ends_on_an_element_or_a_distance_in_dp() {
        let s = parse_drag(&yaml("{ from: { id: a }, to: { id: b } }")).unwrap();
        assert!(matches!(
            s,
            Step::MouseDrag {
                to: MouseDragTo::Element(_),
                ..
            }
        ));
        let s = parse_drag(&yaml("{ from: { id: a }, by: { x: 96 } }")).unwrap();
        assert!(matches!(
            s,
            Step::MouseDrag {
                to: MouseDragTo::ByDp((96.0, 0.0)),
                ..
            }
        ));
        assert!(err(parse_drag(&yaml("{ from: a, to: b, by: { x: 1 } }"))).contains("one of"));
        assert!(err(parse_drag(&yaml("{ from: a }"))).contains("one of"));
        assert!(err(parse_drag(&yaml("{ to: b }"))).contains("mouseDrag.from"));
        assert!(
            err(parse_drag(&yaml("{ from: a, by: { x: 0, y: 0 } }"))).contains("moves nothing")
        );
    }
}

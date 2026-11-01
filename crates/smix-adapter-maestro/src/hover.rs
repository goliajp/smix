//! `hoverOn:` / `hoverAway` — a mouse pointer resting on an element, and
//! taken off the app.
//!
//! ```yaml
//! - hoverOn: { id: row_settings }   # or any selector, or a bare text
//! - assertVisible: { id: row_settings_hovered }
//! - hoverAway
//! ```

use std::time::Duration;

use serde_norway::Value;
use smix_input::Mouse;
use smix_sdk::{ExpectationFailure, FailureCode, FailureInit, Selector};

use crate::parser::visible_to_selector;
use crate::runtime::AppLike;
use crate::{ParseError, Step};

/// How long `hoverOn` waits for its element to appear, as `assertNotVisible`
/// waits for one to leave.
const APPEAR_WITHIN: Duration = Duration::from_secs(5);

pub(crate) fn parse_on(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::String(_) | Value::Mapping(_) => Ok(Step::HoverOn {
            selector: visible_to_selector(v)?,
        }),
        other => Err(ParseError::InvalidValue {
            field: "hoverOn".into(),
            reason: format!(
                "expected the element to rest the pointer on — a text or a selector \
                 like `{{ id: row }}` — got {other:?}"
            ),
        }),
    }
}

pub(crate) fn parse_away(v: &Value) -> Result<Step, ParseError> {
    match v {
        Value::Null => Ok(Step::HoverAway),
        Value::Mapping(m) if m.is_empty() => Ok(Step::HoverAway),
        other => Err(ParseError::InvalidValue {
            field: "hoverAway".into(),
            reason: format!(
                "takes nothing: the pointer leaves the app, not for a place in it; got {other:?}"
            ),
        }),
    }
}

/// Rest the pointer on the centre of the element `selector` names.
pub(crate) async fn run_on<A: AppLike + ?Sized>(
    app: &A,
    selector: &Selector,
) -> Result<(), ExpectationFailure> {
    app.wait_for(selector, APPEAR_WITHIN).await?;
    let tree = app.tree().await?;
    let at = smix_host_coord_resolver::resolve_to_norm_point_in_box(&tree, selector, (0.5, 0.5))
        .map_err(|e| {
            ExpectationFailure::new(FailureInit {
                code: Some(FailureCode::ElementNotFound),
                message: format!("hoverOn {}: {e:?}", smix_sdk::describe_selector(selector)),
                selector: Some(selector.clone()),
                ..Default::default()
            })
        })?;
    app.mouse(&Mouse::Hover { at }).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(s: &str) -> Value {
        serde_norway::from_str(s).unwrap()
    }

    #[test]
    fn hover_on_reads_a_selector_or_a_bare_text() {
        let Step::HoverOn { selector } = parse_on(&yaml("{ id: row }")).unwrap() else {
            panic!("not a hover")
        };
        assert_eq!(smix_sdk::describe_selector(&selector), "{ id=\"row\" }");
        assert!(matches!(
            parse_on(&yaml("Settings")).unwrap(),
            Step::HoverOn {
                selector: Selector::Text { .. }
            }
        ));
    }

    #[test]
    fn hover_on_without_an_element_is_refused() {
        let e = parse_on(&yaml("[1, 2]")).unwrap_err().to_string();
        assert!(e.contains("rest the pointer on"), "{e}");
    }

    #[test]
    fn hover_away_takes_nothing() {
        assert_eq!(parse_away(&Value::Null).unwrap(), Step::HoverAway);
        assert_eq!(parse_away(&yaml("{}")).unwrap(), Step::HoverAway);
        let e = parse_away(&yaml("{ id: row }")).unwrap_err().to_string();
        assert!(e.contains("takes nothing"), "{e}");
    }
}

//! The steps that work the device itself: a foldable's hinge and the
//! Digital Crown.

use serde_norway::Value;

use crate::{ParseError, Step};

// `setPosture: <closed|halfOpen|open>` (`half-open` accepted too).
pub(super) fn parse_set_posture(v: &Value) -> Result<Step, ParseError> {
    let s = v.as_str().ok_or_else(|| ParseError::InvalidValue {
        field: "setPosture".into(),
        reason: "expected closed | halfOpen | open".into(),
    })?;
    let posture = s.parse().map_err(|reason| ParseError::InvalidValue {
        field: "setPosture".into(),
        reason,
    })?;
    Ok(Step::SetPosture { posture })
}

// `rotateCrown: <turns>` or `rotateCrown: { rotations: <turns>, velocity: <turns a second> }`.
pub(super) fn parse_rotate_crown(v: &Value) -> Result<Step, ParseError> {
    let bad = |reason: String| ParseError::InvalidValue {
        field: "rotateCrown".into(),
        reason,
    };
    let number = |v: &Value, what: &str| {
        v.as_f64()
            .filter(|n| n.is_finite())
            .ok_or_else(|| bad(format!("{what} must be a number, got {v:?}")))
    };
    let (rotations, velocity) = match v {
        Value::Mapping(m) => {
            for key in m.keys() {
                if !matches!(key.as_str(), Some("rotations" | "velocity")) {
                    return Err(bad(format!(
                        "unknown key {key:?}; expected rotations and, optionally, velocity"
                    )));
                }
            }
            let rotations = m
                .get("rotations")
                .ok_or_else(|| {
                    bad("missing rotations (turns of the crown; 1 is a full turn)".into())
                })
                .and_then(|r| number(r, "rotations"))?;
            let velocity = m
                .get("velocity")
                .map(|r| number(r, "velocity"))
                .transpose()?;
            (rotations, velocity)
        }
        other => (number(other, "rotateCrown")?, None),
    };
    if rotations == 0.0 {
        return Err(bad(
            "a turn of 0 does nothing; 1 is a full turn, negative turns back".into(),
        ));
    }
    if let Some(v) = velocity
        && v <= 0.0
    {
        return Err(bad(format!(
            "velocity is turns a second and must be positive, got {v}"
        )));
    }
    Ok(Step::RotateCrown {
        rotations,
        velocity,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crown(yaml: &str) -> Result<Step, ParseError> {
        parse_rotate_crown(&serde_norway::from_str(yaml).unwrap())
    }

    fn refusal(yaml: &str) -> String {
        match crown(yaml) {
            Err(ParseError::InvalidValue { field, reason }) => {
                assert_eq!(field, "rotateCrown");
                reason
            }
            other => panic!("{yaml}: expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn a_bare_number_and_a_mapping_both_read() {
        assert_eq!(
            crown("0.25").unwrap(),
            Step::RotateCrown {
                rotations: 0.25,
                velocity: None
            }
        );
        assert_eq!(
            crown("{ rotations: -3, velocity: 2 }").unwrap(),
            Step::RotateCrown {
                rotations: -3.0,
                velocity: Some(2.0)
            }
        );
    }

    #[test]
    fn what_is_not_a_turn_is_refused_by_name() {
        assert!(refusal("0").contains("a turn of 0"), "zero");
        assert!(refusal("fast").contains("must be a number"), "word");
        assert!(
            refusal("{ velocity: 1 }").contains("missing rotations"),
            "no rotations"
        );
        assert!(
            refusal("{ rotations: 1, velocity: 0 }").contains("must be positive"),
            "speed"
        );
        assert!(
            refusal("{ rotations: 1, speed: 2 }").contains("unknown key"),
            "key"
        );
    }
}

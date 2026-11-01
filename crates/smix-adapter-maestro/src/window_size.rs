//! `setWindowSize:` — an app's desktop window resized to a size in dp.
//!
//! ```yaml
//! - setWindowSize: { width: 1024, height: 768 }
//! - assertVisible: { id: two_pane_layout }
//! - setWindowSize: 839x768
//! - setWindowSize: compact     # or medium, expanded
//! ```

use serde_norway::Value;
use smix_input::WindowSize;

use crate::{ParseError, Step};

pub(crate) fn parse(v: &Value) -> Result<Step, ParseError> {
    let bad = |reason: String| ParseError::InvalidValue {
        field: "setWindowSize".into(),
        reason,
    };
    let size = match v {
        Value::String(s) => s.parse::<WindowSize>().map_err(bad)?,
        Value::Mapping(m) => {
            for key in m.keys() {
                match key.as_str() {
                    Some("width" | "height") => {}
                    _ => return Err(bad(format!("knows width and height (in dp), not {key:?}"))),
                }
            }
            let side = |name: &str| -> Result<u32, ParseError> {
                let v = m.get(name).ok_or_else(|| {
                    bad(format!("{name} is missing; give width and height in dp"))
                })?;
                v.as_u64()
                    .and_then(|n| u32::try_from(n).ok())
                    .ok_or_else(|| bad(format!("{name} must be a whole number of dp, got {v:?}")))
            };
            let size = WindowSize {
                width_dp: side("width")?,
                height_dp: side("height")?,
            };
            size.check().map_err(bad)?;
            size
        }
        other => {
            return Err(bad(format!(
                "expected {{ width: 1024, height: 768 }} or 1024x768, in dp; got {other:?}"
            )));
        }
    };
    Ok(Step::SetWindowSize { size })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn yaml(s: &str) -> Value {
        serde_norway::from_str(s).unwrap()
    }

    fn size(w: u32, h: u32) -> Step {
        Step::SetWindowSize {
            size: WindowSize {
                width_dp: w,
                height_dp: h,
            },
        }
    }

    #[test]
    fn a_size_is_a_mapping_or_width_by_height() {
        assert_eq!(
            parse(&yaml("{ width: 1024, height: 768 }")).unwrap(),
            size(1024, 768)
        );
        assert_eq!(parse(&yaml("839x768")).unwrap(), size(839, 768));
        assert_eq!(parse(&yaml("compact")).unwrap(), size(412, 800));
    }

    #[test]
    fn a_size_without_both_sides_in_dp_is_refused() {
        let e = parse(&yaml("{ width: 1024 }")).unwrap_err().to_string();
        assert!(e.contains("height is missing"), "{e}");
        let e = parse(&yaml("{ width: 1024, height: 76.5 }"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("whole number of dp"), "{e}");
        let e = parse(&yaml("{ width: 1024, height: 768, x: 0 }"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("not String(\"x\")"), "{e}");
        let e = parse(&yaml("{ width: 0, height: 768 }"))
            .unwrap_err()
            .to_string();
        assert!(e.contains("no area"), "{e}");
        let e = parse(&yaml("[1024, 768]")).unwrap_err().to_string();
        assert!(e.contains("width: 1024"), "{e}");
    }
}

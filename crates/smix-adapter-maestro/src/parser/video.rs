//! `assertVideoPlaying` / `assertVideoFrozen: { <selector>, within? }`.

use serde_norway::Value;

use super::{get_spelled, split_bounds_mapping};
use crate::{ParseError, Step};

/// How long `assertVideoPlaying` waits for a frame that differs: long
/// enough for a stream to start.
const PLAYING_WITHIN_MS: u64 = 10_000;
/// How long `assertVideoFrozen` watches for one: a video playing at even
/// a few frames a second changes many times over in it.
const FROZEN_WITHIN_MS: u64 = 2_000;

pub(super) fn parse_assert_video(v: &Value, playing: bool) -> Result<Step, ParseError> {
    let verb = if playing {
        "assertVideoPlaying"
    } else {
        "assertVideoFrozen"
    };
    let (selector, map) = split_bounds_mapping(v, verb, &["within"])?;
    let within_ms = match get_spelled(map, "within") {
        None if playing => PLAYING_WITHIN_MS,
        None => FROZEN_WITHIN_MS,
        Some(w) => millis(w).ok_or_else(|| ParseError::InvalidValue {
            field: format!("{verb}.within"),
            reason: format!("expected milliseconds, or a number with ms or s (`10s`), got {w:?}"),
        })?,
    };
    Ok(Step::AssertVideo {
        selector,
        playing,
        within_ms,
    })
}

/// `1500`, `"1500ms"`, `"1.5s"` → 1500; nothing for zero or anything else.
fn millis(v: &Value) -> Option<u64> {
    let ms = match v {
        Value::Number(n) => n.as_f64()?,
        Value::String(s) => {
            let s = s.trim();
            if let Some(n) = s.strip_suffix("ms") {
                n.trim().parse::<f64>().ok()?
            } else {
                s.strip_suffix('s')?.trim().parse::<f64>().ok()? * 1000.0
            }
        }
        _ => return None,
    };
    (ms.is_finite() && ms >= 1.0).then(|| ms.round() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str, playing: bool) -> Result<Step, ParseError> {
        parse_assert_video(&serde_norway::from_str(yaml).expect("yaml"), playing)
    }

    fn within(step: Step) -> u64 {
        match step {
            Step::AssertVideo { within_ms, .. } => within_ms,
            other => panic!("not a video step: {other:?}"),
        }
    }

    #[test]
    fn within_takes_milliseconds_or_seconds() {
        assert_eq!(
            within(parse("{ id: tile_3, within: 2500 }", true).expect("ok")),
            2500
        );
        assert_eq!(
            within(parse("{ id: tile_3, within: 10s }", true).expect("ok")),
            10_000
        );
        assert_eq!(
            within(parse("{ id: tile_3, within: 750ms }", false).expect("ok")),
            750
        );
    }

    #[test]
    fn each_verb_has_its_own_default() {
        assert_eq!(
            within(parse("{ id: tile_3 }", true).expect("ok")),
            PLAYING_WITHIN_MS
        );
        assert_eq!(
            within(parse("{ id: tile_3 }", false).expect("ok")),
            FROZEN_WITHIN_MS
        );
    }

    #[test]
    fn a_bad_within_or_a_stray_key_is_refused() {
        assert!(parse("{ id: tile_3, within: soon }", true).is_err());
        assert!(parse("{ id: tile_3, within: 0 }", true).is_err());
        assert!(parse("{ id: tile_3, for: 2s }", false).is_err());
    }
}

//! `point` over MCP: a place, not a thing.
//!
//! The capability was in the yaml surface and four SDKs and not here, so
//! an agent driving through MCP had no coordinate fallback at all —
//! nothing to reach for on a canvas, a map, a video surface.
//!
//! Adding the field is the small half. `Selector::Point` is dispatched by
//! the caller and never resolved: the resolver's own comment reads
//! "reaching matches_base means a caller forgot to dispatch; no node
//! matches". A tool that took `point` and handed it to `App::tap` would
//! report a miss on a screen where the touch was perfectly deliverable —
//! which is the shape of gap this whole change is about, rebuilt one
//! layer down.

use smix_mcp::{SelectorParams, point_of};
use smix_sdk::Selector;

fn params(json: &str) -> SelectorParams {
    serde_json::from_str(json).expect("deserializes")
}

#[test]
fn a_point_becomes_a_point_selector() {
    let sel = params(r#"{"point":"50%,25%"}"#)
        .to_selector()
        .expect("valid point");
    assert!(matches!(sel, Selector::Point { nx, ny } if nx == 0.5 && ny == 0.25));
    assert_eq!(point_of(&sel), Some((0.5, 0.25)));
}

/// The same reading as yaml and the CLI, because it is the same function.
#[test]
fn a_fraction_is_the_same_point_as_a_percentage() {
    let a = params(r#"{"point":"50%,25%"}"#).to_selector().unwrap();
    let b = params(r#"{"point":"0.5,0.25"}"#).to_selector().unwrap();
    assert_eq!(point_of(&a), point_of(&b));
}

/// Pixels are refused here too, with the unit named — an agent that
/// writes screen coordinates gets told what the unit is, not a miss.
#[test]
fn pixels_are_refused_with_the_reason() {
    let e = params(r#"{"point":"267,100"}"#)
        .to_selector()
        .expect_err("267 is off screen");
    let msg = format!("{e:?}");
    assert!(msg.contains("fraction of the viewport"), "{msg}");
}

/// Naming a place and a thing at once is a coin flip the agent never
/// sees, so it is refused like every other pair.
#[test]
fn a_point_and_an_id_together_are_refused() {
    let e = params(r#"{"id":"submit","point":"50%,25%"}"#)
        .to_selector()
        .expect_err("two selectors");
    let msg = format!("{e:?}");
    assert!(
        msg.contains("point"),
        "the message names what was given: {msg}"
    );
}

/// And the other direction, so the pair check cannot be satisfied by
/// refusing everything: one selector still works.
#[test]
fn one_selector_on_its_own_still_works() {
    assert!(params(r#"{"id":"submit"}"#).to_selector().is_ok());
    assert!(params(r#"{"point":"0,0"}"#).to_selector().is_ok());
}

/// Everything that is not a point reads as not a point — otherwise the
/// dispatch split would send ordinary selectors down the coordinate path.
#[test]
fn only_a_point_reads_as_one() {
    for json in [
        r#"{"id":"submit"}"#,
        r#"{"text":"Save"}"#,
        r#"{"label":"Close"}"#,
        r#"{"ocrText":"Total"}"#,
    ] {
        let sel = params(json).to_selector().expect("valid");
        assert_eq!(point_of(&sel), None, "{json}");
    }
}

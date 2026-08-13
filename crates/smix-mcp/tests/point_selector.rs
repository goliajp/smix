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

/// A chain of ways to name the same thing, flattened, in order.
#[test]
fn a_chain_becomes_a_fallback_selector() {
    let sel = params(r#"{"fallback":[{"id":"submit"},{"text":"Send"}]}"#)
        .to_selector()
        .expect("valid chain");
    let layers = smix_mcp::chain_of(&sel).expect("is a chain");
    assert_eq!(layers.len(), 2);
}

/// Nesting is allowed and means what writing it flat means, so every
/// caller iterates one list instead of each rediscovering the recursion.
#[test]
fn a_nested_chain_flattens() {
    let sel = params(r#"{"fallback":[{"id":"a"},{"fallback":[{"id":"b"},{"id":"c"}]}]}"#)
        .to_selector()
        .expect("valid");
    assert_eq!(smix_mcp::chain_of(&sel).expect("chain").len(), 3);
}

/// A coordinate always hits, so a layer after one is never reached. A
/// chain with a dead tail reads as a plan and is not one.
#[test]
fn a_point_before_the_end_of_a_chain_is_refused() {
    let e = params(r#"{"fallback":[{"point":"50%,50%"},{"id":"submit"}]}"#)
        .to_selector()
        .expect_err("dead tail");
    let msg = format!("{e:?}");
    assert!(msg.contains("fallback[0]"), "names the layer: {msg}");
    assert!(msg.contains("always hits"), "says why: {msg}");
}

/// And last is where it belongs, so that case has to keep working.
#[test]
fn a_point_at_the_end_of_a_chain_is_fine() {
    assert!(
        params(r#"{"fallback":[{"id":"submit"},{"point":"50%,50%"}]}"#)
            .to_selector()
            .is_ok()
    );
}

/// An empty chain is not a way of naming anything.
#[test]
fn an_empty_chain_is_refused() {
    assert!(params(r#"{"fallback":[]}"#).to_selector().is_err());
}

/// A layer that is itself malformed says which layer.
#[test]
fn a_bad_layer_names_its_index() {
    let e = params(r#"{"fallback":[{"id":"a"},{"point":"267,100"}]}"#)
        .to_selector()
        .expect_err("pixels in layer 1");
    assert!(format!("{e:?}").contains("fallback[1]"), "{e:?}");
}

/// The recursive schema generates, and generates as a self-reference —
/// the risk this checkpoint was planned around, measured rather than
/// assumed.
#[test]
fn the_schema_is_recursive_and_generates() {
    let s = schemars::schema_for!(SelectorParams);
    let j = serde_json::to_string(&s).expect("serializes");
    assert!(j.contains("fallback"), "the field is in the schema");
    assert!(j.contains("\"$ref\""), "and refers to itself: {j}");
}

/// The languages to read in ride on the selector, so a tool cannot
/// accept the field and then drop it.
#[test]
fn locales_reach_the_selector() {
    let sel = params(r#"{"ocrText":"允许","locales":["zh-Hans"]}"#)
        .to_selector()
        .expect("valid");
    assert_eq!(smix_mcp::ocr_locales_of(&sel), ["zh-Hans".to_string()]);
}

/// Left out means the recogniser decides, which is what every caller got
/// before there was a way to say.
#[test]
fn no_locales_means_the_recogniser_decides() {
    let sel = params(r#"{"ocrText":"Allow"}"#)
        .to_selector()
        .expect("valid");
    assert!(smix_mcp::ocr_locales_of(&sel).is_empty());
}

/// `locales` narrows `ocrText` and means nothing without it — the same
/// rule `name` has for `role`.
#[test]
fn locales_without_ocr_text_is_refused() {
    let e = params(r#"{"id":"submit","locales":["ja"]}"#)
        .to_selector()
        .expect_err("locales needs ocrText");
    assert!(format!("{e:?}").contains("locales"), "{e:?}");
}

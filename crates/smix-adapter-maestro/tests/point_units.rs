//! `tapOn: { point: … }` takes a fraction of the viewport, not pixels.
//!
//! `%` is optional, so `'267,811'` reads as 267% and used to travel to
//! the runner before coming back as `outOfRange("nx", 2.67)` — a number
//! and a field name, with no mention that the units are a fraction. A
//! reader who wrote pixels had to infer that from the shape of the
//! number (EXT1, #10).

use smix_adapter_maestro::parse_flow_yaml;

fn flow(point: &str) -> Result<smix_adapter_maestro::Flow, String> {
    parse_flow_yaml(&format!(
        "appId: com.example.app\n---\n- tapOn:\n    point: '{point}'\n"
    ))
    .map_err(|e| e.to_string())
}

#[test]
fn a_fraction_and_a_percentage_both_parse() {
    for p in ["50%,80%", "0.5,0.8", "0,0", "100%,100%"] {
        assert!(flow(p).is_ok(), "{p} should parse");
    }
}

/// The reported case: pixel coordinates.
#[test]
fn pixels_are_refused_with_the_unit_named() {
    let err = flow("267,811").expect_err("267 is 267% of the viewport");
    assert!(
        err.contains("not pixels") && err.contains("50%,80%"),
        "the message has to name the unit and show the form: {err}"
    );
    assert!(
        err.contains("one screen size"),
        "and say why pixels are not simply accepted: {err}"
    );
}

/// A negative is off screen the other way.
#[test]
fn a_negative_fraction_is_refused_too() {
    assert!(flow("-10%,50%").is_err());
}

//! What `"50%,80%"` and `"0.5,0.8"` mean, and that they mean the same thing.
//!
//! They did not. `%` was stripped and the number divided by a hundred
//! either way, so the second form was a hundredth of the first — a tap
//! aimed at the middle of the screen landing eight pixels from the
//! corner. Every guide says the two forms are the same point.
//!
//! The test that covered this asked whether the string parsed. Both
//! forms parse. It never asked what they parsed to, which is the only
//! question that separates a working escape hatch from one that taps
//! somewhere else and says nothing.

use smix_selector::point_from_str;

#[test]
fn a_percentage_and_a_fraction_are_the_same_point() {
    assert_eq!(point_from_str("50%,25%"), Ok((0.5, 0.25)));
    assert_eq!(point_from_str("0.5,0.25"), Ok((0.5, 0.25)));
    assert_eq!(point_from_str("50%,25%"), point_from_str("0.5,0.25"));
}

/// `%` is the only thing that decides the reading, which is what makes a
/// bare `1` the full width rather than one percent of it.
#[test]
fn the_percent_sign_is_what_decides() {
    assert_eq!(point_from_str("1,1"), Ok((1.0, 1.0)));
    assert_eq!(point_from_str("1%,1%"), Ok((0.01, 0.01)));
    assert_eq!(point_from_str("100%,100%"), Ok((1.0, 1.0)));
    assert_eq!(point_from_str("0,0"), Ok((0.0, 0.0)));
}

/// Pixels are refused, and the refusal names the unit — a reader who
/// wrote pixels otherwise learns it from the shape of a number that
/// comes back from the wire, if at all.
#[test]
fn pixels_are_refused_and_the_reason_is_the_unit() {
    let e = point_from_str("267,100").expect_err("267 is off screen");
    assert!(e.contains("fraction of the viewport"), "{e}");
    assert!(
        e.contains("divide by the viewport's width or height"),
        "{e}"
    );
    assert!(e.contains("one screen size"), "{e}");
}

/// A shape that is not a pair says what the shape should be.
#[test]
fn a_wrong_shape_shows_the_right_one() {
    for bad in ["50%", "a,b", "", "1,2,3"] {
        let e = point_from_str(bad).expect_err("{bad} is not a point");
        assert!(
            e.contains("X%,Y%") || e.contains("not a number"),
            "{bad}: {e}"
        );
    }
}

/// Off screen the other way.
#[test]
fn a_negative_is_refused() {
    assert!(point_from_str("-0.1,0.5").is_err());
}

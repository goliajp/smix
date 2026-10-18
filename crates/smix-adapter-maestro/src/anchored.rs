//! `anchored:` — a point at an offset from an anchor's centre.
//!
//! `dx` and `dy` are fractions of the screen's width and height. The point
//! used to be clamped to the screen's edge, so an offset written in pixels
//! (`dy: 119`) or in anchor heights (`dy: 1.12`) touched the bottom edge
//! and reported success; a consumer found both touching nothing.

/// The point `(dx, dy)` from the anchor's centre, or why there is none:
/// a point off the screen is not touched at the edge instead.
pub(crate) fn anchored_point(anchor: (f64, f64), dx: f64, dy: f64) -> Result<(f64, f64), String> {
    let (nx, ny) = (anchor.0 + dx, anchor.1 + dy);
    if (0.0..=1.0).contains(&nx) && (0.0..=1.0).contains(&ny) {
        return Ok((nx, ny));
    }
    Err(format!(
        "anchored: the anchor's centre is at ({:.3}, {:.3}) of the screen, and dx {dx}, \
         dy {dy} put the point at ({nx:.3}, {ny:.3}), which is off the screen — nothing \
         was touched. dx and dy are fractions of the screen's width and height \
         (dy: 0.05 is 5% of its height below the anchor), not pixels or anchor sizes.",
        anchor.0, anchor.1
    ))
}

/// [`anchored_point`] as a step's result: a refused point fails the step.
pub(crate) fn point(
    anchor: (f64, f64),
    dx: f64,
    dy: f64,
    selector: &smix_sdk::Selector,
) -> Result<(f64, f64), crate::runtime::RunError> {
    anchored_point(anchor, dx, dy).map_err(|message| {
        crate::runtime::RunError::Sdk(smix_sdk::ExpectationFailure::new(smix_sdk::FailureInit {
            code: Some(smix_sdk::FailureCode::NotVisible),
            message,
            selector: Some(selector.clone()),
            ..Default::default()
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_offset_is_a_fraction_of_the_screen() {
        let (nx, ny) = anchored_point((0.5, 0.42), 0.0, 0.05).unwrap();
        assert!((nx - 0.5).abs() < 1e-9 && (ny - 0.47).abs() < 1e-9);
    }

    #[test]
    fn the_edges_are_on_the_screen() {
        assert_eq!(anchored_point((0.5, 0.5), 0.5, -0.5), Ok((1.0, 0.0)));
    }

    // The consumer's two tries: anchor heights and pixels.
    #[test]
    fn a_point_off_the_screen_is_refused_not_clamped() {
        for dy in [1.12, 119.0] {
            let e = anchored_point((0.5, 0.42), 0.0, dy).unwrap_err();
            assert!(
                e.contains("off the screen") && e.contains("nothing was touched"),
                "{e}"
            );
        }
        assert!(anchored_point((0.1, 0.5), -0.2, 0.0).is_err());
    }
}

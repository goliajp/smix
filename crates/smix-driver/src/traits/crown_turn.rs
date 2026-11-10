//! What a turn of the Digital Crown has to be before it is sent.

/// Why `rotations` turns at `velocity` turns a second is not a turn, or
/// `Ok` when it is: a non-zero, finite number of turns, and a speed that is
/// positive when one is given.
///
/// # Errors
///
/// The sentence to show the person who asked for it.
pub fn crown_turn_check(rotations: f64, velocity: Option<f64>) -> Result<(), String> {
    if !rotations.is_finite() || rotations == 0.0 {
        return Err(format!(
            "{rotations} is not a turn; give a non-zero number of turns (1 is a full turn, \
             negative turns back)"
        ));
    }
    match velocity {
        Some(v) if !(v.is_finite() && v > 0.0) => Err(format!(
            "a speed of {v} turns a second is not one; give a positive number"
        )),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_turn_needs_a_size_and_a_positive_speed() {
        assert!(crown_turn_check(-0.5, None).is_ok());
        assert!(crown_turn_check(2.0, Some(0.5)).is_ok());
        assert!(
            crown_turn_check(0.0, None)
                .unwrap_err()
                .contains("not a turn")
        );
        assert!(
            crown_turn_check(f64::NAN, None)
                .unwrap_err()
                .contains("not a turn")
        );
        assert!(
            crown_turn_check(1.0, Some(0.0))
                .unwrap_err()
                .contains("positive")
        );
        assert!(
            crown_turn_check(1.0, Some(-1.0))
                .unwrap_err()
                .contains("positive")
        );
    }
}

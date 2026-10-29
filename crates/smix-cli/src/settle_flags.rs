//! `smix run --settle-max-ms` / `--settle-frames`.

use std::time::Duration;

use smix_driver::settle::Settle;

/// The settle the two flags ask for, or `None` when neither was given;
/// the one not given keeps its default.
pub fn settle_from_flags(max_ms: Option<u64>, frames: Option<u32>) -> Option<Settle> {
    if max_ms.is_none() && frames.is_none() {
        return None;
    }
    let default = Settle::default();
    Some(Settle::new(
        max_ms.map_or(default.limit, Duration::from_millis),
        frames.unwrap_or(default.readings),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_flag_leaves_the_driver_default() {
        assert_eq!(settle_from_flags(None, None), None);
    }

    #[test]
    fn one_flag_keeps_the_other_default() {
        let d = Settle::default();
        assert_eq!(
            settle_from_flags(Some(300), None),
            Some(Settle::new(Duration::from_millis(300), d.readings))
        );
        assert_eq!(
            settle_from_flags(None, Some(3)),
            Some(Settle::new(d.limit, 3))
        );
    }
}

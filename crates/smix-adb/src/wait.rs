//! When a wait that polls has run out, judged by when a look began.
//!
//! A wait that ends on "not yet" makes a claim about the whole of its
//! time, and the claim rests on its latest look. Judging the clock before a
//! look skips the look that would have seen the change; judging it after
//! lets a look that began in time, and came back late, end the wait on a
//! state that had already changed. On a loaded device one look has taken
//! seconds. The wait is over only once a look that *began* after the
//! deadline still says "not yet".

use std::time::{Duration, Instant};

/// The end of a wait.
#[derive(Debug, Clone, Copy)]
pub struct Deadline(Instant);

/// When one look began.
#[derive(Debug, Clone, Copy)]
pub struct Look(Instant);

impl Deadline {
    /// `within` from now.
    #[must_use]
    pub fn after(within: Duration) -> Self {
        Self(Instant::now() + within)
    }

    /// `within` from `start`.
    #[must_use]
    pub fn from(start: Instant, within: Duration) -> Self {
        Self(start + within)
    }

    /// Mark a look beginning now. Take it before looking, not after.
    #[must_use]
    pub fn look(&self) -> Look {
        Look(Instant::now())
    }

    /// Whether a "not yet" from `look` ends the wait: it does when the
    /// look began at or after the deadline.
    #[must_use]
    pub fn passed_by(&self, look: Look) -> bool {
        look.0 >= self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_begun_in_time_does_not_end_the_wait_however_late_it_returns() {
        let d = Deadline::after(Duration::from_millis(30));
        let look = d.look();
        std::thread::sleep(Duration::from_millis(60));
        assert!(
            !d.passed_by(look),
            "a look that began before the deadline ended the wait"
        );
    }

    #[test]
    fn a_look_begun_after_the_deadline_ends_it() {
        let d = Deadline::after(Duration::from_millis(10));
        std::thread::sleep(Duration::from_millis(20));
        assert!(d.passed_by(d.look()));
    }

    #[test]
    fn a_wait_of_nothing_is_over_at_the_first_look() {
        let d = Deadline::after(Duration::ZERO);
        assert!(d.passed_by(d.look()));
    }
}

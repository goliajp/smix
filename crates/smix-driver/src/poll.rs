//! When a wait that polls has run out.
//!
//! A wait that ends on "not yet" makes a claim about the whole of its
//! budget, and the claim rests on its latest look. Checking the clock
//! after a look lets that look be one that began before the budget was
//! over: on a loaded device a single read of the screen has taken two
//! seconds, so a look begun early reads the old screen, finishes after
//! the deadline, and the wait gives up on a state that had already
//! changed. The budget is spent only once a look that *began* after it
//! still says "not yet".

use std::time::Duration;
// tokio's clock, so a test that pauses time moves the budget with it; outside
// a paused runtime it is the monotonic clock.
use tokio::time::Instant;

/// A wait's budget, measured from when the wait began.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    start: Instant,
    limit: Duration,
}

/// When one look began.
#[derive(Debug, Clone, Copy)]
pub struct Look(Instant);

impl Budget {
    /// A budget of `limit`, starting now.
    #[must_use]
    pub fn new(limit: Duration) -> Self {
        Self::starting_at(Instant::now(), limit)
    }

    /// A budget of `limit` that began at `start`.
    #[must_use]
    pub fn starting_at(start: Instant, limit: Duration) -> Self {
        Self { start, limit }
    }

    /// Mark a look beginning now. Take it before reading, not after.
    #[must_use]
    pub fn look(&self) -> Look {
        Look(Instant::now())
    }

    /// Whether a "not yet" from `look` ends the wait: it does when the
    /// look began once the budget was over.
    #[must_use]
    pub fn spent_by(&self, look: Look) -> bool {
        look.0.saturating_duration_since(self.start) >= self.limit
    }

    /// The budget's length.
    #[must_use]
    pub fn limit(&self) -> Duration {
        self.limit
    }
}

impl Look {
    /// A look that began at `at`.
    #[must_use]
    pub fn at(at: Instant) -> Self {
        Self(at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_look_begun_before_the_limit_does_not_end_the_wait_however_late_it_returns() {
        let start = Instant::now();
        let budget = Budget::starting_at(start, Duration::from_millis(2000));
        // Begun at 60 ms; that it came back at 2100 ms is not its business.
        assert!(!budget.spent_by(Look::at(start + Duration::from_millis(60))));
        assert!(!budget.spent_by(Look::at(start + Duration::from_millis(1999))));
    }

    #[test]
    fn a_look_begun_at_or_after_the_limit_ends_it() {
        let start = Instant::now();
        let budget = Budget::starting_at(start, Duration::from_millis(2000));
        assert!(budget.spent_by(Look::at(start + Duration::from_millis(2000))));
        assert!(budget.spent_by(Look::at(start + Duration::from_millis(2110))));
    }

    #[test]
    fn a_zero_budget_is_spent_by_the_first_look() {
        let budget = Budget::new(Duration::ZERO);
        assert!(budget.spent_by(budget.look()));
    }
}

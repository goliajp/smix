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

    /// How long since the budget began.
    #[must_use]
    pub fn elapsed(&self) -> Duration {
        self.start.elapsed()
    }

    /// The budget's length.
    #[must_use]
    pub fn limit(&self) -> Duration {
        self.limit
    }

    /// How long to wait before the next look; see [`pause_after`].
    #[must_use]
    pub fn pause(&self) -> Duration {
        pause_after(self.elapsed())
    }
}

/// Looks this far apart while a wait is young.
pub const EARLY_PAUSE: Duration = Duration::from_millis(100);
/// Looks this far apart once it is not.
pub const LATE_PAUSE: Duration = Duration::from_millis(300);
/// How long a wait counts as young.
pub const EARLY_FOR: Duration = Duration::from_secs(1);

/// The pause between looks in a wait that has run for `elapsed`.
///
/// Most waits end on their first or second look: in one consumer's
/// 50-minute run, 1494 of 1598 waits took three looks or fewer. With a
/// flat 250 ms, every one of those that missed once paid up to a quarter
/// of a second for nothing. A wait still going after a second is waiting
/// on the app, and looking more often then only adds reads — an iOS read
/// of the screen is an accessibility snapshot taken inside the app.
#[must_use]
pub fn pause_after(elapsed: Duration) -> Duration {
    if elapsed < EARLY_FOR {
        EARLY_PAUSE
    } else {
        LATE_PAUSE
    }
}

impl Look {
    /// A look that began at `at`.
    #[must_use]
    pub fn at(at: Instant) -> Self {
        Self(at)
    }
}

/// How long the screen has looked the same across a wait's looks.
///
/// A wait for something that is not coming spends its whole budget —
/// 30 to 120 s in one consumer's flows — on a screen that stopped
/// changing at the start. With a limit, the wait can give up once the
/// screen has been the same for that long. It trades a slow failure for
/// a wrong one when the app is busy behind a screen that does not move
/// (waiting on a server, say), so it is off unless asked for.
#[derive(Debug)]
pub struct Stillness {
    limit: Option<Duration>,
    since: Option<(u64, Instant)>,
}

impl Stillness {
    #[must_use]
    pub fn new(limit: Option<Duration>) -> Self {
        Self { limit, since: None }
    }

    /// Record a look at a screen with `fingerprint`; whether the screen
    /// has now been the same for the whole limit.
    pub fn still_for_the_limit(&mut self, fingerprint: u64) -> bool {
        let Some(limit) = self.limit else {
            return false;
        };
        match self.since {
            Some((seen, at)) if seen == fingerprint => at.elapsed() >= limit,
            _ => {
                self.since = Some((fingerprint, Instant::now()));
                false
            }
        }
    }

    /// The limit, when there is one.
    #[must_use]
    pub fn limit(&self) -> Option<Duration> {
        self.limit
    }
}

/// The failure message for a wait that gave up on a still screen.
#[must_use]
pub fn gave_up_still(target: &str, waited: Duration, limit: Option<Duration>) -> String {
    format!(
        "waitFor({target}) gave up after {} ms: the screen had not changed for {} ms and \
         {target} was not on it (--give-up-when-still-ms; without it the wait runs its whole \
         timeout)",
        waited.as_millis(),
        limit.unwrap_or_default().as_millis()
    )
}

/// A screen's fingerprint: equal for two readings that serialize alike.
#[must_use]
pub fn fingerprint(screen: &smix_screen::A11yNode) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_vec(screen).unwrap_or_default().hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    #[tokio::test(start_paused = true)]
    async fn a_screen_still_for_the_limit_is_said_to_be_still() {
        let mut s = Stillness::new(Some(Duration::from_secs(5)));
        assert!(!s.still_for_the_limit(1));
        tokio::time::advance(Duration::from_secs(4)).await;
        assert!(!s.still_for_the_limit(1));
        tokio::time::advance(Duration::from_secs(1)).await;
        assert!(s.still_for_the_limit(1));
    }

    #[tokio::test(start_paused = true)]
    async fn a_change_starts_the_count_again() {
        let mut s = Stillness::new(Some(Duration::from_secs(5)));
        assert!(!s.still_for_the_limit(1));
        tokio::time::advance(Duration::from_secs(4)).await;
        assert!(!s.still_for_the_limit(2));
        tokio::time::advance(Duration::from_secs(4)).await;
        assert!(!s.still_for_the_limit(2));
    }

    #[tokio::test(start_paused = true)]
    async fn without_a_limit_it_never_gives_up() {
        let mut s = Stillness::new(None);
        s.still_for_the_limit(1);
        tokio::time::advance(Duration::from_secs(600)).await;
        assert!(!s.still_for_the_limit(1));
    }

    #[test]
    fn equal_screens_share_a_fingerprint_and_different_ones_do_not() {
        let a: smix_screen::A11yNode = serde_json::from_str(
            r#"{"bounds":{"x":0,"y":0,"w":390,"h":844},"rawType":"application","enabled":true,"selected":false,"hasFocus":false,"visible":true}"#,
        )
        .expect("a tree");
        let mut b = a.clone();
        assert_eq!(fingerprint(&a), fingerprint(&b));
        b.label = Some("Loading".into());
        assert_ne!(fingerprint(&a), fingerprint(&b));
    }
    use super::*;

    #[test]
    fn a_young_wait_looks_often_and_an_old_one_less() {
        assert_eq!(pause_after(Duration::ZERO), EARLY_PAUSE);
        assert_eq!(pause_after(Duration::from_millis(999)), EARLY_PAUSE);
        assert_eq!(pause_after(Duration::from_secs(1)), LATE_PAUSE);
        assert_eq!(pause_after(Duration::from_secs(90)), LATE_PAUSE);
        assert!(EARLY_PAUSE < LATE_PAUSE);
    }

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

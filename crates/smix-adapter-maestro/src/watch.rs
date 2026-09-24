//! The bookkeeping behind `neverVisible`: what a watch saw, and how
//! densely it looked.
//!
//! Pure. The runtime feeds it one event per attempt to look, stamped
//! with the time since the span began; the verdict is a function of
//! those events and of when the span ended. Kept apart from the loop
//! that asks the device so the arithmetic can be checked with a list of
//! numbers rather than a phone.

use std::time::Duration;

/// Where the watch was when it first saw the element.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Sighting {
    /// Time since the span began.
    pub at: Duration,
    /// 1-based index of the inner step that was running.
    pub step: usize,
    /// That step's verb.
    pub verb: String,
    /// Looks that had come back before this one.
    pub looks_before: u32,
}

/// What a watch knows at the end of its span.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Verdict {
    /// The element was on screen at `Sighting::at`.
    Seen(Sighting),
    /// Not one attempt to look came back with an answer. Nothing is
    /// known about the span, and saying "it never appeared" would be a
    /// claim about a screen nobody read.
    NeverLooked {
        /// How many attempts there were.
        attempts: u32,
        /// What the last one said.
        last_error: String,
    },
    /// Looked, and never saw it.
    Clear {
        /// Answers that came back.
        looks: u32,
        /// Attempts that did not.
        failed: u32,
        /// The longest stretch of the span nobody was looking at —
        /// including before the first look and after the last, because
        /// a flash there is just as unseen.
        longest_gap: Duration,
        /// How long the span was.
        span: Duration,
    },
}

/// A watch in progress.
#[derive(Debug, Default)]
pub(crate) struct Watch {
    looks: u32,
    failed: u32,
    last_look: Option<Duration>,
    longest_gap: Duration,
    seen: Option<Sighting>,
    last_error: Option<String>,
}

impl Watch {
    /// One look came back. `step` is the inner step that was running.
    pub fn looked(&mut self, at: Duration, visible: bool, step: usize, verb: &str) {
        let since = self.last_look.unwrap_or(Duration::ZERO);
        self.longest_gap = self.longest_gap.max(at.saturating_sub(since));
        self.last_look = Some(at);
        if visible && self.seen.is_none() {
            self.seen = Some(Sighting {
                at,
                step,
                verb: verb.to_string(),
                looks_before: self.looks,
            });
        }
        self.looks += 1;
    }

    /// One attempt to look did not come back with an answer.
    pub fn could_not_look(&mut self, why: String) {
        self.failed += 1;
        self.last_error = Some(why);
    }

    /// Whether the element has been seen. Once it has, the verdict is
    /// settled and further looks change nothing.
    pub fn has_seen(&self) -> bool {
        self.seen.is_some()
    }

    /// The verdict, given when the span ended.
    pub fn verdict(&self, span: Duration) -> Verdict {
        if let Some(s) = &self.seen {
            return Verdict::Seen(s.clone());
        }
        if self.looks == 0 {
            return Verdict::NeverLooked {
                attempts: self.failed,
                last_error: self.last_error.clone().unwrap_or_default(),
            };
        }
        let tail = span.saturating_sub(self.last_look.unwrap_or(Duration::ZERO));
        Verdict::Clear {
            looks: self.looks,
            failed: self.failed,
            longest_gap: self.longest_gap.max(tail),
            span,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_look_that_sees_it_settles_the_verdict_with_the_step_that_was_running() {
        let mut w = Watch::default();
        w.looked(ms(40), false, 1, "tapOn");
        w.looked(ms(90), true, 2, "extendedWaitUntil");
        w.looked(ms(140), false, 2, "extendedWaitUntil");
        match w.verdict(ms(300)) {
            Verdict::Seen(s) => {
                assert_eq!(s.at, ms(90));
                assert_eq!(s.step, 2);
                assert_eq!(s.verb, "extendedWaitUntil");
                assert_eq!(s.looks_before, 1);
            }
            other => panic!("expected Seen, got {other:?}"),
        }
    }

    #[test]
    fn the_longest_gap_counts_the_stretch_before_the_first_look_and_after_the_last() {
        let mut w = Watch::default();
        w.looked(ms(120), false, 1, "tapOn"); // 120 unwatched before it
        w.looked(ms(160), false, 1, "tapOn"); // 40
        w.looked(ms(200), false, 1, "tapOn"); // 40
        match w.verdict(ms(450)) {
            // 250 unwatched after the last look.
            Verdict::Clear {
                looks,
                longest_gap,
                span,
                ..
            } => {
                assert_eq!(looks, 3);
                assert_eq!(longest_gap, ms(250));
                assert_eq!(span, ms(450));
            }
            other => panic!("expected Clear, got {other:?}"),
        }
    }

    #[test]
    fn a_watch_whose_every_look_failed_knows_nothing() {
        let mut w = Watch::default();
        w.could_not_look("runner gone".into());
        w.could_not_look("runner still gone".into());
        assert_eq!(
            w.verdict(ms(100)),
            Verdict::NeverLooked {
                attempts: 2,
                last_error: "runner still gone".into()
            }
        );
    }

    #[test]
    fn a_failed_look_does_not_count_as_a_look() {
        let mut w = Watch::default();
        w.could_not_look("blip".into());
        w.looked(ms(50), false, 1, "tapOn");
        match w.verdict(ms(60)) {
            Verdict::Clear { looks, failed, .. } => {
                assert_eq!(looks, 1);
                assert_eq!(failed, 1);
            }
            other => panic!("expected Clear, got {other:?}"),
        }
    }
}

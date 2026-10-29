//! Where a step's time goes, by named phase.
//!
//! A step's wall time says how long it took and nothing about why: a tap
//! that spent two seconds waiting for its target to hold still and one
//! that spent two seconds in the touch look the same. The driver marks
//! the parts it knows — finding the target, waiting for it to settle,
//! sending the touch — and whoever runs the step collects them with
//! [`record`]. Outside a [`record`] the marks are not kept.

use std::cell::RefCell;
use std::future::Future;
use std::time::Instant;

/// One named part of a step: when it began, counted from the start of the
/// step, and how long it took.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct Phase {
    /// What the time was spent on: `find`, `settle`, `touch`, `launch`, ….
    pub name: &'static str,
    /// Milliseconds from the start of the recording to the start of this phase.
    pub start_ms: u64,
    /// How long the phase took, in milliseconds.
    pub ms: u64,
}

struct Log {
    began: Instant,
    phases: Vec<Phase>,
}

tokio::task_local! {
    static LOG: RefCell<Log>;
}

/// Run `f`, and return what it produced with the phases marked while it
/// ran, in the order they ended. A `record` inside another keeps its own:
/// a sub-step's phases are not its parent's.
pub async fn record<F: Future>(f: F) -> (F::Output, Vec<Phase>) {
    let log = RefCell::new(Log {
        began: Instant::now(),
        phases: Vec::new(),
    });
    LOG.scope(log, async {
        let out = f.await;
        let phases = LOG.with(|l| std::mem::take(&mut l.borrow_mut().phases));
        (out, phases)
    })
    .await
}

/// Run `f` as the phase `name`.
pub async fn time<F: Future>(name: &'static str, f: F) -> F::Output {
    let since = Instant::now();
    let out = f.await;
    mark(name, since);
    out
}

/// Mark the phase `name` as having run from `since` until now.
pub fn mark(name: &'static str, since: Instant) {
    let _ = LOG.try_with(|l| {
        let mut l = l.borrow_mut();
        let start_ms = since.saturating_duration_since(l.began).as_millis() as u64;
        l.phases.push(Phase {
            name,
            start_ms,
            ms: since.elapsed().as_millis() as u64,
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test(start_paused = true)]
    async fn phases_come_back_in_order_with_their_offsets() {
        let ((), phases) = record(async {
            time("find", tokio::time::sleep(Duration::from_millis(30))).await;
            time("touch", tokio::time::sleep(Duration::from_millis(50))).await;
        })
        .await;
        let named: Vec<_> = phases.iter().map(|p| p.name).collect();
        assert_eq!(named, ["find", "touch"]);
        assert!(phases[1].start_ms >= phases[0].start_ms + phases[0].ms);
    }

    #[tokio::test]
    async fn a_mark_outside_a_recording_is_not_kept_and_does_not_fail() {
        mark("find", Instant::now());
        let ((), phases) = record(async {}).await;
        assert!(phases.is_empty());
    }

    #[tokio::test]
    async fn an_inner_recording_keeps_its_own_phases() {
        let (inner, outer) = record(async {
            time("launch", async {}).await;
            record(async { time("touch", async {}).await }).await.1
        })
        .await;
        assert_eq!(outer.iter().map(|p| p.name).collect::<Vec<_>>(), ["launch"]);
        assert_eq!(inner.iter().map(|p| p.name).collect::<Vec<_>>(), ["touch"]);
    }
}

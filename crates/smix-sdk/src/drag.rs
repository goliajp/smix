//! A press held, moved and lifted as one touch, between two places named
//! the way a tap names one.

use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use smix_input::Drag;
use smix_selector::Selector;

use crate::{App, now_ms};

fn refused(message: String) -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message,
        ..Default::default()
    })
}

impl App {
    /// Where `target` is on the screen, as shares of it
    /// ([`smix_driver::locate`]).
    ///
    /// # Errors
    ///
    /// `ElementNotFound`, with what is on screen, when nothing matches.
    pub async fn locate(&self, target: &Selector) -> Result<(f64, f64), ExpectationFailure> {
        smix_driver::locate(self.driving()?, target).await
    }

    /// Press at `drag.from`, hold, move to `drag.to`, rest, lift.
    ///
    /// Checked here first, in the caller's terms, so a bad timing is not
    /// a runner's 400.
    ///
    /// # Errors
    ///
    /// The check's reason, or the runner's failure to dispatch the touch.
    pub async fn drag(&self, drag: &Drag) -> Result<(), ExpectationFailure> {
        drag.check()
            .map_err(|why| refused(format!("drag: {why}")))?;
        self.ledger
            .record_swipe_at_coord(now_ms(), drag.from, drag.to);
        self.driving()?.drag(drag).await
    }

    /// [`Self::drag`] between two places named like a tap's target. Holds,
    /// moves and rests for the given milliseconds; returns the drag that
    /// was made, its ends resolved.
    ///
    /// # Errors
    ///
    /// Either end not found ([`Self::locate`]), or the drag's own failure.
    pub async fn drag_between(
        &self,
        from: &Selector,
        to: &Selector,
        hold_ms: u64,
        duration_ms: u64,
        release_ms: u64,
    ) -> Result<Drag, ExpectationFailure> {
        let drag = Drag::new(self.locate(from).await?, self.locate(to).await?)
            .with_hold_ms(hold_ms)
            .with_duration_ms(duration_ms)
            .with_release_ms(release_ms);
        self.drag(&drag).await?;
        Ok(drag)
    }
}

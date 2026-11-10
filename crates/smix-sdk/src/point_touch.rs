//! Touches aimed at a place rather than an element.

use smix_driver::ScreenAtTouch;
use smix_error::ExpectationFailure;

use crate::{App, now_ms};

impl App {
    /// Tap at normalized (nx, ny) coordinates — escape hatch for
    /// coord-based maestro yaml port and other no-a11y-semantic
    /// scenarios. (nx, ny) MUST be in [0, 1] (normalized to viewport).
    ///
    /// **Escape hatch**: the Selector surface still forbids xpath/coord
    /// — this method is NOT a Selector, it is the direct
    /// Apple-native-event-chain wire entry. Only `tap` is exposed;
    /// `fill_at_coord` / `anchor_at_coord` are intentionally NOT
    /// provided.
    ///
    /// Prefer `tap(&selector)` for any path with a11y semantic. Use this
    /// only for yaml-port edge cases (e.g. maestro `point: "X%,Y%"`).
    /// It waits for the screen to stop moving first, and the answer says
    /// what the screen was doing when the touch went.
    pub async fn tap_at_coord(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<ScreenAtTouch, ExpectationFailure> {
        self.ledger.record_tap_at_coord(now_ms(), nx, ny);
        self.driving()?.tap_at_norm_coord(nx, ny).await
    }

    /// Double-tap at a viewport-normalized coordinate.
    ///
    /// The counterpart of [`Self::tap_at_coord`] for the verbs that
    /// reach a place rather than an element: OCR returns a box and an
    /// anchor plus a shift returns a point, and neither is a node to
    /// hand to `double_tap`.
    pub async fn double_tap_at_coord(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<ScreenAtTouch, ExpectationFailure> {
        self.ledger.record_tap_at_coord(now_ms(), nx, ny);
        self.driving()?.double_tap_at_norm_coord(nx, ny).await
    }

    /// Long-press at a viewport-normalized coordinate. See
    /// [`Self::double_tap_at_coord`].
    pub async fn long_press_at_coord(
        &self,
        nx: f64,
        ny: f64,
        duration_ms: u64,
    ) -> Result<ScreenAtTouch, ExpectationFailure> {
        self.ledger.record_tap_at_coord(now_ms(), nx, ny);
        self.driving()?
            .long_press_at_norm_coord(nx, ny, duration_ms)
            .await
    }
}

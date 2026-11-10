//! iOS touches aimed at a point rather than at an element.
//!
//! `(nx, ny)` is normalised to the app's frame, `[0, 1]` on each axis.
//! These bypass selector resolution, but not the wait: each first waits
//! for the screen to stop moving (see [`crate::point_settle`]).

use smix_error::ExpectationFailure;

use crate::{IosDriver, ScreenAtTouch, point_settle, settle, transport_to_failure};

impl IosDriver {
    async fn before_point_touch(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<ScreenAtTouch, ExpectationFailure> {
        crate::phases::time(
            "settle",
            point_settle::before_point_touch(
                nx,
                ny,
                || self.tree_with_retry(None),
                || async { self.runner.screenshot().await.ok() },
                settle::POLL,
                self.settle,
            ),
        )
        .await
    }

    pub async fn double_tap_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<ScreenAtTouch, ExpectationFailure> {
        let at = self.before_point_touch(nx, ny).await?;
        self.runner
            .tap_at_norm_coord_burst(nx, ny, 2, None, None)
            .await
            .map_err(transport_to_failure)?;
        Ok(at)
    }

    pub async fn long_press_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
        duration_ms: u64,
    ) -> Result<ScreenAtTouch, ExpectationFailure> {
        let hold_ms = u32::try_from(duration_ms).unwrap_or(u32::MAX);
        let at = self.before_point_touch(nx, ny).await?;
        self.runner
            .tap_at_norm_coord_burst(nx, ny, 1, None, Some(hold_ms))
            .await
            .map_err(transport_to_failure)?;
        Ok(at)
    }

    pub async fn tap_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<ScreenAtTouch, ExpectationFailure> {
        let at = self.before_point_touch(nx, ny).await?;
        self.runner
            .tap_at_norm_coord(nx, ny)
            .await
            .map_err(transport_to_failure)?;
        Ok(at)
    }
}

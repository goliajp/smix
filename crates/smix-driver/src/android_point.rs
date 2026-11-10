//! Android touches aimed at a point rather than at an element.
//!
//! Each first waits for the screen to stop moving, as the iOS ones do
//! (see [`crate::point_settle`]): a point touched while an activity or a
//! Compose screen is still coming in lands on whatever is passing under
//! it.

use smix_error::ExpectationFailure;

use crate::android::runner_failure;
use crate::{AndroidDriver, ScreenAtTouch, point_settle, settle};

impl AndroidDriver {
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
                || async { self.perceive(None).await.map(|t| t.root) },
                || async { self.runner().screenshot().await.ok() },
                settle::POLL,
                self.settle(),
            ),
        )
        .await
    }

    pub(crate) async fn tap_at_point(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<ScreenAtTouch, ExpectationFailure> {
        let at = self.before_point_touch(nx, ny).await?;
        self.runner()
            .tap_at_norm_coord(nx, ny)
            .await
            .map(|_| at)
            .map_err(|e| runner_failure("tap_at_norm_coord", e))
    }

    pub(crate) async fn double_tap_at_point(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<ScreenAtTouch, ExpectationFailure> {
        let at = self.before_point_touch(nx, ny).await?;
        self.runner()
            .double_tap_at_norm_coord(nx, ny)
            .await
            .map(|_| at)
            .map_err(|e| runner_failure("double_tap_at_norm_coord", e))
    }

    pub(crate) async fn long_press_at_point(
        &self,
        nx: f64,
        ny: f64,
        duration_ms: u64,
    ) -> Result<ScreenAtTouch, ExpectationFailure> {
        let at = self.before_point_touch(nx, ny).await?;
        self.runner()
            .long_press_at_norm_coord(nx, ny, duration_ms)
            .await
            .map(|_| at)
            .map_err(|e| runner_failure("long_press_at_norm_coord", e))
    }
}

//! The touch routes: a tap, a burst, a double tap, a long press — each
//! answered with what the touch was delivered to.

use serde::{Deserialize, Serialize};
use smix_runner_wire::{TapAtCoordResult, TreeReader};

use crate::{HttpRunnerClient, Landed, OkEnvelope, RunnerTransportError, is_one};

impl HttpRunnerClient {
    /// `POST /tap-at-norm-coord` — Apple native UI event coord tap.
    /// `POST /tap-at-norm-coord` — the default tap path.
    ///
    /// Returns what the point turned out to be inside. It used to
    /// return nothing at all, so a caller could report "tapped" having
    /// checked only that a touch was synthesised somewhere.
    ///
    /// A runner older than the `chain` field answers without it and
    /// deserializes to an empty chain — indistinguishable on the wire
    /// from a point that landed outside everything, which is why the
    /// host treats an empty chain as its own verdict rather than as a
    /// pass.
    pub async fn tap_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<TapAtCoordResult, RunnerTransportError> {
        self.tap_at_norm_coord_burst(nx, ny, 1, None, None).await
    }

    /// `POST /tap-at-norm-coord` with several touches at one point.
    ///
    /// One request, `times` touches spaced by `interval_ms`, timed by
    /// the runner. Sending them one at a time costs a round trip each —
    /// measured at ~400 ms on iOS 26.5 — and leaves the spacing as
    /// whatever that round trip happened to be, which is why a gesture
    /// gated on a 500 ms inter-tap window could not be driven: a flow
    /// could not tell a slow harness from a broken app. On iOS a touch
    /// cannot start before the one before it has been delivered, about
    /// 280 ms, so a shorter interval arrives as that.
    ///
    /// `None` for either timing takes the runner's default.
    pub async fn tap_at_norm_coord_burst(
        &self,
        nx: f64,
        ny: f64,
        times: u32,
        interval_ms: Option<u32>,
        hold_ms: Option<u32>,
    ) -> Result<TapAtCoordResult, RunnerTransportError> {
        self.tap_at_norm_coord_aimed(nx, ny, times, interval_ms, hold_ms, None)
            .await
    }

    /// [`Self::tap_at_norm_coord_burst`], saying which tree the point was
    /// aimed from, so the runner reads what the touch landed on from that
    /// tree and says which it read. `None` asks for nothing, as a point
    /// that no selector aimed has no reader to name.
    pub async fn tap_at_norm_coord_aimed(
        &self,
        nx: f64,
        ny: f64,
        times: u32,
        interval_ms: Option<u32>,
        hold_ms: Option<u32>,
        aimed_by: Option<TreeReader>,
    ) -> Result<TapAtCoordResult, RunnerTransportError> {
        self.tap_request(nx, ny, times, interval_ms, hold_ms, aimed_by, false)
            .await
    }

    /// A double tap on the iOS runner's tap route: two touches in one
    /// synthesise, close enough together for a double-tap recogniser,
    /// which two separate taps are not.
    pub async fn double_tap_gesture_aimed(
        &self,
        nx: f64,
        ny: f64,
        aimed_by: Option<TreeReader>,
    ) -> Result<TapAtCoordResult, RunnerTransportError> {
        self.tap_request(nx, ny, 2, None, None, aimed_by, true)
            .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn tap_request(
        &self,
        nx: f64,
        ny: f64,
        times: u32,
        interval_ms: Option<u32>,
        hold_ms: Option<u32>,
        aimed_by: Option<TreeReader>,
        double_tap: bool,
    ) -> Result<TapAtCoordResult, RunnerTransportError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Req {
            nx: f64,
            ny: f64,
            #[serde(skip_serializing_if = "Option::is_none")]
            aimed_by: Option<TreeReader>,
            // Omitted for an ordinary tap, so the common case puts
            // exactly the bytes on the wire it always did — including
            // for a runner that has never heard of a burst.
            #[serde(skip_serializing_if = "is_one")]
            times: u32,
            #[serde(skip_serializing_if = "Option::is_none")]
            interval_ms: Option<u32>,
            #[serde(skip_serializing_if = "Option::is_none")]
            hold_ms: Option<u32>,
            #[serde(skip_serializing_if = "std::ops::Not::not")]
            double_tap: bool,
        }
        #[derive(Deserialize)]
        struct Resp {
            #[serde(default)]
            ok: Option<bool>,
            #[serde(flatten)]
            result: TapAtCoordResult,
        }
        let body: Resp = self
            .json_post(
                "/tap-at-norm-coord",
                &Req {
                    nx,
                    ny,
                    aimed_by,
                    times,
                    interval_ms,
                    hold_ms,
                    double_tap,
                },
                None,
            )
            .await?;
        OkEnvelope {
            ok: body.ok,
            error: None,
            saw: None,
        }
        .require_ok("/tap-at-norm-coord")?;
        Ok(body.result)
    }

    /// `POST /double-tap-at-norm-coord` — double-tap at
    /// viewport-normalized coord. Android runner only, backing
    /// `AndroidDriver::double_tap` after host-resolve; the iOS runner
    /// serves the same gesture as a two-touch burst on
    /// `/tap-at-norm-coord`.
    pub async fn double_tap_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
    ) -> Result<TapAtCoordResult, RunnerTransportError> {
        self.double_tap_at_norm_coord_aimed(nx, ny, None).await
    }

    /// [`Self::double_tap_at_norm_coord`], naming the tree the point was
    /// aimed from; see [`Self::tap_at_norm_coord_aimed`].
    pub async fn double_tap_at_norm_coord_aimed(
        &self,
        nx: f64,
        ny: f64,
        aimed_by: Option<TreeReader>,
    ) -> Result<TapAtCoordResult, RunnerTransportError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Req {
            nx: f64,
            ny: f64,
            #[serde(skip_serializing_if = "Option::is_none")]
            aimed_by: Option<TreeReader>,
        }
        let body: Landed = self
            .json_post("/double-tap-at-norm-coord", &Req { nx, ny, aimed_by }, None)
            .await?;
        body.into_result("/double-tap-at-norm-coord")
    }

    /// `POST /long-press-at-norm-coord` — long-press at coord
    /// with explicit duration. Android-specific.
    pub async fn long_press_at_norm_coord(
        &self,
        nx: f64,
        ny: f64,
        duration_ms: u64,
    ) -> Result<TapAtCoordResult, RunnerTransportError> {
        self.long_press_at_norm_coord_aimed(nx, ny, duration_ms, None)
            .await
    }

    /// [`Self::long_press_at_norm_coord`], naming the tree the point was
    /// aimed from; see [`Self::tap_at_norm_coord_aimed`].
    pub async fn long_press_at_norm_coord_aimed(
        &self,
        nx: f64,
        ny: f64,
        duration_ms: u64,
        aimed_by: Option<TreeReader>,
    ) -> Result<TapAtCoordResult, RunnerTransportError> {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Req {
            nx: f64,
            ny: f64,
            duration_ms: u64,
            #[serde(skip_serializing_if = "Option::is_none")]
            aimed_by: Option<TreeReader>,
        }
        let body: Landed = self
            .json_post(
                "/long-press-at-norm-coord",
                &Req {
                    nx,
                    ny,
                    duration_ms,
                    aimed_by,
                },
                None,
            )
            .await?;
        body.into_result("/long-press-at-norm-coord")
    }
}

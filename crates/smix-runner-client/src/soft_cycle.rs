//! Choosing between `smix runner cycle`'s in-process soft cycle and the
//! hard one.

/// Outcome of probing a live runner for the in-process soft-cycle
/// (`POST /soft-cycle` + a `GET /health` re-confirmation after the
/// FlyingFox bounce).
///
/// The soft-cycle keeps the xcodebuild/XCUITest host alive and bounces
/// the in-process server + rebinds the app, which is why it recovers a
/// reachable runner in seconds instead of the ~36 s SIGINT-teardown +
/// respawn a hard cycle pays. Only a runner that is alive AND answering
/// `/health` can be soft-cycled; everything else routes to the hard
/// fallback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SoftCycleProbe {
    /// `/soft-cycle` returned 200 and `/health` answered again after the
    /// bounce — the runner recovered in-process. Carries the CLI-observed
    /// warm recovery wall-time in ms.
    Recovered {
        /// Warm recovery wall-time, ms (POST start → `/health` 200 again).
        wall_ms: u64,
    },
    /// `/health` never answered — the host is dead or wedged, so there is
    /// no in-process channel to soft-cycle through.
    Unreachable,
    /// The runner answered but does not know `/soft-cycle` (a runner that
    /// shipped before v2.8-C2).
    Unsupported,
    /// `/soft-cycle` was reached but the bounce did not complete (the
    /// server never came back, or the route errored).
    Failed(String),
}

/// The action `smix runner cycle` takes given a [`SoftCycleProbe`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CyclePlan {
    /// The in-process soft-cycle recovered the runner; nothing else to
    /// do. Carries the measured warm recovery wall-time (ms).
    Soft {
        /// Warm recovery wall-time, ms.
        wall_ms: u64,
    },
    /// Fall back to the xcodebuild hard-cycle (`down()` + `up()`),
    /// byte-identical to the pre-C2 behaviour. Carries a human reason.
    HardFallback {
        /// Why the soft-cycle was not taken.
        reason: String,
    },
}

/// Map a soft-cycle probe to the cycle action. Pure — the network work
/// happens in the probe; this only chooses the branch, so the recoverable
/// / unrecoverable split is checkable device-free.
#[must_use]
pub fn soft_cycle_plan(probe: SoftCycleProbe) -> CyclePlan {
    match probe {
        SoftCycleProbe::Recovered { wall_ms } => CyclePlan::Soft { wall_ms },
        SoftCycleProbe::Unreachable => CyclePlan::HardFallback {
            reason: "runner /health unreachable — host dead or wedged".to_string(),
        },
        SoftCycleProbe::Unsupported => CyclePlan::HardFallback {
            reason: "runner does not support /soft-cycle (older runner)".to_string(),
        },
        SoftCycleProbe::Failed(detail) => CyclePlan::HardFallback {
            reason: format!("soft-cycle did not complete: {detail}"),
        },
    }
}

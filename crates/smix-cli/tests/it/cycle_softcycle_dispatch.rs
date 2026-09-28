//! `smix runner cycle` dispatch: given a soft-cycle probe, decide whether
//! the in-process soft-cycle recovered the runner or whether the CLI must
//! fall back to the xcodebuild hard-cycle (`down()` + `up()`).
//!
//! The decision is a pure function (`smix_runner_client::soft_cycle_plan`)
//! so it is checkable device-free. The network probe that produces the
//! `SoftCycleProbe` (`runner::try_soft_cycle`) talks to a live runner and
//! is exercised on device, not here.
//!
//! Recoverable subset → soft; host dead / unreachable / an older runner
//! that never learned `/soft-cycle` → hard fallback (byte-identical to the
//! pre-C2 behaviour, so the N=1 / no-supervisor contract is preserved).

use smix_runner_client::{CyclePlan, SoftCycleProbe, soft_cycle_plan};

#[test]
fn recovered_probe_picks_soft() {
    let plan = soft_cycle_plan(SoftCycleProbe::Recovered { wall_ms: 742 });
    assert_eq!(plan, CyclePlan::Soft { wall_ms: 742 });
}

#[test]
fn unreachable_host_falls_back_to_hard() {
    let plan = soft_cycle_plan(SoftCycleProbe::Unreachable);
    match plan {
        CyclePlan::HardFallback { reason } => {
            assert!(
                reason.contains("unreachable"),
                "reason should name the unreachable host: {reason}"
            );
        }
        other => panic!("expected hard fallback, got {other:?}"),
    }
}

#[test]
fn older_runner_without_soft_cycle_falls_back_to_hard() {
    let plan = soft_cycle_plan(SoftCycleProbe::Unsupported);
    match plan {
        CyclePlan::HardFallback { reason } => {
            assert!(
                reason.contains("soft-cycle") || reason.contains("older"),
                "reason should name the unsupported runner: {reason}"
            );
        }
        other => panic!("expected hard fallback, got {other:?}"),
    }
}

#[test]
fn incomplete_bounce_falls_back_to_hard() {
    let plan = soft_cycle_plan(SoftCycleProbe::Failed("server never came back".into()));
    match plan {
        CyclePlan::HardFallback { reason } => {
            assert!(
                reason.contains("server never came back"),
                "reason should carry the failure detail: {reason}"
            );
        }
        other => panic!("expected hard fallback, got {other:?}"),
    }
}

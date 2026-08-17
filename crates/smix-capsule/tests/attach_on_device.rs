//! C6d on a device: inject a first-attempt timeout, then let the real
//! bring-up attach — proving "really timed out, really retried, really
//! attached" against an iOS simulator, not a decision table.
//!
//! `#[ignore]` so `cargo test` and preflight do not try to reach a
//! device; the e2e script (`scripts/dev/v6.2-c6d-attach-on-device-e2e.sh`)
//! boots the sim, sets the env below, and runs this with `--ignored`.
//!
//! The seam is `up_on_with`: the fake times out the first attempt (no
//! xcodebuild spawned, so nothing grabs the port) and delegates every
//! attempt after it to the real bring-up. The retry between them —
//! `xcrun simctl launch` to foreground, then attach — is up_on_with's own
//! production code, unchanged.

use std::path::{Path, PathBuf};

use smix_capsule::runner::{
    Attempt, BringUpAttempter, RealBringUp, RunnerTarget, UpOptions, up_on_with,
};

/// First attempt times out (injected); the rest are the real thing.
struct FirstTimesOutThenReal {
    real: RealBringUp,
    attach_flags: Vec<bool>,
}

impl BringUpAttempter for FirstTimesOutThenReal {
    #[allow(clippy::too_many_arguments)]
    fn attempt(
        &mut self,
        root: &Path,
        udid: &str,
        port: u16,
        bundle: Option<&str>,
        runner_project: Option<&Path>,
        target: RunnerTarget<'_>,
        record_enabled: bool,
        supervise: bool,
        attach: bool,
        timeout_secs: u64,
    ) -> Result<Attempt, String> {
        self.attach_flags.push(attach);
        if self.attach_flags.len() == 1 {
            // Deterministic first-attempt timeout — no xcodebuild, no
            // port-grabbing orphan, no clock race.
            return Ok(Attempt::TimedOut {
                last_session_gap: Some("injected: first bring-up never bound".into()),
            });
        }
        self.real.attempt(
            root,
            udid,
            port,
            bundle,
            runner_project,
            target,
            record_enabled,
            supervise,
            attach,
            timeout_secs,
        )
    }
}

fn env_or_panic(key: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| {
        panic!(
            "{key} is not set — this test is driven by \
             scripts/dev/v6.2-c6d-attach-on-device-e2e.sh, which supplies the \
             device it booted. Running it bare has no device to reach."
        )
    })
}

#[test]
#[ignore = "needs a booted iOS simulator; run via the C6d e2e script"]
fn first_timeout_then_real_attach_brings_the_runner_up() {
    let udid = env_or_panic("SMIX_C6D_UDID");
    let port: u16 = env_or_panic("SMIX_C6D_PORT")
        .parse()
        .expect("port is a number");
    let bundle = env_or_panic("SMIX_C6D_BUNDLE");
    let project = PathBuf::from(env_or_panic("SMIX_C6D_RUNNER_PROJECT"));
    let root = tempfile::tempdir().expect("tempdir");

    let mut fake = FirstTimesOutThenReal {
        real: RealBringUp,
        attach_flags: Vec::new(),
    };
    let result = up_on_with(
        &mut fake,
        root.path(),
        &udid,
        port,
        Some(&bundle),
        Some(&project),
        UpOptions::default(),
        RunnerTarget::Simulator,
    );

    assert_eq!(
        result,
        Ok(()),
        "attach retry should have brought the runner up after the injected timeout"
    );
    assert_eq!(
        fake.attach_flags,
        vec![false, true],
        "exactly two attempts: first relaunch (false), then attach (true)"
    );
}

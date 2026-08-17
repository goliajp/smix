//! The bring-up seam: `up_on_with` drives an injected attempter and
//! returns on the first `Attempt::Up`, threading the initial attach flag
//! (false) into the first attempt. This is the device-free half of C6d —
//! it pins that the seam is wired (a fake attempter reaches the loop and
//! its result decides the outcome) without a simulator. The "really
//! timed out → really attached" chain runs on a device in
//! `attach_on_device.rs` (`#[ignore]`), driven by the e2e script.

use std::path::Path;

use smix_capsule::runner::{Attempt, BringUpAttempter, RunnerTarget, UpOptions, up_on_with};

struct FakeUp {
    attach_flags: Vec<bool>,
}

impl BringUpAttempter for FakeUp {
    #[allow(clippy::too_many_arguments)]
    fn attempt(
        &mut self,
        _root: &Path,
        _udid: &str,
        _port: u16,
        _bundle: Option<&str>,
        _runner_project: Option<&Path>,
        _target: RunnerTarget<'_>,
        _record_enabled: bool,
        _supervise: bool,
        attach: bool,
        _timeout_secs: u64,
    ) -> Result<Attempt, String> {
        self.attach_flags.push(attach);
        Ok(Attempt::Up)
    }
}

#[test]
fn up_on_with_drives_the_attempter_and_returns_on_up() {
    let root = tempfile::tempdir().expect("tempdir");
    // A free port so the "already up" health short-circuit does not fire.
    let port = {
        let l = std::net::TcpListener::bind("127.0.0.1:0").expect("bind ephemeral");
        l.local_addr().expect("addr").port()
    };
    let mut fake = FakeUp {
        attach_flags: Vec::new(),
    };
    let result = up_on_with(
        &mut fake,
        root.path(),
        "C6D-TEST-UDID",
        port,
        Some("jp.golia.smix.fixture"),
        None,
        UpOptions::default(),
        RunnerTarget::Simulator,
    );
    assert_eq!(result, Ok(()), "up_on_with returns on the attempter's Up");
    assert_eq!(
        fake.attach_flags,
        vec![false],
        "the first bring-up is asked with attach=false (relaunch, not attach)"
    );
}

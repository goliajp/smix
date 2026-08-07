//! The gate: actions that change a device, reachable only by holding a
//! lease on it.
//!
//! [`DeviceControl`] treats `screenshot` and `keychain_reset` as the same
//! kind of thing — two methods on one trait, either callable by whoever
//! holds it. `ACTION_LEVELS` writes down that they are not, but a table
//! only describes; it cannot stop anything. This does.
//!
//! Everything classed `Device` or `Destructive` lives on [`Leased`], and
//! the only way to get a `Leased` is to take the device's lease. A caller
//! holding the bare trait can still read the screen and drive the app —
//! and cannot wipe a keychain.
//!
//! The bare-trait methods are not removed: they are published API, and
//! removing them is a major-version change rather than something to do on
//! the way past. They are deprecated in favour of this, which is what a
//! deprecation is for.

use std::path::{Path, PathBuf};

use smix_lease::{Admission, CleanupExecutor, CleanupReport, store};
use smix_simctl::DeviceControlError;

use crate::device_control::{DeviceControl, Permission, PermissionAction};

/// Why the gate would not open.
#[derive(Debug, thiserror::Error)]
pub enum AdmissionError {
    /// Somebody else is using the device. Named, not merely refused: a
    /// caller told "busy" can do nothing with that, and a person told
    /// which pid holds it can.
    #[error(
        "device {device_id} is in use by pid {holder_pid} ({holder_cmd}), held since {acquired_at}\n\
         Wait for it to finish, or run `smix lease status {device_id}` to see what it has open."
    )]
    InUse {
        /// The device.
        device_id: String,
        /// Who holds it.
        holder_pid: u32,
        /// What they are running.
        holder_cmd: String,
        /// Since when (RFC3339).
        acquired_at: String,
    },
    /// The previous holder is gone, but settling what it left behind did
    /// not fully succeed. The device is not handed over in that state —
    /// the next holder would inherit a runner still occupying the
    /// device's automation slot and no way to know.
    #[error("device {device_id} could not be settled after its last holder died:\n{details}")]
    NotSettled {
        /// The device.
        device_id: String,
        /// What failed, one per line.
        details: String,
    },
    /// The ledger itself could not be read or written.
    #[error(transparent)]
    Ledger(#[from] store::LeaseError),
}

/// A device this process holds the lease on.
///
/// Obtained from [`Leased::acquire`]. Drops the lease's resources on
/// [`Leased::release`].
pub struct Leased<'a> {
    inner: &'a dyn DeviceControl,
    device_id: String,
    root: PathBuf,
    /// What settling the previous holder's mess produced, if anything.
    /// Carried so the caller can report it rather than have it vanish
    /// into a log nobody reads.
    settled: Vec<CleanupReport>,
}

impl<'a> Leased<'a> {
    /// Take the device's lease, settling an abandoned session first.
    ///
    /// Three outcomes, and the middle one is the reason this is not just
    /// a boolean: free (granted), in use by someone alive (refused, with
    /// their name), or abandoned (settled by `executor`, *then* granted).
    /// Handing over an abandoned device without settling it first would
    /// give the next holder a device with someone else's runner still on
    /// it.
    pub fn acquire(
        inner: &'a dyn DeviceControl,
        root: &Path,
        device_id: &str,
        executor: &dyn CleanupExecutor,
    ) -> Result<Self, AdmissionError> {
        let facts = store::collect_facts(root, device_id)?;
        let settled = match smix_lease::assess(&facts) {
            Admission::Granted => Vec::new(),
            Admission::Denied(c) => {
                return Err(AdmissionError::InUse {
                    device_id: device_id.to_string(),
                    holder_pid: c.holder.pid,
                    holder_cmd: c.holder.cmd,
                    acquired_at: c.acquired_at,
                });
            }
            Admission::Reclaimable { cleanup, .. } => {
                let reports = executor.execute(root, &cleanup);
                let failures: Vec<&str> = reports
                    .iter()
                    .filter(|r| !r.clean)
                    .map(|r| r.line.as_str())
                    .collect();
                if !failures.is_empty() {
                    return Err(AdmissionError::NotSettled {
                        device_id: device_id.to_string(),
                        details: failures.join("\n"),
                    });
                }
                store::remove(root, device_id)?;
                reports
            }
        };
        Ok(Self {
            inner,
            device_id: device_id.to_string(),
            root: root.to_path_buf(),
            settled,
        })
    }

    /// What settling the previous holder's session produced. Empty when
    /// the device was already free.
    pub fn settled(&self) -> &[CleanupReport] {
        &self.settled
    }

    /// The device this lease is for.
    pub fn device_id(&self) -> &str {
        &self.device_id
    }

    /// Record something opened on this device, so a later process can
    /// close it if this one dies first.
    pub fn record(&self, resource: smix_lease::Resource) -> Result<(), store::LeaseError> {
        store::add_resource(&self.root, &self.device_id, resource)
    }

    /// Give the device back.
    ///
    /// Explicit rather than on `Drop`: releasing writes to the ledger and
    /// writing can fail, and a `Drop` that fails has nowhere to say so.
    /// A lease left unreleased is not a leak — the next process finds it,
    /// sees the holder is gone, and settles it. That is the whole design.
    ///
    /// What is dropped is the process-backed rows. A `Booted` row
    /// survives, because it is not part of this lease: it records who may
    /// shut the device down, and the answer does not change just because
    /// one command finished with it. Dropping it here would lose the
    /// right to shut down a device smix itself turned on.
    pub fn release(self) -> Result<(), store::LeaseError> {
        store::drop_process_rows(&self.root, &self.device_id)
    }

    // === Device level ===

    /// Quieten animations on the device.
    pub async fn set_animations_quiet(&self, quiet: bool) -> Result<(), DeviceControlError> {
        self.inner
            .set_animations_quiet(&self.device_id, quiet)
            .await
    }

    /// Write the device pasteboard.
    pub async fn pasteboard_set(&self, text: &str) -> Result<(), DeviceControlError> {
        self.inner.pasteboard_set(&self.device_id, text).await
    }

    /// Add media to the device library.
    pub async fn add_media(&self, paths: &[String]) -> Result<(), DeviceControlError> {
        self.inner.add_media(&self.device_id, paths).await
    }

    /// Set the simulated location.
    pub async fn location_set(&self, lat: f64, lon: f64) -> Result<(), DeviceControlError> {
        self.inner.location_set(&self.device_id, lat, lon).await
    }

    /// Start a simulated location run.
    pub async fn location_start(
        &self,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), DeviceControlError> {
        self.inner
            .location_start(&self.device_id, points, speed_mps)
            .await
    }

    /// Start recording the device display, and write it into the ledger.
    ///
    /// The ledger row is the difference between a recording that survives
    /// this process dying and one that does not. `simctl io recordVideo`
    /// writes its mp4 trailer on SIGINT and on nothing else; a hard kill
    /// of whoever started it leaves an orphan writing into a file that
    /// will never be playable, and — before the row existed — no record
    /// anywhere that it was running.
    ///
    /// A failure to record it is not swallowed. Starting a recording that
    /// nobody can later find is the exact state this is here to prevent,
    /// so the recording is stopped again and the error returned.
    pub async fn start_recording(&self, output_path: &Path) -> Result<(), DeviceControlError> {
        self.inner
            .start_recording(&self.device_id, output_path)
            .await?;
        let pid = self.inner.recording_pid().await;
        let proc = pid
            .and_then(smix_lease::store::identify)
            .unwrap_or_else(|| smix_lease::ProcIdentity {
                pid: pid.unwrap_or(0),
                // No start time matches nothing, so a recording we could
                // not identify reads as already gone rather than as
                // something to signal blindly.
                started_at: String::new(),
                cmd: format!("simctl io {} recordVideo", self.device_id),
            });
        if let Err(e) = self.record(smix_lease::Resource::Recording {
            path: output_path.to_string_lossy().into_owned(),
            proc,
        }) {
            let _ = self.inner.stop_recording().await;
            return Err(DeviceControlError::non_zero_exit(
                "io recordVideo",
                -1,
                format!(
                    "recording started but could not be written to the device \
                     ledger ({e}); stopped it again rather than leave one \
                     nothing can find"
                )
                .as_str(),
            ));
        }
        Ok(())
    }

    /// Stop the recording and drop its ledger row.
    pub async fn stop_recording(&self) -> Result<(), DeviceControlError> {
        self.inner.stop_recording().await?;
        if let Err(e) = smix_lease::store::drop_resource_kind(
            &self.root,
            &self.device_id,
            &smix_lease::Resource::Recording {
                path: String::new(),
                proc: smix_lease::store::identify_self(),
            },
        ) {
            // The recording did stop; saying otherwise would have callers
            // retry a stop that already happened.
            eprintln!("warning: recording row not cleared from the device ledger: {e}");
        }
        Ok(())
    }

    // === Destructive level ===

    /// Uninstall the app, taking its container with it.
    pub async fn uninstall(&self, bundle_id: &str) -> Result<(), DeviceControlError> {
        self.inner.uninstall(&self.device_id, bundle_id).await
    }

    /// Reset the device keychain. Device-wide, not app-scoped.
    pub async fn keychain_reset(&self) -> Result<(), DeviceControlError> {
        self.inner.keychain_reset(&self.device_id).await
    }

    /// Reset every privacy grant for the app.
    pub async fn privacy_reset_all(&self, bundle_id: &str) -> Result<(), DeviceControlError> {
        self.inner
            .privacy_reset_all(&self.device_id, bundle_id)
            .await
    }

    /// Wipe the app's persisted data without uninstalling it.
    pub async fn clear_app_sandbox(&self, bundle_id: &str) -> Result<(), DeviceControlError> {
        self.inner
            .clear_app_sandbox(&self.device_id, bundle_id)
            .await
    }

    /// Delete one key from the app's persisted defaults.
    pub async fn user_defaults_delete(
        &self,
        bundle_id: &str,
        key: &str,
    ) -> Result<bool, DeviceControlError> {
        self.inner
            .user_defaults_delete(&self.device_id, bundle_id, key)
            .await
    }

    /// Grant or revoke a permission for the app.
    pub async fn set_permission(
        &self,
        bundle_id: &str,
        permission: Permission,
        action: PermissionAction,
    ) -> Result<(), DeviceControlError> {
        self.inner
            .set_permission(&self.device_id, bundle_id, permission, action)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_lease::{CleanupAction, Lease, ProcIdentity, Resource};
    use std::cell::RefCell;

    /// Records what it was asked to close, and answers however the test
    /// needs it to.
    struct RecordingExecutor {
        seen: RefCell<Vec<CleanupAction>>,
        clean: bool,
    }

    impl CleanupExecutor for RecordingExecutor {
        fn execute(&self, _root: &Path, actions: &[CleanupAction]) -> Vec<CleanupReport> {
            self.seen.borrow_mut().extend_from_slice(actions);
            actions
                .iter()
                .map(|_| CleanupReport {
                    line: "handled".into(),
                    clean: self.clean,
                })
                .collect()
        }
    }

    fn executor(clean: bool) -> RecordingExecutor {
        RecordingExecutor {
            seen: RefCell::new(Vec::new()),
            clean,
        }
    }

    /// Stands in for a device that can record. Reports a pid that is
    /// certainly alive — this process — so the ledger row it produces is
    /// one a probe would accept.
    struct Recorder {
        // `DeviceControl` is `Send + Sync`, so the interior mutability
        // here has to be too — a `RefCell` compiles everywhere except at
        // the trait bound.
        started: std::sync::Mutex<Vec<String>>,
        stopped: std::sync::Mutex<usize>,
    }

    impl Recorder {
        fn new() -> Self {
            Self {
                started: std::sync::Mutex::new(Vec::new()),
                stopped: std::sync::Mutex::new(0),
            }
        }
        fn stop_count(&self) -> usize {
            *self.stopped.lock().expect("lock")
        }
    }

    #[async_trait::async_trait]
    impl DeviceControl for Recorder {
        fn platform(&self) -> smix_driver::Platform {
            smix_driver::Platform::Ios
        }
        async fn start_recording(&self, _: &str, path: &Path) -> Result<(), DeviceControlError> {
            self.started
                .lock()
                .expect("lock")
                .push(path.to_string_lossy().into_owned());
            Ok(())
        }
        async fn stop_recording(&self) -> Result<(), DeviceControlError> {
            *self.stopped.lock().expect("lock") += 1;
            Ok(())
        }
        async fn recording_pid(&self) -> Option<u32> {
            Some(std::process::id())
        }
        async fn launch(&self, _: &str, _: &str) -> Result<u32, DeviceControlError> {
            unreachable!()
        }
        async fn launch_with_args(
            &self,
            _: &str,
            _: &str,
            _: &[String],
            _: Option<&str>,
        ) -> Result<u32, DeviceControlError> {
            unreachable!()
        }
        async fn terminate(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn install(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn uninstall(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn keychain_reset(&self, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn privacy_reset_all(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn clear_app_sandbox(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn open_url(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn send_push(&self, _: &str, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn screenshot(&self, _: &str) -> Result<Vec<u8>, DeviceControlError> {
            unreachable!()
        }
        async fn capture_bgra(
            &self,
            _: &str,
        ) -> Result<smix_simctl::surface_capture::CapturedFrame, DeviceControlError> {
            unreachable!()
        }
        async fn pasteboard_set(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn pasteboard_get(&self, _: &str) -> Result<String, DeviceControlError> {
            unreachable!()
        }
        async fn add_media(&self, _: &str, _: &[String]) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn location_set(&self, _: &str, _: f64, _: f64) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn location_start(
            &self,
            _: &str,
            _: &[(f64, f64)],
            _: Option<f64>,
        ) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn set_permission(
            &self,
            _: &str,
            _: &str,
            _: Permission,
            _: PermissionAction,
        ) -> Result<(), DeviceControlError> {
            unreachable!()
        }
    }

    /// A `DeviceControl` that would panic if anything reached the device.
    /// Admission is decided before any of it is touched, so nothing
    /// should.
    struct NeverCalled;

    #[async_trait::async_trait]
    impl DeviceControl for NeverCalled {
        fn platform(&self) -> smix_driver::Platform {
            smix_driver::Platform::Ios
        }
        async fn launch(&self, _: &str, _: &str) -> Result<u32, DeviceControlError> {
            unreachable!("admission tests never reach the device")
        }
        async fn launch_with_args(
            &self,
            _: &str,
            _: &str,
            _: &[String],
            _: Option<&str>,
        ) -> Result<u32, DeviceControlError> {
            unreachable!()
        }
        async fn terminate(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn install(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn uninstall(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn keychain_reset(&self, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn privacy_reset_all(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn clear_app_sandbox(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn open_url(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn send_push(&self, _: &str, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn screenshot(&self, _: &str) -> Result<Vec<u8>, DeviceControlError> {
            unreachable!()
        }
        async fn capture_bgra(
            &self,
            _: &str,
        ) -> Result<smix_simctl::surface_capture::CapturedFrame, DeviceControlError> {
            unreachable!()
        }
        async fn pasteboard_set(&self, _: &str, _: &str) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn pasteboard_get(&self, _: &str) -> Result<String, DeviceControlError> {
            unreachable!()
        }
        async fn add_media(&self, _: &str, _: &[String]) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn location_set(&self, _: &str, _: f64, _: f64) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn location_start(
            &self,
            _: &str,
            _: &[(f64, f64)],
            _: Option<f64>,
        ) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn set_permission(
            &self,
            _: &str,
            _: &str,
            _: Permission,
            _: PermissionAction,
        ) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn start_recording(&self, _: &str, _: &Path) -> Result<(), DeviceControlError> {
            unreachable!()
        }
        async fn stop_recording(&self) -> Result<(), DeviceControlError> {
            unreachable!()
        }
    }

    fn live_lease(device_id: &str) -> Lease {
        Lease {
            device_id: device_id.into(),
            holder: store::identify_self(),
            acquired_at: store::now_rfc3339(),
            heartbeat_at: store::now_rfc3339(),
            resources: vec![Resource::Booted { by_us: true }],
        }
    }

    /// A session whose holder died: a boot, and a runner that went with
    /// it. The runner row is what makes this abandoned rather than merely
    /// a device left switched on — a ledger holding only a boot is the
    /// latter, and settling it would shut down a device somebody just
    /// asked for.
    fn dead_holder_lease(device_id: &str) -> Lease {
        Lease {
            device_id: device_id.into(),
            holder: ProcIdentity {
                pid: 0,
                started_at: "Thu Aug  6 10:00:00 2026".into(),
                cmd: "smix run gone.yaml".into(),
            },
            acquired_at: store::now_rfc3339(),
            heartbeat_at: store::now_rfc3339(),
            resources: vec![
                Resource::Booted { by_us: true },
                Resource::Runner {
                    port: 22087,
                    proc: ProcIdentity {
                        pid: 0,
                        started_at: "Thu Aug  6 10:00:05 2026".into(),
                        cmd: "xcodebuild test".into(),
                    },
                },
            ],
        }
    }

    #[test]
    fn a_free_device_is_granted_without_cleaning_anything() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ex = executor(true);
        let leased = Leased::acquire(&NeverCalled, tmp.path(), "UDID-FREE", &ex).expect("granted");
        assert!(leased.settled().is_empty());
        assert!(
            ex.seen.borrow().is_empty(),
            "nothing to clean, nothing done"
        );
    }

    #[test]
    fn a_live_holder_refuses_and_names_itself() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let mut lease = live_lease("UDID-BUSY");
        // A different pid than ours, alive: this process's parent shell
        // would do, but pid 1 is guaranteed present and is not us.
        lease.holder = store::identify(1).unwrap_or(ProcIdentity {
            pid: 1,
            started_at: "Thu Aug  6 10:00:00 2026".into(),
            cmd: "launchd".into(),
        });
        store::write(tmp.path(), &lease).expect("write");
        let ex = executor(true);
        match Leased::acquire(&NeverCalled, tmp.path(), "UDID-BUSY", &ex) {
            Err(AdmissionError::InUse { holder_pid, .. }) => assert_eq!(holder_pid, 1),
            other => panic!("expected InUse, got {other:?}", other = other.err()),
        }
        assert!(
            ex.seen.borrow().is_empty(),
            "a live session must never be cleaned up"
        );
    }

    #[test]
    fn an_abandoned_device_is_settled_before_it_is_handed_over() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        store::write(tmp.path(), &dead_holder_lease("UDID-DEAD")).expect("write");
        let ex = executor(true);
        let leased = Leased::acquire(&NeverCalled, tmp.path(), "UDID-DEAD", &ex).expect("granted");
        assert_eq!(
            ex.seen.borrow().len(),
            2,
            "the dead runner and the boot it performed were both owed"
        );
        assert_eq!(leased.settled().len(), 2, "and the caller is told about it");
        assert!(
            store::read(tmp.path(), "UDID-DEAD")
                .expect("read")
                .is_none(),
            "the dead holder's ledger is gone once settled"
        );
    }

    #[test]
    fn a_settle_that_failed_does_not_hand_the_device_over() {
        // Handing it over anyway would give the next holder a device with
        // someone else's runner still on it, and no way to know.
        let tmp = tempfile::tempdir().expect("tmpdir");
        store::write(tmp.path(), &dead_holder_lease("UDID-STUCK")).expect("write");
        let ex = executor(false);
        match Leased::acquire(&NeverCalled, tmp.path(), "UDID-STUCK", &ex) {
            Err(AdmissionError::NotSettled { details, .. }) => assert!(details.contains("handled")),
            other => panic!("expected NotSettled, got {other:?}", other = other.err()),
        }
        assert!(
            store::read(tmp.path(), "UDID-STUCK")
                .expect("read")
                .is_some(),
            "the ledger stays so the next command still sees what did not close"
        );
    }

    #[test]
    fn releasing_drops_what_the_lease_covered() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ex = executor(true);
        let leased = Leased::acquire(&NeverCalled, tmp.path(), "UDID-REL", &ex).expect("granted");
        leased
            .record(Resource::Recording {
                path: "x.mov".into(),
                proc: store::identify_self(),
            })
            .expect("record");
        assert!(store::read(tmp.path(), "UDID-REL").expect("read").is_some());
        leased.release().expect("release");
        assert!(
            store::read(tmp.path(), "UDID-REL").expect("read").is_none(),
            "nothing was left that this process owes a teardown"
        );
    }

    #[test]
    fn releasing_keeps_the_right_to_shut_down_a_device_we_booted() {
        // The boot outlives the lease. Dropping it here would leave a
        // device smix turned on with nobody entitled to turn it off.
        let tmp = tempfile::tempdir().expect("tmpdir");
        let ex = executor(true);
        let leased = Leased::acquire(&NeverCalled, tmp.path(), "UDID-BOOT", &ex).expect("granted");
        leased
            .record(Resource::Booted { by_us: true })
            .expect("boot row");
        leased
            .record(Resource::Recording {
                path: "x.mov".into(),
                proc: store::identify_self(),
            })
            .expect("recording row");
        leased.release().expect("release");
        let after = store::read(tmp.path(), "UDID-BOOT")
            .expect("read")
            .expect("ledger kept");
        assert_eq!(after.resources, vec![Resource::Booted { by_us: true }]);
    }

    #[tokio::test]
    async fn a_started_recording_is_written_into_the_ledger() {
        // Without the row, the only record of a running recording is a
        // struct inside this process — and the mp4 it is writing needs a
        // SIGINT from somebody who knows it exists.
        let tmp = tempfile::tempdir().expect("tmpdir");
        let dev = Recorder::new();
        let ex = executor(true);
        let leased = Leased::acquire(&dev, tmp.path(), "UDID-REC", &ex).expect("granted");
        leased
            .start_recording(Path::new("/tmp/run.mov"))
            .await
            .expect("start");

        let lease = store::read(tmp.path(), "UDID-REC")
            .expect("read")
            .expect("ledger");
        match lease.resources.as_slice() {
            [Resource::Recording { path, proc }] => {
                assert_eq!(path, "/tmp/run.mov", "the row must say where it is writing");
                assert!(
                    !proc.started_at.is_empty(),
                    "no start time means nothing can verify this pid later"
                );
            }
            other => panic!("expected one recording row, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn stopping_a_recording_drops_its_row() {
        let tmp = tempfile::tempdir().expect("tmpdir");
        let dev = Recorder::new();
        let ex = executor(true);
        let leased = Leased::acquire(&dev, tmp.path(), "UDID-REC2", &ex).expect("granted");
        leased
            .start_recording(Path::new("/tmp/run.mov"))
            .await
            .expect("start");
        leased.stop_recording().await.expect("stop");
        assert_eq!(dev.stop_count(), 1);
        assert!(
            store::read(tmp.path(), "UDID-REC2")
                .expect("read")
                .is_none(),
            "nothing is left owed once the recording is stopped"
        );
    }
}

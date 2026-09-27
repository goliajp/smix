//! Where the runner handle lives: the device's lease, and nowhere else.
//!
//! There used to be two books. The machine's device ledger held a
//! `Runner` row; the checkout held a singleton per platform
//! (`runner-ios`, `runner-android`, and before those
//! `.smix/runner/state.json`). The singleton had one slot per platform,
//! so two simulators driven from one checkout overwrote each other's
//! record, `down` on one erased the other's, and `down --runner-port P`
//! stopped whichever runner the slot happened to name. The failure paths
//! of `up` cleared one book and left the other, so the two disagreed
//! even with a single runner.
//!
//! Now the ledger row is the record ([`find`]), keyed by device and
//! looked up by port or device. What the checkout still holds is read
//! only to be cited when a refusal needs it ([`legacy_evidence`]).

use std::path::Path;

use crate::runner::RunnerState;

fn store_root(root: &Path) -> std::path::PathBuf {
    root.join(".smix")
}

fn read_legacy(root: &Path) -> Result<Option<RunnerState>, String> {
    let legacy = root.join(".smix/runner/state.json");
    let text = match std::fs::read_to_string(&legacy) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("read {}: {e}", legacy.display())),
    };
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| format!("{} is not a runner state: {e}", legacy.display()))
}

/// How to find a runner in the device ledger.
#[derive(Debug, Clone, Copy)]
pub enum Lookup<'a> {
    /// The runner answering on this port.
    Port(u16),
    /// The runner driving this device.
    Device(&'a str),
}

/// The runner a device's lease records.
///
/// The lease is the only record of a runner. Every lookup is by port or
/// by device, never "the iOS runner": a checkout can drive several
/// simulators at once, and the one-key-per-platform record that used to
/// live in the checkout had each new runner overwrite the last.
///
/// Rows are left behind: a runner killed from outside, or one another
/// smix started and never took down, still names its port. So when
/// several devices' rows name one port, each row's runner is asked
/// whether it is still the process that was recorded, and only the ones
/// that are count. One such row is the runner on the port; several is an
/// error that names them all, since only one process can hold a port and
/// picking one would be guessing which. No row is removed here — a stale
/// row belongs to its device's holder to settle.
pub fn find(
    leases: &smix_lease::store::LeaseDir,
    by: Lookup<'_>,
) -> Result<Option<RunnerState>, String> {
    let devices = match by {
        Lookup::Device(id) => vec![id.to_string()],
        Lookup::Port(_) => leases.device_ids(),
    };
    let mut found: Vec<(RunnerState, smix_lease::ProcIdentity)> = Vec::new();
    for device in &devices {
        let Some(lease) = smix_lease::store::read(leases, device)
            .map_err(|e| format!("read the ledger for {device}: {e}"))?
        else {
            continue;
        };
        let Some((state, proc)) = runner_of(&lease) else {
            continue;
        };
        if let Lookup::Port(port) = by
            && state.port != port
        {
            continue;
        }
        found.push((state, proc));
    }
    if found.len() > 1 {
        found.retain(|(_, proc)| smix_lease::store::probe(proc).identity_matches);
    }
    let mut found: Vec<RunnerState> = found.into_iter().map(|(state, _)| state).collect();
    match found.len() {
        0 | 1 => Ok(found.pop()),
        _ => {
            let named: Vec<String> = found
                .iter()
                .map(|s| format!("{} (pid {})", s.udid, s.pid))
                .collect();
            Err(format!(
                "the ledger has {} running runners on one port: {} — a port is held by \
                 one process, so all but one of them is not listening on it. \
                 `smix runner down --device <id>` the one that should not be there.",
                found.len(),
                named.join(", ")
            ))
        }
    }
}

/// The runner a lease records, with its supervisor if it has one, and
/// the process the row says it is.
fn runner_of(lease: &smix_lease::Lease) -> Option<(RunnerState, smix_lease::ProcIdentity)> {
    let mut runner = None;
    let mut supervisor_pid = None;
    for r in lease.known_resources() {
        match r {
            smix_lease::Resource::Runner {
                port,
                proc,
                bundle,
                log,
            } => {
                runner = Some((
                    RunnerState {
                        pid: proc.pid,
                        udid: lease.device_id.clone(),
                        port: *port,
                        log: log.as_ref().map(std::path::PathBuf::from),
                        bundle: bundle.clone(),
                        supervisor_pid: None,
                    },
                    proc.clone(),
                ));
            }
            smix_lease::Resource::Supervisor { proc } => supervisor_pid = Some(proc.pid),
            _ => {}
        }
    }
    runner.map(|(st, proc)| {
        (
            RunnerState {
                supervisor_pid,
                ..st
            },
            proc,
        )
    })
}

/// What the checkout's old runner record says, for a refusal to cite.
///
/// Evidence only: device facts belong to the machine, not to a checkout,
/// so an old record left in a checkout names where it was read and what it
/// holds, and decides nothing. It is never written and never
/// created — opening a store that is not there would make one.
pub fn legacy_evidence(root: &Path) -> Option<String> {
    let smix = store_root(root);
    let mut said = Vec::new();
    // Only a store that is already there: `Store::open` creates one.
    if smix.join("kv").is_dir()
        && let Ok(store) = smix_store::Store::open(&smix)
    {
        for key in ["runner-ios", "runner-android"] {
            if let Ok(Some(st)) = store.singleton(key).get_json::<RunnerState>() {
                said.push(format!(
                    "{}/kv `{key}` names udid={} port={} pid={}",
                    smix.display(),
                    st.udid,
                    st.port,
                    st.pid
                ));
            }
        }
    }
    if let Ok(Some(st)) = read_legacy(root) {
        said.push(format!(
            "{} names udid={} port={} pid={}",
            root.join(".smix/runner/state.json").display(),
            st.udid,
            st.port,
            st.pid
        ));
    }
    (!said.is_empty()).then(|| said.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_root(name: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("smix-runner-state-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp root");
        dir
    }

    fn state(udid: &str, port: u16) -> RunnerState {
        RunnerState {
            pid: 4242,
            udid: udid.to_string(),
            port,
            log: Some(std::path::PathBuf::from("/tmp/runner.log")),
            bundle: Some("com.example.app".to_string()),
            supervisor_pid: None,
        }
    }

    fn ledger(name: &str) -> smix_lease::store::LeaseDir {
        let dir =
            std::env::temp_dir().join(format!("smix-runner-ledger-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("temp ledger");
        smix_lease::store::LeaseDir::at(dir)
    }

    fn runner_row(port: u16, pid: u32, bundle: Option<&str>) -> smix_lease::Resource {
        smix_lease::Resource::Runner {
            port,
            proc: smix_lease::ProcIdentity {
                pid,
                started_at: "Thu Sep 24 10:00:00 2026".into(),
                cmd: "xcodebuild test".into(),
            },
            bundle: bundle.map(str::to_string),
            log: Some(format!("/tmp/runner-{pid}.log")),
        }
    }

    #[test]
    fn two_devices_each_find_their_own_runner_by_port() {
        let leases = ledger("two-devices");
        smix_lease::store::add_resource(&leases, "UDID-A", runner_row(22087, 11, Some("com.a")))
            .expect("a");
        smix_lease::store::add_resource(&leases, "UDID-B", runner_row(22091, 22, Some("com.b")))
            .expect("b");
        let a = find(&leases, Lookup::Port(22087))
            .expect("a")
            .expect("the runner device A recorded on 22087 is not in the ledger view");
        let b = find(&leases, Lookup::Port(22091))
            .expect("b")
            .expect("the runner device B recorded on 22091 is not in the ledger view");
        assert_eq!(
            (a.udid.as_str(), a.pid, a.bundle.as_deref()),
            ("UDID-A", 11, Some("com.a"))
        );
        assert_eq!(
            (b.udid.as_str(), b.pid, b.bundle.as_deref()),
            ("UDID-B", 22, Some("com.b"))
        );
        assert_eq!(a.log.as_deref(), Some(Path::new("/tmp/runner-11.log")));
        let by_device = find(&leases, Lookup::Device("UDID-B"))
            .expect("b")
            .expect("looking device B up by its id found no runner");
        assert_eq!(by_device.port, 22091);
    }

    #[test]
    fn dropping_one_runner_leaves_the_other_findable() {
        let leases = ledger("drop-one");
        smix_lease::store::add_resource(&leases, "UDID-A", runner_row(22087, 11, Some("com.a")))
            .expect("a");
        smix_lease::store::add_resource(&leases, "UDID-B", runner_row(22091, 22, Some("com.b")))
            .expect("b");
        smix_lease::store::drop_resource_kind(&leases, "UDID-B", &runner_row(0, 0, None))
            .expect("drop b");
        assert!(find(&leases, Lookup::Port(22091)).expect("b").is_none());
        let a = find(&leases, Lookup::Port(22087)).expect("a");
        assert_eq!(
            a.map(|s| s.udid),
            Some("UDID-A".to_string()),
            "taking one runner down lost the other's record"
        );
    }

    fn live_row(port: u16, proc: smix_lease::ProcIdentity) -> smix_lease::Resource {
        smix_lease::Resource::Runner {
            port,
            proc,
            bundle: None,
            log: None,
        }
    }

    fn parent_identity() -> smix_lease::ProcIdentity {
        smix_lease::store::identify(std::os::unix::process::parent_id())
            .expect("the test's parent process is running")
    }

    #[test]
    fn two_live_runners_on_one_port_are_refused_by_name() {
        let leases = ledger("same-port-live");
        let me = smix_lease::store::identify_self();
        smix_lease::store::add_resource(&leases, "UDID-A", live_row(22087, me)).expect("a");
        smix_lease::store::add_resource(&leases, "UDID-B", live_row(22087, parent_identity()))
            .expect("b");
        let err = find(&leases, Lookup::Port(22087)).expect_err("two live rows claim one port");
        assert!(
            err.contains("UDID-A") && err.contains("UDID-B"),
            "the refusal must name both devices: {err}"
        );
    }

    #[test]
    fn a_stale_row_on_the_port_does_not_hide_the_live_one() {
        let leases = ledger("same-port-stale");
        smix_lease::store::add_resource(&leases, "UDID-GONE", runner_row(22087, 11, None))
            .expect("stale");
        let me = smix_lease::store::identify_self();
        smix_lease::store::add_resource(&leases, "UDID-LIVE", live_row(22087, me)).expect("live");
        let st = find(&leases, Lookup::Port(22087))
            .expect("a stale row beside a live one is not a conflict")
            .expect("the live runner on 22087 was not found");
        assert_eq!(st.udid, "UDID-LIVE");
        assert!(
            smix_lease::store::read(&leases, "UDID-GONE")
                .expect("read")
                .is_some(),
            "looking up a port removed another device's row"
        );
    }

    #[test]
    fn rows_on_one_port_whose_runners_are_all_gone_find_nothing() {
        let leases = ledger("same-port-gone");
        smix_lease::store::add_resource(&leases, "UDID-A", runner_row(22087, 11, None)).expect("a");
        smix_lease::store::add_resource(&leases, "UDID-B", runner_row(22087, 22, None)).expect("b");
        assert!(
            find(&leases, Lookup::Port(22087))
                .expect("stale rows are not a conflict")
                .is_none(),
            "a runner whose process is gone was reported as on the port"
        );
    }

    #[test]
    fn a_row_written_before_bundle_and_log_existed_still_reads() {
        let leases = ledger("old-row");
        let path = leases.path().join("UDID-OLD.json");
        std::fs::write(
            &path,
            r#"{"deviceId":"UDID-OLD","holder":{"pid":1,"startedAt":"x","cmd":"smix"},
                "acquiredAt":"2026-09-01T00:00:00Z","heartbeatAt":"2026-09-01T00:00:00Z",
                "resources":[{"kind":"runner","port":22087,
                  "proc":{"pid":7,"startedAt":"x","cmd":"xcodebuild test"}}]}"#,
        )
        .expect("write old row");
        let st = find(&leases, Lookup::Port(22087))
            .expect("reads")
            .expect("a runner row written before bundle and log existed was not read");
        assert_eq!((st.pid, st.bundle, st.log), (7, None, None));
    }

    #[test]
    fn the_supervisor_pid_comes_from_the_same_device() {
        let leases = ledger("supervisor");
        smix_lease::store::add_resource(&leases, "UDID-A", runner_row(22087, 11, None)).expect("a");
        smix_lease::store::add_resource(
            &leases,
            "UDID-A",
            smix_lease::Resource::Supervisor {
                proc: smix_lease::ProcIdentity {
                    pid: 99,
                    started_at: "x".into(),
                    cmd: "smix runner supervise".into(),
                },
            },
        )
        .expect("sup");
        smix_lease::store::add_resource(&leases, "UDID-B", runner_row(22091, 22, None)).expect("b");
        let a = find(&leases, Lookup::Port(22087))
            .expect("a")
            .expect("device A's runner is not in the ledger view");
        let b = find(&leases, Lookup::Port(22091))
            .expect("b")
            .expect("device B's runner is not in the ledger view");
        assert_eq!((a.supervisor_pid, b.supervisor_pid), (Some(99), None));
    }

    #[test]
    fn the_checkout_record_is_evidence_and_nothing_more() {
        let root = temp_root("evidence");
        {
            let store = smix_store::Store::open(&root.join(".smix")).expect("open");
            store
                .singleton("runner-ios")
                .put_json(&state("UDID-CHECKOUT", 22087))
                .expect("put");
        }
        let leases = ledger("evidence");
        assert!(
            find(&leases, Lookup::Port(22087)).expect("reads").is_none(),
            "the checkout's record was read as the ledger's"
        );
        let said = legacy_evidence(&root)
            .expect("the checkout's old runner record was not offered as evidence");
        assert!(
            said.contains("UDID-CHECKOUT") && said.contains(".smix"),
            "{said}"
        );

        let bare = temp_root("evidence-bare");
        assert!(legacy_evidence(&bare).is_none());
        assert!(
            !bare.join(".smix").exists(),
            "looking for evidence created a store in the checkout"
        );
    }

    #[test]
    fn the_pre_store_state_file_is_cited_too() {
        let root = temp_root("evidence-file");
        std::fs::create_dir_all(root.join(".smix/runner")).expect("mkdir");
        std::fs::write(
            root.join(".smix/runner/state.json"),
            serde_json::to_string(&state("UDID-FILE", 22090)).expect("json"),
        )
        .expect("write");
        let said =
            legacy_evidence(&root).expect("the pre-store state file was not offered as evidence");
        assert!(
            said.contains("UDID-FILE") && said.contains("state.json"),
            "{said}"
        );
    }
}

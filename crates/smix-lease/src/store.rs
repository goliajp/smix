//! Where the ledger lives, and how the world is probed to fill in
//! [`Facts`].
//!
//! Everything here does I/O; nothing here judges. The split is what lets
//! `assess` be tested without a filesystem, and it is why this module is
//! deliberately dull.

use crate::{Facts, Held, HolderProbe, Lease, ProcIdentity, Resource};
use std::path::{Path, PathBuf};

/// Why a ledger operation failed.
#[derive(Debug, thiserror::Error)]
pub enum LeaseError {
    /// Filesystem failure.
    #[error("lease I/O failed at {path}: {source}")]
    Io {
        /// Path involved.
        path: String,
        /// Underlying error.
        source: std::io::Error,
    },
    /// The ledger is not readable as a lease.
    #[error(
        "lease ledger {path} is malformed: {detail}\n\
         It records what is still open on this device, so smix will not \
         guess at it. Inspect the file, then delete it to start clean."
    )]
    Malformed {
        /// Path involved.
        path: String,
        /// Parser detail.
        detail: String,
    },
    /// A device id that cannot be a filename.
    #[error(
        "device id {device_id:?} is not addressable — expected a UDID or \
         serial (letters, digits, '.', '_', '-')"
    )]
    BadDeviceId {
        /// The rejected input.
        device_id: String,
    },
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> LeaseError + '_ {
    move |source| LeaseError::Io {
        path: path.display().to_string(),
        source,
    }
}

/// Directory holding one file per device.
pub fn lease_dir(root: &Path) -> PathBuf {
    root.join(".smix").join("leases")
}

/// Path of one device's ledger.
///
/// The device id becomes a filename, so it is checked rather than
/// trusted. A UDID and an Android serial are both alphanumeric with
/// separators; anything else — a path separator above all — is refused
/// instead of sanitised, because a silently rewritten id would address a
/// different device than the caller named.
pub fn lease_path(root: &Path, device_id: &str) -> Result<PathBuf, LeaseError> {
    let ok = !device_id.is_empty()
        && device_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if !ok {
        return Err(LeaseError::BadDeviceId {
            device_id: device_id.to_string(),
        });
    }
    Ok(lease_dir(root).join(format!("{device_id}.json")))
}

/// Read a device's ledger. `Ok(None)` means no lease; a ledger that
/// exists but cannot be parsed is an error, never an absence.
///
/// Reporting a corrupt ledger as "no lease" would hand the device to the
/// next caller along with an orphaned runner and a half-written video the
/// ledger was the only record of.
pub fn read(root: &Path, device_id: &str) -> Result<Option<Lease>, LeaseError> {
    let path = lease_path(root, device_id)?;
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(&path)(e)),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| LeaseError::Malformed {
            path: path.display().to_string(),
            detail: e.to_string(),
        })
}

/// Write a ledger, atomically.
///
/// Temp file plus rename, the same discipline the store uses: a process
/// killed mid-write is exactly the case this whole module exists for, and
/// it must not be the case that leaves a truncated ledger behind.
pub fn write(root: &Path, lease: &Lease) -> Result<(), LeaseError> {
    let path = lease_path(root, &lease.device_id)?;
    let dir = lease_dir(root);
    std::fs::create_dir_all(&dir).map_err(io_err(&dir))?;
    let json = serde_json::to_vec_pretty(lease).map_err(|e| LeaseError::Malformed {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    let tmp = dir.join(format!(".{}.{}.tmp", lease.device_id, std::process::id()));
    std::fs::write(&tmp, &json).map_err(io_err(&tmp))?;
    std::fs::rename(&tmp, &path).map_err(io_err(&path))
}

/// Drop a device's ledger. Absent is success — release is idempotent.
pub fn remove(root: &Path, device_id: &str) -> Result<(), LeaseError> {
    let path = lease_path(root, device_id)?;
    match std::fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(io_err(&path)(e)),
    }
}

fn ps_field(pid: u32, field: &str) -> Option<String> {
    let out = std::process::Command::new("ps")
        .args(["-p", &pid.to_string(), "-o", field])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Look up a live process by pid.
pub fn identify(pid: u32) -> Option<ProcIdentity> {
    let started_at = ps_field(pid, "lstart=")?;
    let cmd = ps_field(pid, "command=").unwrap_or_default();
    Some(ProcIdentity {
        pid,
        started_at,
        cmd,
    })
}

/// This process, as the ledger will record it.
///
/// The start time comes from `ps` rather than from a clock read at
/// startup, so that what is written is the same string a later probe will
/// read back. Two different ways of spelling the same instant would never
/// compare equal, and the comparison is the entire point.
pub fn identify_self() -> ProcIdentity {
    let pid = std::process::id();
    identify(pid).unwrap_or_else(|| ProcIdentity {
        pid,
        // `ps` failing on our own pid is not a state we can be in while
        // running, but the type says it can be. An empty start time
        // compares equal to nothing, so a ledger written this way is
        // reclaimable rather than permanently held — the safe direction.
        started_at: String::new(),
        cmd: std::env::args().collect::<Vec<_>>().join(" "),
    })
}

/// Is the recorded process still the one at that pid?
pub fn probe(recorded: &ProcIdentity) -> HolderProbe {
    match identify(recorded.pid) {
        None => HolderProbe {
            pid_exists: false,
            identity_matches: false,
        },
        Some(live) => HolderProbe {
            pid_exists: true,
            identity_matches: !recorded.started_at.is_empty()
                && live.started_at == recorded.started_at,
        },
    }
}

/// Now, RFC3339.
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

/// Record a resource against a device, creating the ledger if this is
/// the first thing opened on it.
///
/// A resource of the same kind replaces the one already recorded rather
/// than stacking beside it. A device has one runner, one recording, and
/// one boot state; a second row for any of them would be a second thing
/// to tear down that never existed.
pub fn add_resource(root: &Path, device_id: &str, resource: Resource) -> Result<(), LeaseError> {
    let now = now_rfc3339();
    let mut lease = read(root, device_id)?.unwrap_or_else(|| Lease {
        device_id: device_id.to_string(),
        holder: identify_self(),
        acquired_at: now.clone(),
        heartbeat_at: now.clone(),
        resources: Vec::new(),
    });
    let same_kind = std::mem::discriminant(&resource);
    lease
        .resources
        .retain(|r| std::mem::discriminant(r) != same_kind);
    lease.resources.push(resource);
    lease.heartbeat_at = now;
    write(root, &lease)
}

/// Forget every resource of one kind, and the ledger itself once nothing
/// worth tearing down is left.
///
/// A `Booted { by_us: false }` row is not worth keeping a ledger for: it
/// records a device somebody else brought up, which is precisely the
/// thing this process must not act on.
pub fn drop_resource_kind(
    root: &Path,
    device_id: &str,
    sample: &Resource,
) -> Result<(), LeaseError> {
    let Some(mut lease) = read(root, device_id)? else {
        return Ok(());
    };
    let kind = std::mem::discriminant(sample);
    lease
        .resources
        .retain(|r| std::mem::discriminant(r) != kind);
    let worth_keeping = lease
        .resources
        .iter()
        .any(|r| !matches!(r, Resource::Booted { by_us: false }));
    if worth_keeping {
        write(root, &lease)
    } else {
        remove(root, device_id)
    }
}

/// Forget every process-backed row, keeping the device's boot state.
///
/// What a lease covers is the things a holder started; whether smix
/// turned the device on is a separate fact with a longer life. Dropping
/// the boot row here would lose the right to shut down a device smix
/// booted, which is the one thing that stops teardown from turning off
/// somebody else's device — and from leaving its own running.
pub fn drop_process_rows(root: &Path, device_id: &str) -> Result<(), LeaseError> {
    let Some(mut lease) = read(root, device_id)? else {
        return Ok(());
    };
    lease.resources.retain(|r| !crate::is_process_backed(r));
    let worth_keeping = lease
        .resources
        .iter()
        .any(|r| !matches!(r, Resource::Booted { by_us: false }));
    if worth_keeping {
        write(root, &lease)
    } else {
        remove(root, device_id)
    }
}

/// Point the ledger's holder at a different process.
///
/// `smix record start` needs this: the recording outlives the command
/// that started it, so leaving that command as the holder would have the
/// very next smix command find a dead holder with a live recording — read
/// it as an orphan, and stop the recording somebody deliberately started.
/// Handing the holder role to the recording process itself makes the
/// ledger say what is true: this device is busy for as long as that
/// process runs.
pub fn set_holder(root: &Path, device_id: &str, holder: ProcIdentity) -> Result<(), LeaseError> {
    let Some(mut lease) = read(root, device_id)? else {
        return Ok(());
    };
    lease.holder = holder;
    lease.heartbeat_at = now_rfc3339();
    write(root, &lease)
}

/// Gather everything `assess` needs.
pub fn collect_facts(root: &Path, device_id: &str) -> Result<Facts, LeaseError> {
    let existing = read(root, device_id)?.map(|lease| {
        let holder = probe(&lease.holder);
        // Only a runner speaks for "somebody is using this device".
        //
        // A recording is a session's *output*, not the session. Counting
        // it as occupancy sounds harmless and quietly breaks the thing
        // this whole mechanism is for: a session killed mid-recording
        // leaves a `simctl` child still writing, and if that child makes
        // the device read as in-use, nothing will ever send it the SIGINT
        // that writes the mp4 trailer. The file stays unplayable and the
        // ledger row stays forever, waiting for a holder that is dead.
        //
        // A recording nobody is left to stop is exactly an orphan, and
        // orphans are what reconcile is for.
        let any_resource_alive = lease.resources.iter().any(|r| match r {
            // A runner — either platform's — is the session itself.
            Resource::Runner { proc, .. } | Resource::AndroidRunner { proc, .. } => {
                let p = probe(proc);
                p.pid_exists && p.identity_matches
            }
            // A supervisor is not a session either: it is the thing that
            // restarts one. A live supervisor with no live runner is a
            // sidecar watching nothing, and treating it as occupancy
            // would keep the device claimed by a session that has ended.
            // A forwarder is a pipe, not a session — same reasoning as
            // a recording. A pipe nobody is talking through does not make
            // a device busy, and counting it as occupancy would leave an
            // abandoned one running forever.
            Resource::Supervisor { .. }
            | Resource::Recording { .. }
            | Resource::PortForward { .. }
            | Resource::Booted { .. } => false,
        });
        Held {
            lease,
            holder,
            any_resource_alive,
        }
    });
    Ok(Facts {
        existing,
        now: now_rfc3339(),
        self_pid: std::process::id(),
    })
}

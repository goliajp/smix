//! Device settings a run changed and owes back.
//!
//! A run zeroes a device's animation scales before its first step. The
//! scales are the device's, not the run's: an app that reads
//! `animator_duration_scale == 0` as the user's reduce-motion
//! preference stayed in reduced motion on a consumer's emulator after
//! every run, because nothing wrote the scales back. Writing `1` back
//! is not the answer either — a fresh emulator has no
//! `animator_duration_scale` at all, and `1` is a state it never had.
//!
//! So what the device had is written down, per device and on the
//! machine, before it is changed, and put back verbatim when the last
//! run holding it leaves. Several runs may hold one device at once;
//! each registers itself, and the originals are those the first of
//! them saw. A run that dies by SIGKILL never leaves, which is why the
//! holders are process identities that can be probed: the next run
//! prunes the dead ones and, finding nobody left, takes over what they
//! recorded — the zeros the device shows at that moment are the dead
//! run's leftovers, not what the device had.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::ProcIdentity;
use crate::store::{LeaseError, check_device_id, machine_root};

/// A device's settings by key. `None` is a key the device does not
/// have, which is put back by deleting it, never by writing a default.
pub type Settings = BTreeMap<String, Option<String>>;

/// One device's recorded originals and the runs holding them changed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HeldSettings {
    /// Device these belong to.
    pub device_id: String,
    /// What the device had before the first holder changed it.
    pub originals: Settings,
    /// Every run that asked for the change and has not left.
    pub holders: Vec<ProcIdentity>,
}

/// Where a new holder takes the device's own settings from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Baseline {
    /// The record's originals: a holder is alive and the device shows
    /// its change, or every holder died without putting them back and
    /// the device still shows exactly what they wrote.
    Recorded,
    /// What the device shows now: nothing is recorded, or the record's
    /// holders are all dead and the device no longer shows their
    /// change, so somebody has set it since and that is the truth.
    Current,
}

/// Decide the baseline. Pure, so the SIGKILL case can be tested without
/// a device: `is_held` says whether one value is the one holders write.
pub fn baseline(
    record: Option<&HeldSettings>,
    any_holder_alive: bool,
    current: &Settings,
    is_held: impl Fn(&str) -> bool,
) -> Baseline {
    let Some(record) = record else {
        return Baseline::Current;
    };
    if any_holder_alive {
        return Baseline::Recorded;
    }
    // the walk below is over the recorded keys; with none recorded it
    // would be true of any device, so that case is said outright
    let still_held = !record.originals.is_empty()
        && record.originals.keys().all(|k| {
            current
                .get(k)
                .is_some_and(|v| v.as_deref().is_some_and(&is_held))
        });
    if still_held {
        Baseline::Recorded
    } else {
        Baseline::Current
    }
}

/// The directory holding one record per device.
///
/// A type for the same reason [`crate::store::LeaseDir`] is one: a bare
/// path to "the machine's data" is how device facts ended up in
/// checkouts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeldDir(PathBuf);

impl HeldDir {
    /// This machine's directory, `None` when the machine has no home
    /// for it — see [`machine_root`].
    #[must_use]
    pub fn machine() -> Option<Self> {
        machine_root().map(|r| Self(r.join("held-settings")))
    }

    /// A directory named outright, for a test pointing at a tempdir.
    pub fn at(path: impl Into<PathBuf>) -> Self {
        Self(path.into())
    }

    /// Where it is.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.0
    }
}

/// What joining decided for this process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Joined {
    /// No other holder is alive, so this process writes the change;
    /// a later holder only registers, the device already shows it.
    pub first: bool,
    /// What will be put back when the last holder leaves.
    pub originals: Settings,
}

fn io_err(path: &Path) -> impl FnOnce(std::io::Error) -> LeaseError + '_ {
    move |source| LeaseError::Io {
        path: path.display().to_string(),
        source,
    }
}

fn record_path(dir: &HeldDir, device_id: &str) -> Result<PathBuf, LeaseError> {
    check_device_id(device_id)?;
    Ok(dir.path().join(format!("{device_id}.json")))
}

/// Take the device's lock; released when the file drops. Blocking, as
/// the store's is: two runs starting on one device wait their turn.
fn lock(dir: &HeldDir, device_id: &str) -> Result<std::fs::File, LeaseError> {
    std::fs::create_dir_all(dir.path()).map_err(io_err(dir.path()))?;
    let path = dir.path().join(format!("{device_id}.lock"));
    let file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .map_err(io_err(&path))?;
    file.lock().map_err(io_err(&path))?;
    Ok(file)
}

fn read(path: &Path) -> Result<Option<HeldSettings>, LeaseError> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err(path)(e)),
    };
    serde_json::from_slice(&bytes)
        .map(Some)
        .map_err(|e| LeaseError::Malformed {
            path: path.display().to_string(),
            detail: e.to_string(),
        })
}

fn write(path: &Path, record: &HeldSettings) -> Result<(), LeaseError> {
    let json = serde_json::to_vec_pretty(record).map_err(|e| LeaseError::Malformed {
        path: path.display().to_string(),
        detail: e.to_string(),
    })?;
    let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::write(&tmp, json).map_err(io_err(&tmp))?;
    std::fs::rename(&tmp, path).map_err(io_err(path))
}

/// Register `me` as a holder of `device_id`'s settings.
///
/// `current` is what the device shows now, read before calling; it is
/// used only when nothing recorded stands (see [`baseline`]). `alive`
/// answers whether a recorded holder's process is still that process.
pub fn join(
    dir: &HeldDir,
    device_id: &str,
    me: ProcIdentity,
    current: &Settings,
    is_held: impl Fn(&str) -> bool,
    alive: impl Fn(&ProcIdentity) -> bool,
) -> Result<Joined, LeaseError> {
    let path = record_path(dir, device_id)?;
    let _lock = lock(dir, device_id)?;
    let record = read(&path)?;
    let mut holders: Vec<ProcIdentity> = record
        .iter()
        .flat_map(|r| r.holders.iter())
        .filter(|h| h.pid != me.pid && alive(h))
        .cloned()
        .collect();
    let first = holders.is_empty();
    let originals = match baseline(record.as_ref(), !first, current, is_held) {
        Baseline::Recorded => record.map(|r| r.originals).unwrap_or_default(),
        Baseline::Current => current.clone(),
    };
    holders.push(me);
    write(
        &path,
        &HeldSettings {
            device_id: device_id.to_string(),
            originals: originals.clone(),
            holders,
        },
    )?;
    Ok(Joined { first, originals })
}

/// The last holder alive, on its way out, with the device still to
/// put back.
///
/// Holds the device's lock and leaves the record on disk until
/// [`Self::restored`] is called. Dropped without it — the restore
/// failed, or the process died in the middle — the record stays, its
/// holders all dead, and the next run on the device finds the
/// originals there (see [`baseline`]). The lock keeps a run starting
/// at that moment from reading the device half-way through.
#[derive(Debug)]
pub struct LastOut {
    originals: Settings,
    path: PathBuf,
    _lock: std::fs::File,
}

impl LastOut {
    /// What to put back.
    #[must_use]
    pub fn originals(&self) -> &Settings {
        &self.originals
    }

    /// The device shows its originals again; the record is done with.
    pub fn restored(self) -> Result<(), LeaseError> {
        std::fs::remove_file(&self.path).map_err(io_err(&self.path))
    }
}

/// Remove `me` from the holders of `device_id`'s settings.
///
/// `Some` when nobody alive is left holding them, so the caller puts
/// them back and then says so. `None` when another holder remains, or
/// nothing was recorded.
pub fn leave(
    dir: &HeldDir,
    device_id: &str,
    me: &ProcIdentity,
    alive: impl Fn(&ProcIdentity) -> bool,
) -> Result<Option<LastOut>, LeaseError> {
    let path = record_path(dir, device_id)?;
    let lock = lock(dir, device_id)?;
    let Some(record) = read(&path)? else {
        return Ok(None);
    };
    let holders: Vec<ProcIdentity> = record
        .holders
        .into_iter()
        .filter(|h| h.pid != me.pid && alive(h))
        .collect();
    if holders.is_empty() {
        return Ok(Some(LastOut {
            originals: record.originals,
            path,
            _lock: lock,
        }));
    }
    write(
        &path,
        &HeldSettings {
            device_id: record.device_id,
            originals: record.originals,
            holders,
        },
    )?;
    Ok(None)
}

#[cfg(test)]
mod tests;

//! Devices that left without smix hearing about it.
//!
//! A ledger describes a device as present for as long as it exists. When
//! the device goes — an emulator that exits, a simulator shut down from
//! outside, a phone unplugged — the ledger goes on describing it, and
//! nothing says so. In the week of 2026-09-22 the user watched an Android
//! emulator exit abnormally three or four times and could not tell whose
//! it was or when: no crash report, an empty crash database, and ledgers
//! that still read as occupied.
//!
//! So the departure is kept as a fact, the first time smix touches the
//! devices afterwards: which device, when it was noticed, when the ledger
//! last heard from it, who held it, whether smix booted it, what holds
//! its slot now, and what it last printed on its console if smix started
//! it. Kept beside the ledgers, in the machine's directory, because it is
//! a fact about this machine and not about a checkout.

use serde::{Deserialize, Serialize};

use crate::store::{LeaseDir, LeaseError};
use crate::{Held, Lease, ProcIdentity, Resource};

/// The file the history lives in, inside the ledger directory.
///
/// `.jsonl`, not `.json`: a ledger is one `<device>.json`, and
/// [`LeaseDir::device_ids`] reads that suffix and nothing else.
pub const HISTORY_FILE: &str = "vanished.jsonl";

/// What the machine says is present right now.
#[derive(Debug, Clone, Default)]
pub struct LiveDevices {
    /// What `adb devices` listed, or `None` when adb could not be asked.
    pub adb: Option<Vec<AdbDevice>>,
    /// Every simulator `simctl` knows, with whether it is booted, or
    /// `None` when simctl could not be asked.
    pub simulators: Option<Vec<(String, bool)>>,
}

/// One device adb listed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AdbDevice {
    /// Its serial.
    pub serial: String,
    /// For an emulator, the AVD it answers as; `None` when it would not say.
    pub avd: Option<String>,
}

/// Whether the device a ledger is about is still here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Presence {
    /// It is.
    Present,
    /// It is not. `slot_now` names the AVD answering on its serial today,
    /// when that serial is an emulator port somebody else has taken.
    Gone {
        /// The AVD holding the serial now, if any.
        slot_now: Option<String>,
    },
    /// Nothing here can say.
    CannotTell,
}

/// Judge whether a ledger's device is still here. Pure.
#[must_use]
pub fn presence(lease: &Lease, live: &LiveDevices) -> Presence {
    let id = lease.device_id.as_str();
    if id.starts_with("emulator-") {
        let Some(adb) = &live.adb else {
            return Presence::CannotTell;
        };
        return match adb.iter().find(|d| d.serial == id) {
            None => Presence::Gone { slot_now: None },
            // Listed, but it would not say which AVD it is. Present is the
            // honest reading of "adb has a device on this serial"; which
            // device it is cannot be checked, so the recorded name is not
            // contradicted.
            Some(AdbDevice { avd: None, .. }) => Presence::Present,
            Some(AdbDevice { avd: Some(now), .. }) => match recorded_avd(lease) {
                Some(was) if was != now => Presence::Gone {
                    slot_now: Some(now.clone()),
                },
                _ => Presence::Present,
            },
        };
    }
    if is_apple_udid(id) {
        let Some(sims) = &live.simulators else {
            return Presence::CannotTell;
        };
        return match sims.iter().find(|(u, _)| u.eq_ignore_ascii_case(id)) {
            Some((_, true)) => Presence::Present,
            Some((_, false)) => Presence::Gone { slot_now: None },
            // A UDID simctl does not know is a phone; simctl cannot see
            // phones, so its silence is not an answer.
            None => Presence::CannotTell,
        };
    }
    // Anything else is an Android serial of a physical device.
    match &live.adb {
        None => Presence::CannotTell,
        Some(adb) if adb.iter().any(|d| d.serial == id) => Presence::Present,
        Some(_) => Presence::Gone { slot_now: None },
    }
}

/// The AVD an emulator ledger says it is about.
fn recorded_avd(lease: &Lease) -> Option<&str> {
    lease.known_resources().find_map(|r| match r {
        Resource::Emulator { avd, .. } => Some(avd.as_str()),
        _ => None,
    })
}

/// `8-4-4-4-12` hex (a simulator, or a phone before 2018) or `8-16` hex
/// (a phone since). The shape, not a lookup: this module is pure.
fn is_apple_udid(id: &str) -> bool {
    let groups: Vec<&str> = id.split('-').collect();
    let lens: Vec<usize> = groups.iter().map(|g| g.len()).collect();
    let hex = groups
        .iter()
        .all(|g| !g.is_empty() && g.chars().all(|c| c.is_ascii_hexdigit()));
    hex && (lens == [8, 4, 4, 4, 12] || lens == [8, 16])
}

/// One departure, as kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Vanished {
    /// RFC3339. When smix noticed.
    pub noticed_at: String,
    /// The serial or UDID the ledger was keyed by.
    pub device_id: String,
    /// The AVD, for an emulator whose ledger recorded one.
    pub avd: Option<String>,
    /// The AVD answering on the same serial when it was noticed.
    pub slot_now: Option<String>,
    /// Who held the device.
    pub holder: ProcIdentity,
    /// Whether that holder was still running when this was noticed.
    pub holder_alive: bool,
    /// RFC3339. When the ledger was taken.
    pub acquired_at: String,
    /// RFC3339. The last time the ledger heard from its holder.
    pub last_heartbeat: String,
    /// Whether smix booted it.
    pub booted_by_smix: bool,
    /// Where its console was written, when smix started it.
    pub console_log: Option<String>,
    /// The last lines of that console.
    pub console_tail: Vec<String>,
    /// The command that noticed.
    pub noticed_by: String,
}

/// Build the record for a ledger whose device is gone. Pure.
#[must_use]
pub fn vanished_from(
    held: &Held,
    slot_now: Option<String>,
    noticed_at: &str,
    noticed_by: &str,
    console_tail: Vec<String>,
) -> Vanished {
    let lease = &held.lease;
    let console_log = lease.known_resources().find_map(|r| match r {
        Resource::Emulator { console_log, .. } => console_log.clone(),
        _ => None,
    });
    Vanished {
        noticed_at: noticed_at.to_string(),
        device_id: lease.device_id.clone(),
        avd: recorded_avd(lease).map(str::to_string),
        slot_now,
        holder: lease.holder.clone(),
        holder_alive: held.holder.pid_exists && held.holder.identity_matches,
        acquired_at: lease.acquired_at.clone(),
        last_heartbeat: lease.heartbeat_at.clone(),
        booted_by_smix: lease
            .known_resources()
            .any(|r| matches!(r, Resource::Booted { by_us: true })),
        console_log,
        console_tail,
        noticed_by: noticed_by.to_string(),
    }
}

/// Keep a departure. `Ok(false)` when it was already kept.
///
/// One departure is one (device, lease) pair: the same ledger noticed by
/// the next ten commands is still one device leaving once. A new lease
/// on the same serial that later goes too is a second departure.
pub fn record(dir: &LeaseDir, v: &Vanished) -> Result<bool, LeaseError> {
    if history(dir)?
        .iter()
        .any(|k| k.device_id == v.device_id && k.acquired_at == v.acquired_at)
    {
        return Ok(false);
    }
    let path = dir.path().join(HISTORY_FILE);
    let io = |source| LeaseError::Io {
        path: path.display().to_string(),
        source,
    };
    std::fs::create_dir_all(dir.path()).map_err(io)?;
    let line = serde_json::to_string(v).map_err(|e| LeaseError::Io {
        path: path.display().to_string(),
        source: std::io::Error::other(e),
    })?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(io)?;
    use std::io::Write as _;
    writeln!(f, "{line}").map_err(io)?;
    Ok(true)
}

/// Every departure kept, oldest first.
///
/// No file is an empty history, not an error: nothing has left yet.
pub fn history(dir: &LeaseDir) -> Result<Vec<Vanished>, LeaseError> {
    let path = dir.path().join(HISTORY_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(source) => {
            return Err(LeaseError::Io {
                path: path.display().to_string(),
                source,
            });
        }
    };
    text.lines()
        .filter(|l| !l.trim().is_empty())
        .enumerate()
        .map(|(i, l)| {
            serde_json::from_str(l).map_err(|e| LeaseError::Malformed {
                path: format!("{}:{}", path.display(), i + 1),
                detail: e.to_string(),
            })
        })
        .collect()
}

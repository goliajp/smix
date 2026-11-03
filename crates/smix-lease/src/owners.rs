//! Whose a device's smix resources are: the task that asked for them.
//!
//! A lease names its holder, and the holder changes with every command —
//! a shell script's `smix sim boot` exits at once, and the run after it
//! holds the device next. So the lease can say "smix booted this" and
//! never "this task did". Several tasks on one machine need the second:
//! one cleaning up must not take another's simulator, and a machine at
//! its limit must be able to say who holds what.
//!
//! An owner is a name the caller gives (`--owner`, or `SMIX_OWNER` in the
//! environment), written beside the lease when a resource is recorded,
//! one row per kind of resource, and dropped with it. No name given is
//! recorded as nothing, not guessed.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::Resource;
use crate::store::{LeaseDir, LeaseError, check_device_id};

/// Where a command's owner comes from when `--owner` is not given.
pub const OWNER_ENV: &str = "SMIX_OWNER";

/// One kind of resource on a device, and whose it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OwnerRow {
    /// The resource's kind as the lease spells it: `runner`, `booted`, …
    pub kind: String,
    /// Who it is for.
    pub owner: String,
    /// When it was written down.
    pub recorded_at: String,
}

/// A device's owner rows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Owners {
    /// The device.
    pub device_id: String,
    /// One row per kind of resource that was given an owner.
    pub rows: Vec<OwnerRow>,
}

impl Owners {
    /// Every owner named on the device, each once, in the order written.
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        let mut out: Vec<&str> = Vec::new();
        for r in &self.rows {
            if !out.contains(&r.owner.as_str()) {
                out.push(&r.owner);
            }
        }
        out
    }
}

/// Whether `name` can be an owner: what a task would call itself, and safe
/// to print in a refusal. 1–64 of letters, digits, `.`, `_`, `-`, `:`.
pub fn validate(name: &str) -> Result<(), String> {
    let ok = (1..=64).contains(&name.len())
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | ':'));
    if ok {
        Ok(())
    } else {
        Err(format!(
            "owner {name:?}: 1 to 64 characters of letters, digits, `.`, `_`, `-` and `:`"
        ))
    }
}

/// The owner this process records, from [`OWNER_ENV`]. `None` when it is
/// unset or not a valid name — the command line checks it on the way in
/// and says so; here a bad one is simply not written.
#[must_use]
pub fn current() -> Option<String> {
    std::env::var(OWNER_ENV)
        .ok()
        .filter(|n| validate(n).is_ok())
}

fn dir_of(leases: &LeaseDir) -> PathBuf {
    leases.path().with_file_name("owners")
}

fn path(leases: &LeaseDir, device_id: &str) -> Result<PathBuf, LeaseError> {
    check_device_id(device_id)?;
    Ok(dir_of(leases).join(format!("{device_id}.json")))
}

/// The kind a resource is recorded under, as the lease file spells it.
#[must_use]
pub fn kind_of(resource: &Resource) -> String {
    serde_json::to_value(resource)
        .ok()
        .and_then(|v| v.get("kind").and_then(|k| k.as_str()).map(str::to_string))
        .unwrap_or_default()
}

/// Whether recording `resource` gives it this command's owner. A boot row
/// for a device somebody else turned on is not anyone's here; and a boot
/// row's owner is whoever turned the device on, so a later command that
/// found it up and re-states the row does not take it over —
/// [`note`] keeps the first owner of `booted`.
#[must_use]
pub fn takes_an_owner(resource: &Resource) -> bool {
    !matches!(resource, Resource::Booted { by_us: false })
}

/// Drop the owner rows whose resource is no longer in `lease`.
pub fn keep_only(leases: &LeaseDir, lease: &crate::Lease) -> Result<(), LeaseError> {
    let mut owners = read(leases, &lease.device_id)?;
    let before = owners.rows.len();
    let kinds: Vec<String> = lease.known_resources().map(kind_of).collect();
    owners.rows.retain(|r| kinds.contains(&r.kind));
    if owners.rows.len() == before {
        return Ok(());
    }
    write(leases, &owners)
}

/// A device's owner rows; empty when none were written.
pub fn read(leases: &LeaseDir, device_id: &str) -> Result<Owners, LeaseError> {
    let p = path(leases, device_id)?;
    match std::fs::read(&p) {
        Ok(bytes) => serde_json::from_slice(&bytes).map_err(|e| LeaseError::Malformed {
            path: p.display().to_string(),
            detail: e.to_string(),
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Owners {
            device_id: device_id.to_string(),
            rows: Vec::new(),
        }),
        Err(source) => Err(LeaseError::Io {
            path: p.display().to_string(),
            source,
        }),
    }
}

fn write(leases: &LeaseDir, owners: &Owners) -> Result<(), LeaseError> {
    let p = path(leases, &owners.device_id)?;
    let io = |path: &std::path::Path| {
        let path = path.display().to_string();
        move |source: std::io::Error| LeaseError::Io { path, source }
    };
    if owners.rows.is_empty() {
        return match std::fs::remove_file(&p) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => Err(io(&p)(e)),
            _ => Ok(()),
        };
    }
    let dir = dir_of(leases);
    std::fs::create_dir_all(&dir).map_err(io(&dir))?;
    let json = serde_json::to_vec_pretty(owners).map_err(|e| LeaseError::Malformed {
        path: p.display().to_string(),
        detail: e.to_string(),
    })?;
    let tmp = dir.join(format!(".{}.{}.tmp", owners.device_id, std::process::id()));
    std::fs::write(&tmp, json).map_err(io(&tmp))?;
    std::fs::rename(&tmp, &p).map_err(io(&p))
}

/// Write down that `kind` on `device_id` is `owner`'s, replacing whoever
/// had that kind before.
pub fn note(
    leases: &LeaseDir,
    device_id: &str,
    kind: &str,
    owner: &str,
    at: &str,
) -> Result<(), LeaseError> {
    let mut owners = read(leases, device_id)?;
    owners.device_id = device_id.to_string();
    if kind == "booted" && owners.rows.iter().any(|r| r.kind == kind) {
        return Ok(());
    }
    owners.rows.retain(|r| r.kind != kind);
    owners.rows.push(OwnerRow {
        kind: kind.to_string(),
        owner: owner.to_string(),
        recorded_at: at.to_string(),
    });
    write(leases, &owners)
}

/// Drop the owner of `kind` on `device_id`.
pub fn forget_kind(leases: &LeaseDir, device_id: &str, kind: &str) -> Result<(), LeaseError> {
    let mut owners = read(leases, device_id)?;
    owners.rows.retain(|r| r.kind != kind);
    write(leases, &owners)
}

/// Drop every owner row of `device_id`.
pub fn forget(leases: &LeaseDir, device_id: &str) -> Result<(), LeaseError> {
    write(
        leases,
        &Owners {
            device_id: device_id.to_string(),
            rows: Vec::new(),
        },
    )
}

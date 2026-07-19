//! The device registry — deterministic device addressing.
//!
//! Records live in the `smix-store` under `.smix/`. A pre-store
//! `sims.json` sitting beside it is imported on open and then left
//! alone; smix never writes that file again.
//!
//! Every smix device operation targets either an explicit UDID or an
//! alias recorded in this file. Resolution never consults the live
//! simulator set: the registry file is the only mapping source, so a
//! given input always resolves to the same device regardless of what
//! happens to be booted on the machine.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Failure variants for registry load / device-ref resolution.
#[derive(Debug, Error)]
pub enum RegistryError {
    /// Registry file could not be read.
    #[error("cannot read sim registry {path}: {source}")]
    Io {
        /// Path that failed to read.
        path: String,
        /// Underlying I/O error.
        source: std::io::Error,
    },
    /// Registry file is not valid registry JSON.
    #[error("malformed sim registry {path}: {detail}")]
    Malformed {
        /// Path that failed to parse.
        path: String,
        /// Parser-side detail.
        detail: String,
    },
    /// Input is neither a UDID nor a recorded alias.
    #[error(
        "unknown device ref {device_ref:?} — pass an explicit UDID or one of \
         the recorded aliases: {}",
        known.join(", ")
    )]
    UnknownDevice {
        /// The input that failed to resolve.
        device_ref: String,
        /// Alias keys and device names available in the registry.
        known: Vec<String>,
    },
}

/// One registered simulator.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisteredSim {
    /// Human-chosen device name (also usable as an alias).
    #[serde(rename = "deviceName")]
    pub device_name: String,
    /// CoreSimulator UDID.
    pub udid: String,
    /// Runtime identifier.
    pub runtime: String,
    /// Device type identifier.
    #[serde(rename = "deviceType")]
    pub device_type: String,
    /// Desired BCP 47 locale tag (e.g. `"en-US"`, `"ja-JP"`). When set,
    /// `smix sim boot` enforces it via
    /// `defaults write -g AppleLanguages + AppleLocale` and reboots the
    /// sim if the current locale differs. `None` (field absent) =
    /// honor whatever locale the sim boots with, no enforcement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub locale: Option<String>,
    /// Desired runner port (SmixRunner FlyingFox HTTP port). When set,
    /// `smix runner up <alias>` binds the runner to this port instead
    /// of the CLI default 22087. Two sims can then run their own runner
    /// in parallel without port collision
    /// (e.g. `sim-a.runnerPort = 22087` + `sim-b.runnerPort = 22088`).
    /// Falls through to `--runner-port` flag or `SMIX_RUNNER_PORT` env
    /// when absent.
    #[serde(
        default,
        rename = "runnerPort",
        skip_serializing_if = "Option::is_none"
    )]
    pub runner_port: Option<u16>,
}

/// What [`SimRegistry::register`] did with the alias.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegisterOutcome {
    /// The alias did not exist; a new row was written.
    Added,
    /// The alias existed; its row was replaced.
    Updated,
}

/// Loaded view of the registry, keyed by alias.
#[derive(Debug)]
pub struct SimRegistry {
    sims: BTreeMap<String, RegisteredSim>,
}

/// Whether `s` has CoreSimulator UDID form (8-4-4-4-12 hex).
///
/// UDID-form input is always treated as a deliberate instruction and
/// never requires registry membership.
pub fn is_udid(s: &str) -> bool {
    let bytes = s.as_bytes();
    if bytes.len() != 36 {
        return false;
    }
    for (i, b) in bytes.iter().enumerate() {
        match i {
            8 | 13 | 18 | 23 => {
                if *b != b'-' {
                    return false;
                }
            }
            _ => {
                if !b.is_ascii_hexdigit() {
                    return false;
                }
            }
        }
    }
    true
}

/// Resolve a caller-supplied path to the `.smix` directory holding the
/// store.
///
/// Callers pass either form: `SMIX_SIMS_JSON` documents a file, while
/// discovery yields a directory. Accepting both is also what lets the
/// pre-store test suite keep exercising the legacy import path
/// unchanged, instead of being rewritten into agreement with the code
/// it is supposed to check.
pub fn store_dir(path: &Path) -> PathBuf {
    smix_dir(path)
}

fn smix_dir(path: &Path) -> PathBuf {
    if path.extension().is_some_and(|e| e == "json") {
        path.parent().unwrap_or(path).to_path_buf()
    } else {
        path.to_path_buf()
    }
}

/// Open the store under `path` and fold in any legacy `sims.json`
/// sitting beside it.
///
/// The legacy file is read, never written and never removed: a user who
/// has to go back to a pre-store smix must still find their registry.
fn open_store(path: &Path) -> Result<smix_store::Store, RegistryError> {
    let dir = smix_dir(path);
    std::fs::create_dir_all(&dir).map_err(|source| RegistryError::Io {
        path: dir.display().to_string(),
        source,
    })?;
    let store = smix_store::Store::open(&dir).map_err(|e| RegistryError::Io {
        path: dir.display().to_string(),
        source: std::io::Error::other(e.to_string()),
    })?;
    let legacy = dir.join("sims.json");
    smix_store::import_legacy_records(&store.sims(), &legacy, "sims").map_err(|e| {
        RegistryError::Malformed {
            path: legacy.display().to_string(),
            detail: e.to_string(),
        }
    })?;
    Ok(store)
}

impl SimRegistry {
    /// Write `sim` into the registry under `alias`.
    ///
    /// One key, not a whole file. The read-modify-write this replaces
    /// lost an alias whenever two processes registered at once — each
    /// read the file, each inserted its own row, and the second write
    /// erased the first, with no error on either side.
    pub fn register(
        path: &Path,
        alias: &str,
        sim: RegisteredSim,
    ) -> Result<RegisterOutcome, RegistryError> {
        let store = open_store(path)?;
        let existed = store
            .sims()
            .get(alias)
            .map_err(|e| RegistryError::Malformed {
                path: path.display().to_string(),
                detail: e.to_string(),
            })?
            .is_some();
        store
            .sims()
            .put_json(alias, &sim)
            .map_err(|e| RegistryError::Io {
                path: path.display().to_string(),
                source: std::io::Error::other(e.to_string()),
            })?;
        store.sync().map_err(|e| RegistryError::Io {
            path: path.display().to_string(),
            source: std::io::Error::other(e.to_string()),
        })?;
        Ok(if existed {
            RegisterOutcome::Updated
        } else {
            RegisterOutcome::Added
        })
    }

    /// Read every registered sim.
    ///
    /// `path` may be the `.smix` directory or a legacy `sims.json`
    /// inside it; both land on the same store.
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let store = open_store(path)?;
        let mut sims = BTreeMap::new();
        for alias in store.sims().list() {
            let sim: RegisteredSim = store
                .sims()
                .get_json(&alias)
                .map_err(|e| RegistryError::Malformed {
                    path: path.display().to_string(),
                    detail: e.to_string(),
                })?
                .ok_or_else(|| RegistryError::Malformed {
                    path: path.display().to_string(),
                    detail: format!("`{alias}` vanished between listing and reading"),
                })?;
            sims.insert(alias, sim);
        }
        Ok(Self { sims })
    }

    /// Walk up from `start` looking for a `.smix` that holds a
    /// registry — either the store or a legacy `sims.json`.
    pub fn discover(start: &Path) -> Option<PathBuf> {
        let mut dir = Some(start);
        while let Some(d) = dir {
            let smix = d.join(".smix");
            if smix.join("sims.json").is_file() || smix.join("kv").is_dir() {
                return Some(smix);
            }
            dir = d.parent();
        }
        None
    }

    /// Resolve a device ref to a UDID.
    ///
    /// An explicit UDID passes through (normalized to uppercase) whether
    /// or not it is registered — UDID-form input is always a deliberate
    /// instruction. Otherwise the ref must match an alias key or a
    /// `deviceName` exactly; anything else errors listing what exists.
    pub fn resolve(&self, device_ref: &str) -> Result<String, RegistryError> {
        if is_udid(device_ref) {
            return Ok(device_ref.to_ascii_uppercase());
        }
        if let Some(sim) = self.sims.get(device_ref) {
            return Ok(sim.udid.to_ascii_uppercase());
        }
        if let Some(sim) = self.sims.values().find(|s| s.device_name == device_ref) {
            return Ok(sim.udid.to_ascii_uppercase());
        }
        let mut known: Vec<String> = Vec::with_capacity(self.sims.len() * 2);
        for (alias, sim) in &self.sims {
            known.push(alias.clone());
            known.push(sim.device_name.clone());
        }
        Err(RegistryError::UnknownDevice {
            device_ref: device_ref.to_string(),
            known,
        })
    }

    /// All registered sims, keyed by alias.
    pub fn sims(&self) -> &BTreeMap<String, RegisteredSim> {
        &self.sims
    }

    /// Look up a [`RegisteredSim`] by alias key, device name, or UDID.
    /// Returns `None` if no entry matches any of the three. Mirrors
    /// [`Self::resolve`]'s match precedence so cli callers can fetch
    /// the full spec (e.g. `locale` field) after they already resolved
    /// the UDID.
    pub fn lookup(&self, device_ref: &str) -> Option<&RegisteredSim> {
        if let Some(sim) = self.sims.get(device_ref) {
            return Some(sim);
        }
        self.sims
            .values()
            .find(|sim| sim.device_name == device_ref || sim.udid.eq_ignore_ascii_case(device_ref))
    }
}

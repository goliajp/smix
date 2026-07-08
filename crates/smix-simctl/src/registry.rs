//! `.smix/sims.json` device registry — deterministic device addressing.
//!
//! Every smix device operation targets either an explicit UDID or an
//! alias recorded in this file. Resolution never consults the live
//! simulator set: the registry file is the only mapping source, so a
//! given input always resolves to the same device regardless of what
//! happens to be booted on the machine.

use serde::Deserialize;
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

/// One registered simulator (a row of `.smix/sims.json`).
#[derive(Debug, Clone, Deserialize)]
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
    #[serde(default)]
    pub locale: Option<String>,
    /// Desired runner port (SmixRunner FlyingFox HTTP port). When set,
    /// `smix runner up <alias>` binds the runner to this port instead
    /// of the CLI default 22087. Two sims can then run their own runner
    /// in parallel without port collision
    /// (e.g. `sim-a.runnerPort = 22087` + `sim-b.runnerPort = 22088`).
    /// Falls through to `--runner-port` flag or `SMIX_RUNNER_PORT` env
    /// when absent.
    #[serde(default, rename = "runnerPort")]
    pub runner_port: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct RegistryFile {
    #[allow(dead_code)]
    version: u32,
    sims: BTreeMap<String, RegisteredSim>,
}

/// Loaded view of `.smix/sims.json`, keyed by alias.
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

impl SimRegistry {
    /// Parse the registry file at `path`.
    pub fn load(path: &Path) -> Result<Self, RegistryError> {
        let text = std::fs::read_to_string(path).map_err(|source| RegistryError::Io {
            path: path.display().to_string(),
            source,
        })?;
        let file: RegistryFile =
            serde_json::from_str(&text).map_err(|e| RegistryError::Malformed {
                path: path.display().to_string(),
                detail: e.to_string(),
            })?;
        Ok(Self { sims: file.sims })
    }

    /// Walk up from `start` looking for `.smix/sims.json`.
    pub fn discover(start: &Path) -> Option<PathBuf> {
        let mut dir = Some(start);
        while let Some(d) = dir {
            let candidate = d.join(".smix/sims.json");
            if candidate.is_file() {
                return Some(candidate);
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

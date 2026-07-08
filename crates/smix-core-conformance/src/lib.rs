#![doc = include_str!("../README.md")]
#![deny(missing_docs)]

//! smix-core-conformance — cross-SDK conformance harness lib.
//!
//! Provides [`Fixture`] type + [`load_fixture`] loader for use by both
//! the binary `fixture-runner` and per-backend tests.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

/// One conformance fixture loaded from `fixtures/<id>.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fixture {
    /// Unique fixture id (filename root).
    pub id: String,
    /// One-line human description.
    pub description: String,
    /// A11yNode wire JSON form (kept as `Value` so this crate stays
    /// pure to wire-level — the FFI backend re-serializes when calling).
    pub tree: Value,
    /// Selector wire JSON form (same rationale).
    pub selector: Value,
    /// Expected resolved node ids (`Vec<String>`).
    pub expected: Vec<String>,
}

/// Load a fixture by id from `crates/smix-core-conformance/fixtures/<id>-*.json`.
///
/// The directory is resolved relative to the workspace root via the
/// `CARGO_MANIFEST_DIR` env baked into the binary/test at compile time.
///
/// # Errors
/// Returns an [`anyhow::Error`] when the fixture file does not exist or
/// fails JSON parse.
pub fn load_fixture(id: &str) -> anyhow::Result<Fixture> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let fixtures_dir = PathBuf::from(manifest_dir).join("fixtures");
    let entries = std::fs::read_dir(&fixtures_dir)?;
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        // accept either exact `<id>` or `<id>-<rest>` filename root
        if stem == id || stem.starts_with(&format!("{id}-")) {
            let body = std::fs::read_to_string(&path)?;
            let fixture: Fixture = serde_json::from_str(&body)?;
            return Ok(fixture);
        }
    }
    Err(anyhow::anyhow!(
        "fixture id '{id}' not found in {fixtures_dir:?}"
    ))
}

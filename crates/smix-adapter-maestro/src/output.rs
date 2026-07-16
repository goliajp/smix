//! Canonical file-write helper for yaml output verbs.
//!
//! # Reviewer invariant
//!
//! Any new verb that writes to a yaml-specified path MUST go through
//! [`write_yaml_output`], not inline `std::fs::write`. Grep this
//! crate for `fs::write` — every non-test hit should be in this file.
//!
//! # Policy layers this helper implements
//!
//! 1. **Path preparation** — [`fs::create_dir_all`] on the parent so
//!    yaml paths like `.devtools/screenshots/hub-form.png` work in a
//!    fresh cwd without pre-mkdir'ing.
//! 2. **Extension convention** — when the yaml path has no extension,
//!    infer one from magic bytes (`89 50 4E 47…` → `.png`, `FF D8 FF`
//!    → `.jpg`). Matches Maestro semantics; downstream consumers can
//!    still filter by extension safely.
//! 3. **Severity tiering** — [`OutputIntent`] declares whether the
//!    caller treats the write as load-bearing (write failure ⇒
//!    `runOutcome: failure` propagated via `RunError::Io`) or
//!    best-effort (write failure ⇒ warning only, run continues).
//!    This closes a silent-success hazard when `takeScreenshot` fell
//!    into #2 or #3 and warned without failing.
//!
//! # Consumers
//!
//! - [`Step::TakeScreenshot`] — `LoadBearing`
//! - [`Step::StartRecording`] — `LoadBearing` (once the recording
//!   path becomes a required arg vs the current mp4 dispatch)
//! - `AssertScreenshot` auto-baseline record path — `LoadBearing`
//! - `write_step_debug` (per-step JSON + fail screenshot) —
//!   `BestEffort`

use std::io;
use std::path::{Path, PathBuf};

/// Severity of a write failure. See module doc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputIntent {
    /// The verb's whole point is producing this file — e.g.
    /// takeScreenshot / assertScreenshot baseline record. Write
    /// failures must not be swallowed.
    LoadBearing,
    /// The verb succeeded; this file is a debug / observability side-
    /// effect. Write failures warn but don't fail the step.
    BestEffort,
}

/// Write `bytes` to `path` following the [`OutputIntent`] policy.
///
/// Returns the actual path used (may differ from input via extension
/// inference — see module doc). `LoadBearing` errors surface via
/// [`io::Error`]; callers convert to their crate's error type.
///
/// `BestEffort` still returns `Err` on write failure; callers on that
/// tier catch and add to their warnings vec (see [`write_yaml_output_lenient`]
/// convenience wrapper).
pub fn write_yaml_output(
    path: &Path,
    bytes: &[u8],
    _intent: OutputIntent,
) -> Result<PathBuf, io::Error> {
    let inferred = infer_extension(path, bytes);
    if let Some(parent) = inferred.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&inferred, bytes)?;
    Ok(inferred)
}

/// `BestEffort`-tier convenience: swallow the error into a caller-
/// supplied warnings vec, returning `None` when the write failed.
pub fn write_yaml_output_lenient(
    path: &Path,
    bytes: &[u8],
    warnings: &mut Vec<String>,
) -> Option<PathBuf> {
    match write_yaml_output(path, bytes, OutputIntent::BestEffort) {
        Ok(p) => Some(p),
        Err(e) => {
            warnings.push(format!(
                "write {path:?}: {e}; {} bytes captured but not persisted",
                bytes.len()
            ));
            None
        }
    }
}

/// Infer a file extension when the caller-supplied path has none.
///
/// Recognized magic bytes: PNG, JPEG. Any other byte
/// sequence + missing extension → return path unchanged (caller
/// wrote a raw dump).
fn infer_extension(path: &Path, bytes: &[u8]) -> PathBuf {
    if path.extension().is_some() {
        return path.to_path_buf();
    }
    let ext = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(b"\xFF\xD8\xFF") {
        "jpg"
    } else {
        return path.to_path_buf();
    };
    path.with_extension(ext)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\nrestofpng";
    const JPG_MAGIC: &[u8] = b"\xFF\xD8\xFFrestofjpg";

    #[test]
    fn write_creates_missing_parent_dir() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("sub/dir/screenshot.png");
        let out = write_yaml_output(&path, PNG_MAGIC, OutputIntent::LoadBearing).unwrap();
        assert_eq!(out, path);
        assert!(path.exists());
    }

    #[test]
    fn png_extension_auto_appended_when_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("sub/hub-form"); // no ext
        let out = write_yaml_output(&path, PNG_MAGIC, OutputIntent::LoadBearing).unwrap();
        assert_eq!(out, path.with_extension("png"));
        assert!(out.exists());
    }

    #[test]
    fn jpg_extension_auto_appended_when_missing() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("frame");
        let out = write_yaml_output(&path, JPG_MAGIC, OutputIntent::LoadBearing).unwrap();
        assert_eq!(out, path.with_extension("jpg"));
    }

    #[test]
    fn existing_extension_preserved() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("shot.png"); // has ext
        let out = write_yaml_output(&path, PNG_MAGIC, OutputIntent::LoadBearing).unwrap();
        assert_eq!(out, path);
    }

    #[test]
    fn unknown_bytes_and_no_ext_write_path_verbatim() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("raw-blob"); // no ext, no known magic
        let out = write_yaml_output(&path, b"raw", OutputIntent::LoadBearing).unwrap();
        assert_eq!(out, path);
    }

    #[test]
    fn write_error_surfaces_when_parent_write_denied() {
        // We can't reliably chmod on all CIs, so verify the error
        // path via a genuinely-unwritable target — the input path
        // pointing at a filename we've written as a REGULAR file
        // (not a directory).
        let tmp = TempDir::new().unwrap();
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, b"").unwrap();
        // Now try to write to blocker/something → parent "blocker" is
        // a file, not a dir. create_dir_all fails.
        let path = blocker.join("under-blocker.png");
        let err = write_yaml_output(&path, PNG_MAGIC, OutputIntent::LoadBearing);
        assert!(err.is_err());
    }

    #[test]
    fn lenient_swallows_error_and_pushes_warning() {
        let tmp = TempDir::new().unwrap();
        let blocker = tmp.path().join("blocker");
        std::fs::write(&blocker, b"").unwrap();
        let path = blocker.join("nope.png");
        let mut warnings = Vec::new();
        let out = write_yaml_output_lenient(&path, PNG_MAGIC, &mut warnings);
        assert!(out.is_none());
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("bytes captured but not persisted"));
    }
}

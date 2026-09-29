//! Reading and writing `defaults` inside a simulator: which spelling of the
//! program starts, and what its answers mean.

use crate::DeviceControlError;

/// How `defaults` is named inside a simulator, in the order tried.
///
/// Neither spelling works on every runtime. Earlier runtimes refused the
/// bare name — `simctl spawn` runs no login shell, and it exited 255 with
/// nothing on stderr — so every call site was written with the absolute
/// path. Under Xcode 27 `simctl spawn` reads a path that starts with `/`
/// from the host's root, not the runtime's — the runtime still has
/// `usr/bin/defaults` — so the absolute path fails to start (111, "Invalid
/// or missing Program"; measured 2026-09-25), while the bare name runs.
/// Tried in order; only a spelling that did not start moves on to the next.
pub const DEFAULTS_PROGRAMS: &[&str] = &["defaults", "/usr/bin/defaults"];

/// Whether a `simctl spawn` failure means the program never started, as
/// opposed to the program running and answering with a failure. Only the
/// first is fixed by spelling the program another way.
#[must_use]
pub fn spawn_did_not_start(code: i32, stderr: &str) -> bool {
    stderr.contains("Invalid or missing Program") || (code == 255 && stderr.trim().is_empty())
}

pub(crate) fn did_not_start(e: &DeviceControlError) -> bool {
    matches!(e, DeviceControlError::NonZeroExit { code, stderr, .. } if spawn_did_not_start(*code, stderr))
}

/// Whether `defaults` itself said the key (or its domain) is not there —
/// its own words, not any non-zero exit. A spawn that never started also
/// exits non-zero, and reading that as "unset" turned a broken read into a
/// confident "no".
///
/// Two generations of wording: up to iOS 26 `The domain/default pair of
/// (D, K) does not exist`; from iOS 27 (and macOS 27) `Could not find key
/// 'K' in domain 'D'.` or `Domain 'D' not found.`, for read and delete
/// alike.
#[must_use]
pub fn defaults_key_is_absent(stderr: &str) -> bool {
    stderr.contains("does not exist")
        || (stderr.contains("Could not find key '") && stderr.contains("' in domain '"))
        || (stderr.contains("Domain '") && stderr.contains("' not found."))
}

/// `defaults read` of a boolean: `1` or `0` on a line of its own.
/// Anything else is not an answer.
#[must_use]
pub fn parse_defaults_bool(out: &str) -> Option<bool> {
    match out.trim() {
        "1" => Some(true),
        "0" => Some(false),
        _ => None,
    }
}

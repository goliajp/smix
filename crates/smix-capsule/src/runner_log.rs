//! Where a recorded iOS runner's output is going.
//!
//! The ledger row carries the path, but the ledger is shared by every smix
//! on the machine, and one that predates the field rewrites the row without
//! it. The process still knows: its stdout is the file. So a row without a
//! path is answered by asking the runner process, and refused only when the
//! process cannot say.

use std::path::{Path, PathBuf};

use crate::runner::RunnerState;

/// The file the runner writes its output to.
pub(crate) fn runner_output(st: &RunnerState) -> Result<PathBuf, String> {
    if let Some(log) = &st.log {
        return Ok(log.clone());
    }
    let out = std::process::Command::new("lsof")
        .args([
            "-b",
            "-w",
            "-nP",
            "-a",
            "-p",
            &st.pid.to_string(),
            "-d",
            "1",
            "-Fn",
        ])
        .output()
        .map_err(|e| format!("lsof: {e}"))?;
    stdout_file(&String::from_utf8_lossy(&out.stdout)).ok_or_else(|| {
        format!(
            "the runner on port {port} has no log path in its record — a smix older than \
             this one rewrote the row without it — and its process (pid {pid}) is not \
             writing to a file smix can name; `smix runner down --runner-port {port}` and \
             `smix runner up {udid} --bundle <id>` start one whose record carries it",
            port = st.port,
            pid = st.pid,
            udid = st.udid,
        )
    })
}

/// The regular file named on descriptor 1 in `lsof -Fn` output.
///
/// A pipe or socket is named `->0x…` and a terminal or `/dev/null` is a
/// device: none of them is a log anyone can tail.
fn stdout_file(lsof_fn: &str) -> Option<PathBuf> {
    let name = lsof_fn.lines().find_map(|l| l.strip_prefix('n'))?;
    let path = Path::new(name);
    (path.is_absolute() && !name.starts_with("/dev/")).then(|| path.to_path_buf())
}

/// The last `lines` lines of a runner log, or nothing when it cannot be read.
pub(crate) fn tail(log: &Path, lines: usize) -> String {
    let Ok(text) = std::fs::read_to_string(log) else {
        return String::new();
    };
    let all: Vec<&str> = text.lines().collect();
    let start = all.len().saturating_sub(lines);
    all[start..].join("\n")
}

/// What to say when xcodebuild exits before the runner is up: the cause,
/// then the last `lines` lines.
///
/// XCTest's automatic restart after a failed `test_runForever` writes
/// twenty-odd lines of "executed 0 tests" after the failure, so the tail
/// alone never shows why it failed. The test's own `error:` lines are
/// brought up from wherever they are in the log.
pub(crate) fn early_exit(log: &Path, lines: usize) -> String {
    let text = std::fs::read_to_string(log).unwrap_or_default();
    let tail = tail(log, lines);
    match cause(&text) {
        Some(cause) => format!("{cause}\n\nlog tail:\n{tail}"),
        None => format!("log tail:\n{tail}"),
    }
}

/// The failures a runner's test reported, with a plain sentence first when
/// the reason is that the app is not installed.
fn cause(text: &str) -> Option<String> {
    let errors: Vec<&str> = text
        .lines()
        .filter(|l| l.contains(": error: -[") && l.contains("test_runForever"))
        .collect();
    let first = errors.first()?;
    // what iOS answers when no app with that bundle id is on the device
    let missing = first
        .split_once("Failed to launch ")
        .and_then(|(_, rest)| rest.split_once(": "))
        .map(|(bundle, _)| bundle)
        .filter(|_| first.contains("returned nil for"));
    let said = errors.join("\n");
    Some(match missing {
        Some(bundle) => format!(
            "{bundle} is not installed on this simulator, so the runner could not \
             launch it. Install the app, then run `smix runner up` again.\n{said}"
        ),
        None => said,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const MISSING: &str = "/x/SmixRunnerUITests.swift:903: error: -[SmixRunnerUITests.SmixRunnerUITests test_runForever] : Failed to launch com.example.app: The request to open \"com.example.app\" failed. (Underlying Error: The operation couldn\u{2019}t be completed. Application info provider (FBSApplicationLibrary) returned nil for \"com.example.app\")";

    fn log_of(text: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("runner.log");
        std::fs::write(&log, text).unwrap();
        (dir, log)
    }

    #[test]
    fn an_app_that_is_not_installed_is_named_above_the_restart_noise() {
        let noise = "\tExecuted 0 tests, with 0 failures\n".repeat(30);
        let (_d, log) = log_of(&format!(
            "Launch com.example.app\n{MISSING}\n{noise}** TEST FAILED **\n"
        ));
        let said = early_exit(&log, 25);
        assert!(
            said.starts_with("com.example.app is not installed on this simulator"),
            "{said}"
        );
        assert!(said.contains("returned nil for"), "{said}");
        assert!(said.contains("** TEST FAILED **"), "{said}");
    }

    #[test]
    fn another_failure_is_brought_up_as_it_was_written() {
        let other = "/x/T.swift:1: error: -[T test_runForever] : something else broke";
        let noise = "\tExecuted 0 tests\n".repeat(30);
        let (_d, log) = log_of(&format!("{other}\n{noise}"));
        let said = early_exit(&log, 25);
        assert!(said.starts_with(other), "{said}");
        assert!(!said.contains("not installed"), "{said}");
    }

    #[test]
    fn a_log_without_a_test_failure_is_only_its_tail() {
        let (_d, log) = log_of("line one\nline two\n");
        assert_eq!(early_exit(&log, 25), "log tail:\nline one\nline two");
    }

    #[test]
    fn a_file_on_stdout_is_the_log() {
        let out = "p64844\nf1\nn/Users/me/smix/.smix/runner/runner-UDID.log\n";
        assert_eq!(
            stdout_file(out),
            Some(PathBuf::from("/Users/me/smix/.smix/runner/runner-UDID.log"))
        );
    }

    #[test]
    fn a_pipe_a_device_or_nothing_is_not_a_log() {
        assert_eq!(stdout_file("p1\nf1\nn->0x5f1c2a3b\n"), None);
        assert_eq!(stdout_file("p1\nf1\nn/dev/null\n"), None);
        assert_eq!(stdout_file("p1\nf1\nn/dev/ttys003\n"), None);
        assert_eq!(stdout_file(""), None);
    }
}

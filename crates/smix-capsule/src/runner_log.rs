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

#[cfg(test)]
mod tests {
    use super::*;

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

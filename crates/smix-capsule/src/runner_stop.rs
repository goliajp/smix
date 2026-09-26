//! Stopping an `xcodebuild` session this crate started, and reading back
//! that it is gone.

use std::time::Duration;

/// Whether `pid` is still a running process. A zombie is not: it has
/// exited and only waits for a parent to reap it, which a long-lived
/// host such as the MCP server may never do for a child it let go of.
pub(crate) fn running(pid: u32) -> bool {
    let Ok(out) = std::process::Command::new("ps")
        .args(["-o", "stat=", "-p", &pid.to_string()])
        .output()
    else {
        return true;
    };
    let stat = String::from_utf8_lossy(&out.stdout);
    let stat = stat.trim();
    !stat.is_empty() && !stat.starts_with('Z')
}

fn gone_within(pid: u32, limit: Duration) -> bool {
    let deadline = std::time::Instant::now() + limit;
    while running(pid) {
        if std::time::Instant::now() >= deadline {
            return false;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    true
}

/// Interrupt `pid`, give it `grace` to wind down, kill it if it has not,
/// and return only once it is no longer running.
///
/// The caller drops its record of the session after this returns, so an
/// `Ok` must mean the process is gone: a record dropped while the process
/// lives leaves a runner nobody can find to stop.
pub(crate) fn stop(pid: u32, grace: Duration) -> Result<(), String> {
    crate::runner::signal(pid, "-INT");
    if gone_within(pid, grace) {
        return Ok(());
    }
    // xcodebuild answers an interrupt by collecting simulator diagnostics
    // first, which on a loaded machine can run for its own ten-minute limit.
    eprintln!(
        "warning: pid {pid} still running {}s after SIGINT — escalating to \
         SIGKILL (expect a macOS crash-report dialog from the runner app)",
        grace.as_secs()
    );
    crate::runner::signal(pid, "-9");
    if gone_within(pid, Duration::from_secs(5)) {
        return Ok(());
    }
    Err(format!(
        "pid {pid} is still running after SIGINT and SIGKILL — inspect \
         `ps -o pid,stat,command -p {pid}`"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_process_that_ignores_the_interrupt_is_gone_when_stop_returns() {
        let mut child = std::process::Command::new("/bin/sh")
            .args(["-c", "trap '' INT; exec sleep 30"])
            .spawn()
            .unwrap();
        let pid = child.id();
        std::thread::sleep(Duration::from_millis(200));
        assert!(running(pid), "the subject must be running before the stop");

        stop(pid, Duration::from_millis(500)).unwrap();

        assert!(
            !running(pid),
            "pid {pid} still running after stop returned Ok"
        );
        child.wait().unwrap();
    }

    #[test]
    fn an_exited_child_nobody_reaped_is_not_running() {
        let child = std::process::Command::new("/bin/sh")
            .args(["-c", "exit 0"])
            .spawn()
            .unwrap();
        let pid = child.id();
        std::thread::sleep(Duration::from_millis(300));
        let stat = std::process::Command::new("ps")
            .args(["-o", "stat=", "-p", &pid.to_string()])
            .output()
            .unwrap();
        assert!(
            String::from_utf8_lossy(&stat.stdout)
                .trim()
                .starts_with('Z'),
            "the subject must be an unreaped zombie for this to test anything"
        );

        assert!(!running(pid));
        drop(child);
    }
}

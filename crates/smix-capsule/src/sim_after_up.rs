//! What `runner up` does to a simulator once its runner is up.

use std::path::Path;

use smix_adb::{CrashReport, crash_loops_at};
use smix_simctl::crash_report;

use crate::runner::RunnerTarget;

/// On a simulator, keep XCUITest's query log out of the app's log, and
/// say so when a process there is crashing again and again. Neither stops
/// the runner: it reads the same either way.
pub(crate) fn settle(target: RunnerTarget<'_>, udid: &str) {
    if !matches!(target, RunnerTarget::Simulator) {
        return;
    }
    crate::xctest_log::quiet(udid);
    let Some(dir) = crash_report::reports_dir() else {
        return;
    };
    for line in crash_loop_warnings(&dir, udid, chrono::Utc::now().naive_utc()) {
        eprintln!("{line}");
    }
}

/// A system process that dies as it starts is restarted and dies again,
/// each time with a crash report dialog on this Mac's desktop: PosterBoard
/// on 2026-10-14, 36 times, on a clock configuration it could not load.
/// Nothing in the runner's answers shows it, so it is said here — only
/// while it is still going, since one that ended days ago is history that
/// `smix sim crashes` lists.
fn crash_loop_warnings(dir: &Path, udid: &str, now: chrono::NaiveDateTime) -> Vec<String> {
    let reports: Vec<CrashReport> = crash_report::for_simulator(dir, udid)
        .into_iter()
        .map(|r| CrashReport {
            summary: r.summary(),
            when: r.captured,
            process: r.process,
            lines: vec![],
        })
        .collect();
    let refs: Vec<&CrashReport> = reports.iter().collect();
    crash_loops_at(&refs, now)
        .into_iter()
        .map(|l| {
            format!(
                "[runner] warning: {} on {udid} keeps crashing — {} times in {} min ({} – {}), \
                 each with a crash report dialog on this Mac; `smix sim crashes {udid}` lists them",
                l.process,
                l.count,
                l.span.num_minutes().max(1),
                l.first,
                l.last
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const UDID: &str = "A6E0D8D5-E9AA-43CD-BBFE-0F7CBC7F4150";
    const OTHER: &str = "0D1E2F30-4152-6374-8596-A7B8C9D0E1F2";

    fn ips(dir: &Path, name: &str, process: &str, udid: &str, when: &str, termination: &str) {
        let head =
            serde_json::json!({"app_name": process, "bundleID": format!("com.apple.{process}")});
        let body = serde_json::json!({
            "procName": process,
            "captureTime": when,
            "coalitionName": crash_report::coalition_of(udid),
            "exception": {"type": "EXC_CRASH", "signal": "SIGKILL"},
            "termination": {"indicator": termination},
            "faultingThread": 0,
            "threads": [{"frames": []}],
        });
        std::fs::write(dir.join(name), format!("{head}\n{body}")).expect("write");
    }

    fn utc(s: &str) -> chrono::NaiveDateTime {
        smix_adb::crash_time(s).expect("a report time")
    }

    #[test]
    fn a_loop_still_going_on_this_simulator_is_said() {
        let dir = tempfile::tempdir().expect("tempdir");
        for s in 0..4 {
            let when = format!("2026-10-14 06:24:{:02}.0000 +0900", 10 + s);
            ips(
                dir.path(),
                &format!("pb-{s}.ips"),
                "PosterBoard",
                UDID,
                &when,
                "Trace/BPT trap: 5",
            );
        }
        let lines = crash_loop_warnings(dir.path(), UDID, utc("2026-10-14 06:26:00.0000 +0900"));
        assert_eq!(lines.len(), 1, "{lines:?}");
        assert!(lines[0].contains("PosterBoard on A6E0D8D5"), "{}", lines[0]);
        assert!(lines[0].contains("4 times"), "{}", lines[0]);
    }

    #[test]
    fn nothing_is_said_for_an_old_loop_another_simulator_or_shutdown_kills() {
        let dir = tempfile::tempdir().expect("tempdir");
        for s in 0..4 {
            let when = format!("2026-10-14 06:24:{:02}.0000 +0900", 10 + s);
            // the same loop, on another simulator
            ips(
                dir.path(),
                &format!("other-{s}.ips"),
                "PosterBoard",
                OTHER,
                &when,
                "Trace/BPT trap: 5",
            );
            // ended at shutdown on this one, which every shutdown leaves
            ips(
                dir.path(),
                &format!("tccd-{s}.ips"),
                "tccd",
                UDID,
                &when,
                "XPC_EXIT_REASON_SIGTERM_TIMEOUT",
            );
        }
        let now = utc("2026-10-14 06:26:00.0000 +0900");
        assert!(crash_loop_warnings(dir.path(), UDID, now).is_empty());
        // the other simulator's loop is found where it belongs, so the
        // empty answer above is not a reader that found nothing at all
        assert_eq!(crash_loop_warnings(dir.path(), OTHER, now).len(), 1);
        // and a day later it is history
        let later = utc("2026-10-15 06:26:00.0000 +0900");
        assert!(crash_loop_warnings(dir.path(), OTHER, later).is_empty());
    }
}

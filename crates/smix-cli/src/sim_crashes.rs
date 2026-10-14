//! `smix sim crashes`: the reports a device recorded, and which process
//! keeps crashing.

use smix_sdk::device_control::CrashReport;

use crate::CliError;

/// A process that crashed this many times inside `WINDOW` is being
/// restarted and dying again: PosterBoard on a simulator, 2026-10-14,
/// crashed 36 times in 18 minutes on data it could not load. The same
/// count spread over days is separate runs that each crashed once.
const A_LOOP: usize = 3;
const WINDOW: chrono::TimeDelta = chrono::TimeDelta::minutes(10);

/// What launchd writes when it ends a process that did not exit in time
/// after being asked to — every simulator shutdown leaves a few. Those
/// are not crashes of the process's own making.
const ENDED_AT_SHUTDOWN: &str = "SIGTERM_TIMEOUT";

pub(crate) fn print(
    all: &[CrashReport],
    app: Option<&str>,
    device: &str,
    json: bool,
) -> Result<(), CliError> {
    let shown: Vec<&CrashReport> = match app {
        Some(name) => all.iter().filter(|r| r.process.contains(name)).collect(),
        None => all.iter().collect(),
    };
    if json {
        let rows: Vec<_> = shown
            .iter()
            .map(|r| {
                serde_json::json!({
                    "when": r.when,
                    "process": r.process,
                    "summary": r.summary,
                    "lines": r.lines,
                })
            })
            .collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).map_err(|e| CliError::Other(e.to_string()))?
        );
        return Ok(());
    }
    for report in &shown {
        println!("{} {} — {}", report.when, report.process, report.summary);
        for line in &report.lines {
            println!("  {line}");
        }
    }
    // How many were filtered out is the evidence that reads apart from an
    // empty buffer: "none of them was yours" and "the device recorded
    // none" are different answers.
    match app {
        Some(name) => println!(
            "crashes: {} of {} report(s) on {device} name {name}",
            shown.len(),
            all.len()
        ),
        None => println!("crashes: {} report(s) on {device}", all.len()),
    }
    let ended = shown
        .iter()
        .filter(|r| r.summary.contains(ENDED_AT_SHUTDOWN))
        .count();
    if ended > 0 {
        println!(
            "crashes: {ended} of them are processes ended at shutdown for not exiting in time ({ENDED_AT_SHUTDOWN}), not crashes"
        );
    }
    for l in loops(&shown) {
        println!(
            "crashes: {} crashed {} times in {} min ({} – {}) on {device} — it keeps being started and crashing again",
            l.process,
            l.count,
            l.span.num_minutes().max(1),
            l.first,
            l.last
        );
    }
    Ok(())
}

#[derive(Debug, PartialEq, Eq)]
struct Loop {
    process: String,
    count: usize,
    span: chrono::TimeDelta,
    first: String,
    last: String,
}

/// A simulator report's `2026-10-14 06:42:45.8602 +0900`, or logcat's
/// `10-14 06:42:45.123` (no year; only the gaps between reports matter).
fn moment(when: &str) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::parse_from_str(when, "%Y-%m-%d %H:%M:%S%.f %z")
        .map(|t| t.naive_utc())
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&format!("2000-{when}"), "%Y-%m-%d %H:%M:%S%.f")
        })
        .ok()
}

/// Per process, the most crashes inside any `WINDOW`, when that is at
/// least `A_LOOP`; most first. Reports whose time does not read are left
/// out of this, not out of the listing.
fn loops(reports: &[&CrashReport]) -> Vec<Loop> {
    let mut by_process: Vec<(String, Vec<(chrono::NaiveDateTime, String)>)> = Vec::new();
    for r in reports {
        if r.summary.contains(ENDED_AT_SHUTDOWN) {
            continue;
        }
        let Some(at) = moment(&r.when) else { continue };
        match by_process.iter_mut().find(|(p, _)| *p == r.process) {
            Some((_, times)) => times.push((at, r.when.clone())),
            None => by_process.push((r.process.clone(), vec![(at, r.when.clone())])),
        }
    }
    let mut found: Vec<Loop> = by_process
        .into_iter()
        .filter_map(|(process, mut times)| {
            times.sort();
            let mut best: Option<(usize, usize)> = None;
            let mut start = 0;
            for end in 0..times.len() {
                while times[end].0 - times[start].0 > WINDOW {
                    start += 1;
                }
                if best.is_none_or(|(s, e)| end - start > e - s) {
                    best = Some((start, end));
                }
            }
            let (s, e) = best?;
            (e - s + 1 >= A_LOOP).then(|| Loop {
                process,
                count: e - s + 1,
                span: times[e].0 - times[s].0,
                first: times[s].1.clone(),
                last: times[e].1.clone(),
            })
        })
        .collect();
    found.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.process.cmp(&b.process))
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn crash(process: &str, when: &str) -> CrashReport {
        CrashReport {
            when: when.into(),
            process: process.into(),
            summary: "EXC_BREAKPOINT (SIGTRAP) — Trace/BPT trap: 5".into(),
            lines: vec![],
        }
    }

    fn at(minute: u32) -> String {
        format!("2026-10-14 06:{minute:02}:31.0000 +0900")
    }

    #[test]
    fn crashes_close_together_are_a_loop_and_its_span_is_said() {
        // PosterBoard, 2026-10-14: 36 in 18 minutes
        let reports: Vec<_> = (0..36)
            .map(|i| crash("PosterBoard", &at(24 + i / 2)))
            .collect();
        let refs: Vec<_> = reports.iter().collect();
        let found = loops(&refs);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].process, "PosterBoard");
        // two a minute, minutes 24 through 34 inclusive
        assert_eq!(found[0].count, 22, "the most inside ten minutes");
    }

    #[test]
    fn the_same_count_spread_over_days_is_not_a_loop() {
        // NoScene: one deliberate crash per test run, 12 over two days
        let reports: Vec<_> = (0..12)
            .map(|d| {
                crash(
                    "NoScene",
                    &format!("2026-10-{:02} 0{}:00:00.0000 +0900", 1 + d, d % 9),
                )
            })
            .collect();
        assert!(loops(&reports.iter().collect::<Vec<_>>()).is_empty());
    }

    #[test]
    fn processes_ended_at_shutdown_are_not_a_loop() {
        let mut ended: Vec<_> = (0..5).map(|i| crash("tccd", &at(30 + i))).collect();
        for r in &mut ended {
            r.summary = "EXC_CRASH (SIGKILL) — XPC_EXIT_REASON_SIGTERM_TIMEOUT".into();
        }
        assert!(loops(&ended.iter().collect::<Vec<_>>()).is_empty());
    }

    #[test]
    fn logcat_times_read_too_and_two_is_not_a_loop() {
        let android = [
            crash("com.example", "10-14 06:42:45.123"),
            crash("com.example", "10-14 06:43:01.000"),
            crash("com.example", "10-14 06:44:10.500"),
        ];
        assert_eq!(loops(&android.iter().collect::<Vec<_>>())[0].count, 3);
        assert!(loops(&android[..2].iter().collect::<Vec<_>>()).is_empty());
    }
}

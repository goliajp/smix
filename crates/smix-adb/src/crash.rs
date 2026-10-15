//! A crash a device recorded, and which process keeps crashing.

/// One crash the device recorded, as its own buffer words it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashReport {
    /// The timestamp logcat put on the first line, verbatim.
    pub when: String,
    /// The process the report names, or empty when it names none. A
    /// report whose process cannot be read is still a crash, so it is
    /// kept rather than dropped.
    pub process: String,
    /// The first line's message — what happened, in the device's words.
    pub summary: String,
    /// Every line of the report in order, verbatim.
    pub lines: Vec<String>,
}

/// A process that crashed this many times inside `WINDOW` is being
/// restarted and dying again: PosterBoard on a simulator, 2026-10-14,
/// crashed 36 times in 18 minutes on data it could not load. The same
/// count spread over days is separate runs that each crashed once.
const A_LOOP: usize = 3;
const WINDOW: chrono::TimeDelta = chrono::TimeDelta::minutes(10);

/// What launchd writes when it ends a process that did not exit in time
/// after being asked to — every simulator shutdown leaves a few. Those
/// are not crashes of the process's own making.
pub const ENDED_AT_SHUTDOWN: &str = "SIGTERM_TIMEOUT";

/// A process crashing again and again, at its densest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrashLoop {
    pub process: String,
    /// The most crashes inside any ten minutes.
    pub count: usize,
    /// From the first to the last of those.
    pub span: chrono::TimeDelta,
    /// The first and last of them, as the reports wrote the time.
    pub first: String,
    pub last: String,
    /// The process's latest crash of all, read as UTC.
    pub latest: chrono::NaiveDateTime,
}

/// A simulator report's `2026-10-14 06:42:45.8602 +0900` as UTC, or
/// logcat's `10-14 06:42:45.123` (no year and no zone; only the gaps
/// between reports read right).
#[must_use]
pub fn crash_time(when: &str) -> Option<chrono::NaiveDateTime> {
    chrono::DateTime::parse_from_str(when, "%Y-%m-%d %H:%M:%S%.f %z")
        .map(|t| t.naive_utc())
        .or_else(|_| {
            chrono::NaiveDateTime::parse_from_str(&format!("2000-{when}"), "%Y-%m-%d %H:%M:%S%.f")
        })
        .ok()
}

/// Per process, the most crashes inside any `WINDOW`, when that is at
/// least `A_LOOP`; most first. Reports whose time does not read are left
/// out of this, not out of a listing.
#[must_use]
pub fn crash_loops(reports: &[&CrashReport]) -> Vec<CrashLoop> {
    let mut by_process: Vec<(String, Vec<(chrono::NaiveDateTime, String)>)> = Vec::new();
    for r in reports {
        if r.summary.contains(ENDED_AT_SHUTDOWN) {
            continue;
        }
        let Some(at) = crash_time(&r.when) else {
            continue;
        };
        match by_process.iter_mut().find(|(p, _)| *p == r.process) {
            Some((_, times)) => times.push((at, r.when.clone())),
            None => by_process.push((r.process.clone(), vec![(at, r.when.clone())])),
        }
    }
    let mut found: Vec<CrashLoop> = by_process
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
            (e - s + 1 >= A_LOOP).then(|| CrashLoop {
                process,
                count: e - s + 1,
                span: times[e].0 - times[s].0,
                first: times[s].1.clone(),
                last: times[e].1.clone(),
                latest: times[times.len() - 1].0,
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

/// The loops still going at `now` (UTC): a loop of `A_LOOP` or more
/// whose process crashed again inside the last `WINDOW`. One that ended
/// days ago is history, which `smix sim crashes` lists and a warning
/// should not repeat.
#[must_use]
pub fn crash_loops_at(reports: &[&CrashReport], now: chrono::NaiveDateTime) -> Vec<CrashLoop> {
    crash_loops(reports)
        .into_iter()
        .filter(|l| now - l.latest <= WINDOW)
        .collect()
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
        let found = crash_loops(&refs);
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
        assert!(crash_loops(&reports.iter().collect::<Vec<_>>()).is_empty());
    }

    #[test]
    fn processes_ended_at_shutdown_are_not_a_loop() {
        let mut ended: Vec<_> = (0..5).map(|i| crash("tccd", &at(30 + i))).collect();
        for r in &mut ended {
            r.summary = "EXC_CRASH (SIGKILL) — XPC_EXIT_REASON_SIGTERM_TIMEOUT".into();
        }
        assert!(crash_loops(&ended.iter().collect::<Vec<_>>()).is_empty());
    }

    #[test]
    fn logcat_times_read_too_and_two_is_not_a_loop() {
        let android = [
            crash("com.example", "10-14 06:42:45.123"),
            crash("com.example", "10-14 06:43:01.000"),
            crash("com.example", "10-14 06:44:10.500"),
        ];
        assert_eq!(crash_loops(&android.iter().collect::<Vec<_>>())[0].count, 3);
        assert!(crash_loops(&android[..2].iter().collect::<Vec<_>>()).is_empty());
    }

    fn utc(s: &str) -> chrono::NaiveDateTime {
        crash_time(s).expect("a report time")
    }

    #[test]
    fn a_loop_still_crashing_is_current_and_one_from_yesterday_is_not() {
        // PosterBoard's loop ran 06:24–06:34 +0900
        let reports: Vec<_> = (0..36)
            .map(|i| crash("PosterBoard", &at(24 + i / 2)))
            .collect();
        let refs: Vec<_> = reports.iter().collect();
        let during = utc("2026-10-14 06:35:00.0000 +0900");
        assert_eq!(crash_loops_at(&refs, during).len(), 1);
        let next_day = utc("2026-10-15 06:35:00.0000 +0900");
        assert!(crash_loops_at(&refs, next_day).is_empty());
    }

    #[test]
    fn current_is_judged_by_the_latest_crash_not_the_densest_stretch() {
        // a dense stretch long ago, then the process kept crashing more
        // slowly — still crashing, so still current
        let mut reports: Vec<_> = (0..3).map(|i| crash("callservicesd", &at(i))).collect();
        reports.push(crash("callservicesd", &at(30)));
        reports.push(crash("callservicesd", &at(55)));
        let refs: Vec<_> = reports.iter().collect();
        let found = crash_loops_at(&refs, utc("2026-10-14 06:58:00.0000 +0900"));
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].count, 3);
    }
}

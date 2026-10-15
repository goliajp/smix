//! The crash reports macOS writes for simulator processes, read by device.
//!
//! Every simulator's reports land in one folder on this machine,
//! `~/Library/Logs/DiagnosticReports`, and age into its `Retired`
//! subfolder. The folder is not divided by device, but each report is:
//! its body names the coalition it ran in,
//! `com.apple.CoreSimulator.SimDevice.<UDID>`. That is what lets one
//! simulator's crashes be told from another's — on 2026-10-14 a
//! simulator's PosterBoard crashed 36 times in a loop and nothing in smix
//! could say so, while another simulator ran beside it with none.

use std::path::{Path, PathBuf};

/// One report, the fields smix reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IpsReport {
    /// The report file.
    pub path: PathBuf,
    /// The header's `bundleID`, empty when it names none.
    pub bundle: String,
    /// The process that crashed, from `procName`, else the header's
    /// `app_name`.
    pub process: String,
    /// `captureTime` as written, e.g. `2026-10-14 06:42:45.8602 +0900`.
    pub captured: String,
    /// `com.apple.CoreSimulator.SimDevice.<UDID>` for a simulator process.
    pub coalition: String,
    /// `EXC_BREAKPOINT (SIGTRAP)`, or what of it the report carries.
    pub exception: String,
    /// The termination indicator, e.g. `Trace/BPT trap: 5`.
    pub termination: String,
    /// The faulting thread's symbols, innermost first.
    pub frames: Vec<String>,
}

impl IpsReport {
    /// What happened, in one line: the exception, then the termination
    /// indicator, whichever of them the report carries.
    #[must_use]
    pub fn summary(&self) -> String {
        match (self.exception.is_empty(), self.termination.is_empty()) {
            (false, false) => format!("{} — {}", self.exception, self.termination),
            (false, true) => self.exception.clone(),
            (true, _) => self.termination.clone(),
        }
    }
}

/// Where macOS writes the reports, simulator processes included.
#[must_use]
pub fn reports_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Logs/DiagnosticReports"))
}

/// A report is one line of JSON (the header), then the body as JSON.
/// `None` for a file that is not one.
#[must_use]
pub fn read(path: &Path) -> Option<IpsReport> {
    let text = std::fs::read_to_string(path).ok()?;
    parse(path, &text)
}

fn parse(path: &Path, text: &str) -> Option<IpsReport> {
    let (head, body) = text.split_once('\n')?;
    let head: serde_json::Value = serde_json::from_str(head).ok()?;
    let body: serde_json::Value = serde_json::from_str(body).ok()?;
    let s = |v: &serde_json::Value| v.as_str().unwrap_or_default().to_string();
    let thread = body["faultingThread"].as_u64().unwrap_or(0) as usize;
    let frames = body["threads"][thread]["frames"]
        .as_array()
        .map(|fs| {
            fs.iter()
                .filter_map(|f| f["symbol"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let exception = match (
        body["exception"]["type"].as_str(),
        body["exception"]["signal"].as_str(),
    ) {
        (Some(t), Some(sig)) => format!("{t} ({sig})"),
        (Some(t), None) => t.to_string(),
        (None, Some(sig)) => sig.to_string(),
        (None, None) => String::new(),
    };
    let process = match body["procName"].as_str() {
        Some(p) => p.to_string(),
        None => s(&head["app_name"]),
    };
    Some(IpsReport {
        path: path.to_path_buf(),
        bundle: s(&head["bundleID"]),
        process,
        captured: s(&body["captureTime"]),
        coalition: s(&body["coalitionName"]),
        exception,
        termination: s(&body["termination"]["indicator"]),
        frames,
    })
}

/// The coalition a simulator's processes run in.
#[must_use]
pub fn coalition_of(udid: &str) -> String {
    format!("com.apple.CoreSimulator.SimDevice.{udid}")
}

/// Every report in `dir` and its `Retired` subfolder whose process ran in
/// the simulator `udid`, oldest first.
#[must_use]
pub fn for_simulator(dir: &Path, udid: &str) -> Vec<IpsReport> {
    let coalition = coalition_of(udid);
    let mut reports: Vec<IpsReport> = [dir.to_path_buf(), dir.join("Retired")]
        .iter()
        .filter_map(|d| std::fs::read_dir(d).ok())
        .flatten()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "ips"))
        .filter_map(|p| read(&p))
        .filter(|r| r.coalition == coalition)
        .collect();
    // captureTime sorts as text within one zone, which is the machine's
    reports.sort_by(|a, b| a.captured.cmp(&b.captured));
    reports
}

#[cfg(test)]
mod tests {
    use super::*;

    const UDID: &str = "A6E0D8D5-E9AA-43CD-BBFE-0F7CBC7F4150";

    // The fields a PosterBoard report carried on 2026-10-14, when it
    // crashed in a loop on an iOS 27 simulator.
    fn report(process: &str, udid: &str, captured: &str) -> String {
        format!(
            "{{\"app_name\":\"{process}\",\"bundleID\":\"com.apple.{process}\",\"bug_type\":\"309\"}}\n{}",
            serde_json::json!({
                "procName": process,
                "captureTime": captured,
                "coalitionName": coalition_of(udid),
                "exception": {"type": "EXC_BREAKPOINT", "signal": "SIGTRAP"},
                "termination": {"indicator": "Trace/BPT trap: 5"},
                "faultingThread": 0,
                "threads": [{"frames": [{"symbol": "-[PRPosterDescriptor _initWithPath:]"}]}],
            })
        )
    }

    fn write(dir: &Path, name: &str, text: &str) {
        std::fs::create_dir_all(dir).expect("dir");
        std::fs::write(dir.join(name), text).expect("write");
    }

    #[test]
    fn a_report_says_which_simulator_and_what_happened() {
        let r = parse(
            Path::new("x.ips"),
            &report("PosterBoard", UDID, "2026-10-14 06:42:45.8602 +0900"),
        )
        .expect("a report");
        assert_eq!(r.process, "PosterBoard");
        assert_eq!(
            r.coalition,
            format!("com.apple.CoreSimulator.SimDevice.{UDID}")
        );
        assert_eq!(r.exception, "EXC_BREAKPOINT (SIGTRAP)");
        assert_eq!(r.termination, "Trace/BPT trap: 5");
        assert_eq!(
            r.frames,
            vec!["-[PRPosterDescriptor _initWithPath:]".to_string()]
        );
    }

    #[test]
    fn only_this_simulators_reports_are_its_own_retired_ones_included() {
        let dir = tempfile::tempdir().expect("tempdir");
        let other = "49034EAE-EBE2-4A30-8D16-41CAC882058F";
        write(
            dir.path(),
            "PosterBoard-2.ips",
            &report("PosterBoard", UDID, "2026-10-14 06:24:31 +0900"),
        );
        write(
            &dir.path().join("Retired"),
            "PosterBoard-1.ips",
            &report("PosterBoard", UDID, "2026-10-14 06:24:29 +0900"),
        );
        write(
            dir.path(),
            "Insight-1.ips",
            &report("Insight", other, "2026-10-14 06:25:00 +0900"),
        );
        write(dir.path(), "notes.txt", "not a report");
        write(dir.path(), "Broken-1.ips", "{not json");
        let mine = for_simulator(dir.path(), UDID);
        let when: Vec<_> = mine.iter().map(|r| r.captured.as_str()).collect();
        assert_eq!(
            when,
            ["2026-10-14 06:24:29 +0900", "2026-10-14 06:24:31 +0900"]
        );
        assert_eq!(for_simulator(dir.path(), other).len(), 1);
    }

    #[test]
    fn a_report_without_a_body_is_not_one() {
        assert!(parse(Path::new("x.ips"), "{\"app_name\":\"X\"}").is_none());
    }
}

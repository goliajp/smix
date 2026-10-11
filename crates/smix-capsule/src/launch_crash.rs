//! Why the app a runner was brought up for stopped at launch, read from
//! the crash report macOS wrote for it.
//!
//! On iOS 27 an app that still makes its window in the app delegate, with
//! no scene in its Info.plist, is stopped by UIKit before it draws:
//! `__UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption`. All
//! `runner up` saw was xcodebuild exiting 65 with "Application … is not
//! running", which says nothing about the app.

use std::path::{Path, PathBuf};
use std::time::SystemTime;

const NO_SCENE: &str = "UIApplicationEvaluateRuntimeIssueForNoSceneLifecycleAdoption";

/// Where macOS writes the reports for simulator apps too.
#[must_use]
pub fn reports_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join("Library/Logs/DiagnosticReports"))
}

/// A sentence about the newest report for `bundle` written at or after
/// `since`, or `None` when there is none.
#[must_use]
pub fn explain(dir: &Path, bundle: &str, since: SystemTime) -> Option<String> {
    let newest = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .filter(|e| e.path().extension().is_some_and(|x| x == "ips"))
        .filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path())))
        .filter(|(at, _)| *at >= since)
        .filter_map(|(at, path)| {
            let report = read(&path)?;
            (report.bundle == bundle).then_some((at, path, report))
        })
        .max_by_key(|(at, _, _)| *at)?;
    let (_, path, report) = newest;
    Some(describe(bundle, &report, &path))
}

struct Report {
    bundle: String,
    exception: String,
    frames: Vec<String>,
}

/// A report is one line of JSON naming the app, then the body as JSON.
fn read(path: &Path) -> Option<Report> {
    let text = std::fs::read_to_string(path).ok()?;
    let (head, body) = text.split_once('\n')?;
    let head: serde_json::Value = serde_json::from_str(head).ok()?;
    let body: serde_json::Value = serde_json::from_str(body).ok()?;
    let thread = body["faultingThread"].as_u64().unwrap_or(0) as usize;
    let frames = body["threads"][thread]["frames"]
        .as_array()
        .map(|fs| {
            fs.iter()
                .filter_map(|f| f["symbol"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    Some(Report {
        bundle: head["bundleID"].as_str()?.to_string(),
        exception: body["exception"]["type"]
            .as_str()
            .unwrap_or("an exception")
            .to_string(),
        frames,
    })
}

fn describe(bundle: &str, report: &Report, path: &Path) -> String {
    if report.frames.iter().any(|f| f.contains(NO_SCENE)) {
        return format!(
            "{bundle} was stopped at launch by iOS: it makes its window in the app \
             delegate and does not adopt the scene lifecycle, which iOS 27 requires. \
             Give it a UIWindowSceneDelegate that makes the window in \
             `scene(_:willConnectTo:options:)`, and a `UIApplicationSceneManifest` in \
             its Info.plist. Crash report: {}",
            path.display()
        );
    }
    let top = report
        .frames
        .first()
        .map_or("an unnamed frame", String::as_str);
    format!(
        "{bundle} crashed at launch ({}, in {top}). Crash report: {}",
        report.exception,
        path.display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    // A report macOS wrote for an app that makes its window in the app
    // delegate, launched on an iOS 27 simulator.
    const NO_SCENE_REPORT: &str = include_str!("../tests/data/noscene-ios27.ips");

    fn dir_with(name: &str, text: &str) -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join(name), text).expect("write report");
        dir
    }

    #[test]
    fn an_app_without_a_scene_is_told_what_ios_27_wants() {
        let dir = dir_with("NoScene-1.ips", NO_SCENE_REPORT);
        let said = explain(dir.path(), "dev.smix.noscene", SystemTime::UNIX_EPOCH)
            .expect("the report names this bundle");
        assert!(
            said.contains("does not adopt the scene lifecycle"),
            "{said}"
        );
        assert!(said.contains("UIApplicationSceneManifest"), "{said}");
        assert!(said.contains("NoScene-1.ips"), "{said}");
    }

    #[test]
    fn another_crash_is_named_by_what_threw() {
        let other = NO_SCENE_REPORT.replace(NO_SCENE, "SomethingElseEntirely");
        let dir = dir_with("NoScene-1.ips", &other);
        let said = explain(dir.path(), "dev.smix.noscene", SystemTime::UNIX_EPOCH).expect("named");
        assert!(said.contains("crashed at launch (EXC_BREAKPOINT"), "{said}");
        assert!(!said.contains("scene lifecycle"), "{said}");
    }

    #[test]
    fn another_apps_report_or_an_older_one_is_not_this_launch() {
        let dir = dir_with("NoScene-1.ips", NO_SCENE_REPORT);
        assert!(explain(dir.path(), "com.example.other", SystemTime::UNIX_EPOCH).is_none());
        let later = SystemTime::now() + std::time::Duration::from_secs(60);
        assert!(explain(dir.path(), "dev.smix.noscene", later).is_none());
    }
}

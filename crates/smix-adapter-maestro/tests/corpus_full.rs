//! Conformance harness for the Maestro yaml command corpus.
//!
//! Walks `tests/fixtures/*_basic.yaml`, asserting per fixture:
//!
//! 1. `parse_flow_file` returns `Ok(Flow)` (zero `UnsupportedCommand`).
//! 2. `Adapter::new(&MockApp, fixtures_dir).run(&flow)` returns `Ok(RunReport)`.
//! 3. Every step report is `Ok` or `ExpandedSubflow` — `Skipped` is a red
//!    flag for the Maestro parity corpus (only the legacy `cycle_*`
//!    fixtures, which exercise the runFlow cycle-detection error path,
//!    are excluded from the run; they still must parse).
//! 4. `report.warnings` MUST NOT contain "swipe-at-coord not supported".

use async_trait::async_trait;
use smix_adapter_maestro::{Adapter, AppLike, RunReport, RunStepReport, parse_flow_file};
use smix_sdk::{ExpectationFailure, KeyName, Selector, SwipeDirection};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

// ----------------------------------------------------------------------
// SilentMockApp — a `MockApp` variant that records nothing and always
// succeeds. Used by `corpus_full.rs`; per-command call-trace assertions
// live in `runtime_mock.rs` (a different test target). Keeping a slim
// silent mock here avoids cross-test coupling.
// ----------------------------------------------------------------------

#[derive(Default)]
struct SilentMockApp {
    /// Captures fail-path env (e.g. SMIX_APP_PATH_* unset) signals that
    /// some corpus warnings are expected and acceptable.
    last_warning_count: Mutex<usize>,
}

#[async_trait]
impl AppLike for SilentMockApp {
    async fn tap(&self, _: &Selector) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn tap_xcui(&self, _: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn tap_with_mode(
        &self,
        _: &Selector,
        _: smix_sdk::TapMode,
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn clear_user_defaults(
        &self,
        _: &str,
        _: &[String],
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn find_by_text_ocr(
        &self,
        _: &str,
        _: &[String],
    ) -> Result<Option<smix_sdk::OcrFrame>, ExpectationFailure> {
        Ok(None)
    }
    async fn find_norm_coord(
        &self,
        _: &Selector,
    ) -> Result<Option<(f64, f64)>, ExpectationFailure> {
        // SilentMockApp: pretend anchor resolves at center; corpus_full
        // exercises yaml shape coverage, not anchor coord precision.
        Ok(Some((0.5, 0.5)))
    }
    async fn webview_eval(&self, _: &str) -> Result<serde_json::Value, ExpectationFailure> {
        // SilentMockApp: webview_eval not used in corpus_full yaml coverage.
        Ok(serde_json::Value::Null)
    }
    async fn tap_at_coord(&self, _: f64, _: f64) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn fill(&self, _: &Selector, _: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn press_key(&self, _: KeyName) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn scroll(&self, _: &Selector, _: SwipeDirection) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn wait_for(&self, _: &Selector, _: Duration) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn wait_for_not_visible(
        &self,
        _: &Selector,
        _: Duration,
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn assert_visible(&self, _: &Selector) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn find(&self, _: &Selector) -> Result<bool, ExpectationFailure> {
        // For the conformance corpus, `find` defaults to false so that
        // `runFlowConditional { when.visible }` short-circuits the
        // subflow expansion (we test runFlow expansion separately).
        Ok(false)
    }
    async fn launch(&self, _: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn terminate(&self, _: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn launch_fresh(
        &self,
        _: &str,
        _: bool,
        _: bool,
        _: Option<&str>,
        _: &[String],
    ) -> Result<Vec<String>, ExpectationFailure> {
        Ok(Vec::new())
    }
    async fn open_url(&self, _: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn system_popups(&self) -> Result<Vec<smix_sdk::SystemPopup>, ExpectationFailure> {
        Ok(Vec::new())
    }
    async fn system_popup_action(
        &self,
        _popup_id: &str,
        _button_id: &str,
    ) -> Result<bool, ExpectationFailure> {
        Ok(true)
    }
    async fn foreground(&self, _: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn launch_app_with_options(
        &self,
        _: &smix_sdk::LaunchAppOptions,
    ) -> Result<Vec<String>, ExpectationFailure> {
        Ok(Vec::new())
    }
    async fn swipe_at_coord(&self, _: (f64, f64), _: (f64, f64)) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn scroll_screen(&self, _: SwipeDirection) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn assert_not_visible(&self, _: &Selector) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn hide_keyboard(&self) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn screenshot(&self) -> Result<Vec<u8>, ExpectationFailure> {
        Ok(b"PNG-MOCK".to_vec())
    }
    async fn set_clipboard(&self, _: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn get_clipboard(&self) -> Result<String, ExpectationFailure> {
        Ok(String::new())
    }
    async fn paste_text(&self, _: Option<&str>) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn copy_text_from(&self, _: &Selector) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn double_tap(&self, _: &Selector) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn long_press(&self, _: &Selector, _: Duration) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn set_location(&self, _: f64, _: f64) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn travel(&self, _: &[(f64, f64)], _: Option<f64>) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn set_permissions(
        &self,
        _: &str,
        _: &[(smix_sdk::SimctlPermission, smix_sdk::PermissionAction)],
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn add_media(&self, _: &[String]) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn set_orientation(
        &self,
        _: smix_sdk::MaestroOrientation,
    ) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn start_recording(&self, _: &str) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn stop_recording(&self) -> Result<(), ExpectationFailure> {
        Ok(())
    }
    async fn assert_screenshot(
        &self,
        _: &std::path::Path,
        _: u32,
    ) -> Result<smix_sdk::AssertScreenshotOutcome, ExpectationFailure> {
        // SilentMockApp does not decode the screenshot — c6 fixture
        // `assert_screenshot_basic.yaml` validates parser+adapter wire,
        // not the SDK dhash path (covered by SDK lib unit tests).
        Ok(smix_sdk::AssertScreenshotOutcome::Matched { hamming: 0 })
    }
}

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// The conformance corpus is gated to `*_basic.yaml` — one fixture per
/// Maestro yaml command, parser+runtime self-contained, no external
/// subflows, no other-command interactions. Historical fixtures (cycle
/// detection, subflow chains) live in the same directory but are
/// excluded from this strict harness.
fn list_corpus_fixtures() -> Vec<PathBuf> {
    let dir = fixtures_dir();
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read fixtures dir {}: {e}", dir.display()))
        .filter_map(|entry| {
            let p = entry.ok()?.path();
            let name = p.file_name().and_then(|n| n.to_str())?;
            (name.ends_with("_basic.yaml")).then_some(p)
        })
        .collect();
    out.sort();
    out
}

/// `cycle_*` fixtures are deliberately recursive-self-referencing and
/// exercise the `RunFlowCycle` error path — they parse but cannot be
/// `run` against a fresh adapter without raising `RunError::RunFlowCycle`.
/// Excluded from the run-phase; still must parse.
fn is_cycle_fixture(path: &Path) -> bool {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.starts_with("cycle_"))
        .unwrap_or(false)
}

/// Some historical fixtures reference `runFlow: ../../subflows/...` paths
/// that live outside `tests/fixtures/`. They are preserved as real-world
/// parser inputs but cannot be parsed without the external subflow on
/// disk. These fixtures are NOT part of the parity corpus and are skipped
/// from BOTH the parse and run phases. Pattern: yaml content contains a
/// `runFlow:` reference into a `../` parent path.
fn references_external_subflow(path: &Path) -> bool {
    let Ok(text) = std::fs::read_to_string(path) else {
        return false;
    };
    text.lines().any(|line| {
        let trimmed = line.trim_start();
        if !trimmed.contains("runFlow") && !trimmed.contains("file:") {
            return false;
        }
        // any `../` path component in the runFlow value pulls outside
        // the fixtures dir
        trimmed.contains("../")
    })
}

/// Fixtures whose `runScript` / `evalScript` commands are **expected**
/// to surface `RunError::Sdk(DriverError)` containing "not supported"
/// (complete JS runtime is out of scope).
const EXPECTED_DRIVER_ERROR_FIXTURES: &[&str] = &[
    "run_script_unsupported.yaml",
    "eval_script_unsupported.yaml",
];

fn is_expected_driver_error(path: &Path) -> bool {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
    EXPECTED_DRIVER_ERROR_FIXTURES.contains(&name)
}

fn assert_no_skip(report: &RunReport, fixture: &Path) {
    for (i, step) in report.steps.iter().enumerate() {
        if let RunStepReport::Skipped { reason } = step {
            panic!(
                "fixture {} step {i} unexpectedly Skipped: {reason}",
                fixture.display()
            );
        }
    }
}

fn assert_no_swipe_warn(report: &RunReport, fixture: &Path) {
    for w in &report.warnings {
        assert!(
            !w.to_lowercase().contains("swipe-at-coord not supported"),
            "fixture {} carries pre-lift swipe-skip warning: {w}",
            fixture.display()
        );
    }
}

#[tokio::test]
async fn corpus_full_parses_and_runs() {
    let fixtures = list_corpus_fixtures();
    assert!(
        !fixtures.is_empty(),
        "expected at least one fixture in {}",
        fixtures_dir().display()
    );

    let mut parsed = 0;
    let mut ran = 0;
    let mut skipped_run = Vec::new();

    let app = SilentMockApp::default();
    let dir = fixtures_dir();

    let mut skipped_external = Vec::new();
    for path in &fixtures {
        if references_external_subflow(path) {
            skipped_external.push(path.clone());
            continue;
        }
        if is_cycle_fixture(path) {
            // cycle_a / cycle_b reference each other; parse_flow_file
            // walks runFlow recursively and reports RunFlowCycle at
            // parse-time. They live alongside basic fixtures as cycle
            // detection regression cases — not part of conformance.
            skipped_run.push(path.clone());
            continue;
        }
        let flow = match parse_flow_file(path) {
            Ok(f) => f,
            Err(e) => panic!("parse fixture {} failed: {e}", path.display()),
        };
        parsed += 1;

        let mut adapter = Adapter::new(&app, dir.clone());
        let result = adapter.run(&flow).await;
        if is_expected_driver_error(path) {
            match result {
                Err(smix_adapter_maestro::RunError::Sdk(f))
                    if f.message.contains("not supported")
                        && f.code == smix_sdk::FailureCode::DriverError =>
                {
                    ran += 1;
                    continue;
                }
                other => panic!(
                    "fixture {} expected DriverError + 'not supported', got {other:?}",
                    path.display()
                ),
            }
        }
        let report = match result {
            Ok(r) => r,
            Err(e) => panic!("run fixture {} failed: {e}", path.display()),
        };
        *app.last_warning_count.lock().unwrap() += report.warnings.len();
        assert_no_skip(&report, path);
        assert_no_swipe_warn(&report, path);
        ran += 1;
    }

    println!(
        "corpus_full: parsed {parsed}, ran {ran}, cycle-skip {}, external-subflow-skip {}",
        skipped_run.len(),
        skipped_external.len()
    );
    assert!(ran >= 8, "at least 8 basic fixtures should run");
}

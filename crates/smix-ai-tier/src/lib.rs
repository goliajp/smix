#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![forbid(unsafe_code)]

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use smix_error::{ExpectationFailure, FailureCode, FailureInit};
use tokio::process::Command;

/// How to reach the judge.
#[derive(Clone, Debug)]
pub struct AiTierConfig {
    /// Path to the `claude` CLI. Defaults to `claude`, i.e. a PATH lookup.
    pub claude_bin: String,
    /// How long the judge may take before the flow gives up on it.
    pub timeout_secs: u64,
}

impl Default for AiTierConfig {
    fn default() -> Self {
        Self {
            claude_bin: "claude".into(),
            timeout_secs: 60,
        }
    }
}

/// What the judge decided.
///
/// This is a judgement, not a measurement: the same screen and the same
/// condition may not produce the same verdict twice. Callers surface it
/// marked as such.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StructuredVerdict {
    /// Whether the condition holds.
    pub pass: bool,
    /// What the judge saw, in its own words. Carried into the failure
    /// message so a reader knows why.
    pub reason: String,
}

/// Ask the judge whether `condition` holds on the screen in `screenshot_png`.
///
/// Every error here means the judge never reached a verdict — a missing CLI, a
/// timeout, unreadable output. None of them collapse into `pass: false`, which
/// would report a broken app instead of a judge that never ran.
pub async fn judge(
    screenshot_png: &[u8],
    condition: &str,
    cfg: &AiTierConfig,
) -> Result<StructuredVerdict, ExpectationFailure> {
    // The CLI reads the image off disk, so it has to land somewhere first.
    let image = scratch_png_path();
    tokio::fs::write(&image, screenshot_png)
        .await
        .map_err(|e| {
            driver_error(
                format!(
                    "ai-tier: could not stage the screenshot at {}: {e}",
                    image.display()
                ),
                None,
            )
        })?;

    let verdict = ask(&image, condition, cfg).await;
    // Best-effort: a leftover temp png must never turn into a flow failure.
    let _ = tokio::fs::remove_file(&image).await;
    verdict
}

async fn ask(
    image: &Path,
    condition: &str,
    cfg: &AiTierConfig,
) -> Result<StructuredVerdict, ExpectationFailure> {
    let mut cmd = Command::new(&cfg.claude_bin);
    // `-p` needs an explicit `--tools`; `Read` is the narrowest set that can
    // still open the screenshot.
    cmd.arg("--tools")
        .arg("Read")
        .arg("-p")
        .arg(build_prompt(image, condition))
        .arg("--output-format")
        .arg("text")
        // Timing out only drops the future — without this the judge keeps
        // running, outliving the step that gave up on it.
        .kill_on_drop(true);

    let output = match tokio::time::timeout(Duration::from_secs(cfg.timeout_secs), cmd.output())
        .await
    {
        Err(_) => {
            return Err(driver_error(
                format!("ai-tier: the judge timed out after {}s", cfg.timeout_secs),
                Some("raise timeout_secs, or narrow the condition".into()),
            ));
        }
        Ok(Err(e)) => {
            return Err(driver_error(
                format!(
                    "ai-tier: could not run the claude CLI at `{}`: {e}",
                    cfg.claude_bin
                ),
                Some(format!(
                    "install the claude CLI, or point claude_bin at it (currently `{}`)",
                    cfg.claude_bin
                )),
            ));
        }
        Ok(Ok(output)) => output,
    };

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(driver_error(
            format!(
                "ai-tier: the claude CLI exited {}: {stderr}",
                output.status.code().unwrap_or(-1)
            ),
            None,
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_verdict(&stdout).ok_or_else(|| {
        driver_error(
            format!(
                "ai-tier: no verdict in the judge's reply — wanted a JSON object, got: {}",
                stdout.trim()
            ),
            Some("the judge answered but not in the shape asked for; this is not an assertion failure".into()),
        )
    })
}

fn driver_error(message: String, hint: Option<String>) -> ExpectationFailure {
    ExpectationFailure::new(FailureInit {
        code: Some(FailureCode::DriverError),
        message,
        hint,
        ..Default::default()
    })
}

fn scratch_png_path() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("smix-ai-tier-{}-{nanos}.png", std::process::id()))
}

fn build_prompt(image: &Path, condition: &str) -> String {
    format!(
        "Read the screenshot at {}.\n\n\
         Decide whether this condition holds on that screen:\n\
         {condition}\n\n\
         Reply with one JSON object and nothing else:\n\
         {{\"pass\": true|false, \"reason\": \"<one sentence naming what you saw>\"}}",
        image.display()
    )
}

/// Take the verdict out of the reply, tolerating prose around it — models
/// like to introduce themselves, and that is not a reason to fail a flow.
fn parse_verdict(stdout: &str) -> Option<StructuredVerdict> {
    let start = stdout.find('{')?;
    let end = stdout.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&stdout[start..=end]).ok()
}

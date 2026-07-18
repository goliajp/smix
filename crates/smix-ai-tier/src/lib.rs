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
    let reply = with_staged_image(screenshot_png, cfg, |image| {
        format!(
            "Read the screenshot at {}.\n\n\
             Decide whether this condition holds on that screen:\n\
             {condition}\n\n\
             Reply with one JSON object and nothing else:\n\
             {{\"pass\": true|false, \"reason\": \"<one sentence naming what you saw>\"}}",
            image.display()
        )
    })
    .await?;

    parse_json_object(&reply).ok_or_else(|| {
        driver_error(
            format!(
                "ai-tier: no verdict in the judge's reply — wanted a JSON object, got: {}",
                reply.trim()
            ),
            Some("the judge answered but not in the shape asked for; this is not an assertion failure".into()),
        )
    })
}

/// Read `fields` off the screen.
///
/// Values come back as strings. The expression engine's output store is flat
/// and stringly-typed, and what a judge reads off a screen is text anyway.
pub async fn extract(
    screenshot_png: &[u8],
    fields: &[String],
    cfg: &AiTierConfig,
) -> Result<std::collections::BTreeMap<String, String>, ExpectationFailure> {
    let wanted = fields.join(", ");
    let reply = with_staged_image(screenshot_png, cfg, |image| {
        format!(
            "Read the screenshot at {}.\n\n\
             Read these fields off that screen: {wanted}\n\n\
             Reply with one JSON object and nothing else, mapping each field \
             name to its value as a string. Use an empty string for a field \
             you cannot find:\n\
             {{\"<field>\": \"<value>\"}}",
            image.display()
        )
    })
    .await?;

    parse_json_object(&reply).ok_or_else(|| {
        driver_error(
            format!(
                "ai-tier: no field object in the judge's reply — wanted a JSON object, got: {}",
                reply.trim()
            ),
            Some("the judge answered but not in the shape asked for".into()),
        )
    })
}

/// Stage the screenshot where the CLI can read it, ask, then clean up.
async fn with_staged_image(
    screenshot_png: &[u8],
    cfg: &AiTierConfig,
    prompt: impl FnOnce(&Path) -> String,
) -> Result<String, ExpectationFailure> {
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

    let reply = ask(prompt(&image), cfg).await;
    // Best-effort: a leftover temp png must never turn into a flow failure.
    let _ = tokio::fs::remove_file(&image).await;
    reply
}

async fn ask(prompt: String, cfg: &AiTierConfig) -> Result<String, ExpectationFailure> {
    let mut cmd = Command::new(&cfg.claude_bin);
    // `-p` needs an explicit `--tools`; `Read` is the narrowest set that can
    // still open the screenshot.
    cmd.arg("--tools")
        .arg("Read")
        .arg("-p")
        .arg(prompt)
        .arg("--output-format")
        .arg("text")
        // Timing out only drops the future — without this the judge keeps
        // running, outliving the step that gave up on it.
        .kill_on_drop(true);

    let output =
        match tokio::time::timeout(Duration::from_secs(cfg.timeout_secs), cmd.output()).await {
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

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
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

/// Take the object out of the reply, tolerating prose around it — models like
/// to introduce themselves, and that is not a reason to fail a flow.
fn parse_json_object<T: serde::de::DeserializeOwned>(reply: &str) -> Option<T> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&reply[start..=end]).ok()
}

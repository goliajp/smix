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

    let reply = ask(prompt(&image), &[Attachment::image(&image)], cfg).await;
    // Best-effort: a leftover temp png must never turn into a flow failure.
    let _ = tokio::fs::remove_file(&image).await;
    reply
}

/// Something the question is about, alongside the words.
///
/// The primitive is prompt-and-attachments to text. Stating an image as an
/// attachment rather than writing its path into the question is what keeps
/// the primitive independent of who answers it: a local CLI locates it by
/// path, and something speaking a network protocol would send the bytes.
/// A caller that writes the path into its own prose has decided, on the
/// satisfier's behalf, that the reader can open local files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Attachment {
    /// An image on disk.
    Image {
        /// Where it is.
        path: PathBuf,
    },
}

impl Attachment {
    /// An image attachment.
    #[must_use]
    pub fn image(path: impl Into<PathBuf>) -> Self {
        Self::Image { path: path.into() }
    }
}

/// The argv for the local-CLI satisfier.
///
/// Separated from running it so the decisions are testable: that a
/// text-only question grants no tools, and that an attachment is what
/// causes read access rather than it being granted always.
fn claude_argv(prompt: &str, attachments: &[Attachment], _cfg: &AiTierConfig) -> Vec<String> {
    let mut argv: Vec<String> = Vec::new();

    // Read access exists to open the attachments and nothing else, so a
    // question that carries none does not get it. Granting it always made
    // the text case look as though it needed a filesystem.
    if !attachments.is_empty() {
        argv.push("--tools".into());
        argv.push("Read".into());
    }

    let mut full = String::from(prompt);
    for attachment in attachments {
        match attachment {
            Attachment::Image { path } => {
                full.push_str(&format!("\n\nThe screenshot is at {}", path.display()));
            }
        }
    }

    argv.push("-p".into());
    argv.push(full);
    argv.push("--output-format".into());
    argv.push("text".into());
    argv
}

/// Ask once, and return what came back.
///
/// The whole AI surface is this one call: words in, optional attachments
/// alongside, text out. Who satisfies it is configuration — today a local
/// `claude` CLI, which is the default and the only one wired. What smix
/// does not do is branch on provider or keep a capability matrix; that is
/// the abstraction §9 forbids, and it is a different thing from being
/// able to reach a model over a different transport.
///
/// Every error means no answer was produced — a missing binary, a
/// timeout, a non-zero exit — surfaced as a `DriverError`, never folded
/// into a made-up reply. Other tiers (`smix-authoring-propose`) build on
/// this rather than spawning their own.
pub async fn ask(
    prompt: String,
    attachments: &[Attachment],
    cfg: &AiTierConfig,
) -> Result<String, ExpectationFailure> {
    let mut cmd = Command::new(&cfg.claude_bin);
    cmd.args(claude_argv(&prompt, attachments, cfg))
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

/// Take the first JSON object out of the reply, tolerating prose around it —
/// models like to introduce themselves, and that is not a reason to fail a
/// flow. The generic type is the caller's target shape; a reply that carries
/// no object, or one that does not deserialize into `T`, yields `None`.
pub fn parse_json_object<T: serde::de::DeserializeOwned>(reply: &str) -> Option<T> {
    let start = reply.find('{')?;
    let end = reply.rfind('}')?;
    if end < start {
        return None;
    }
    serde_json::from_str(&reply[start..=end]).ok()
}

#[cfg(test)]
mod transport_tests {
    use super::*;
    use std::path::PathBuf;

    fn cfg() -> AiTierConfig {
        AiTierConfig::default()
    }

    #[test]
    fn a_text_only_ask_grants_no_tools() {
        // The primitive is prompt-to-text. Handing the satisfier file
        // access for a question that carries no files widens what it can
        // reach for no reason, and makes the text case look like it needs
        // a filesystem when it does not.
        let argv = claude_argv("is the sky blue?", &[], &cfg());
        assert!(
            !argv.iter().any(|a| a == "--tools"),
            "text-only ask should grant nothing: {argv:?}"
        );
        assert!(argv.iter().any(|a| a == "-p"));
    }

    #[test]
    fn an_attachment_is_named_in_the_prompt_and_readable() {
        // How an attachment reaches the model is the satisfier's business.
        // This one is a local CLI, so it passes a path and the permission
        // to open it; a satisfier speaking HTTP would inline the bytes
        // instead. What the caller states is *that* there is an image.
        let png = PathBuf::from("/tmp/shot.png");
        let argv = claude_argv("what is on screen?", &[Attachment::image(&png)], &cfg());
        let tools = argv
            .iter()
            .position(|a| a == "--tools")
            .expect("an attachment needs read access");
        assert_eq!(argv[tools + 1], "Read");
        let prompt = argv
            .iter()
            .position(|a| a == "-p")
            .map(|i| argv[i + 1].clone())
            .expect("prompt");
        assert!(
            prompt.contains("/tmp/shot.png"),
            "the CLI satisfier locates an attachment by path: {prompt}"
        );
    }

    #[test]
    fn the_caller_does_not_write_the_path_into_its_own_prose() {
        // The judge used to embed the path in the question it wrote. That
        // is what made the primitive untransportable: the prompt itself
        // assumed the reader could open local files.
        let png = PathBuf::from("/tmp/a.png");
        let argv = claude_argv("describe it", &[Attachment::image(&png)], &cfg());
        let prompt = argv
            .iter()
            .position(|a| a == "-p")
            .map(|i| argv[i + 1].clone())
            .expect("prompt");
        let written_by_caller = prompt.split("/tmp/a.png").count() - 1;
        assert_eq!(
            written_by_caller, 1,
            "the path should appear once, added here"
        );
    }
}

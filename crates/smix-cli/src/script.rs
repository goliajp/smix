//! `smix run-script <path>` — sequential script driver over
//! smix's act + sense subcommands. Lets a user assemble a tap / fill /
//! wait-for / etc. workflow in a single YAML file instead of chaining
//! shell invocations of `smix tap`, `smix fill`, ...
//!
//! NOT a replacement for maestro yaml — that has loops, conditionals,
//! `runFlow`, expression evaluation. This is the lightweight
//! shell-friendly alternative for users who already drive smix from the
//! command line and want one file instead of `smix tap … && smix fill …
//! && smix press-key …`.
//!
//! Schema:
//!
//! ```yaml
//! steps:
//!   - cmd: launch          # via smix-sdk simctl client
//!     udid: <UDID>          # required for lifecycle steps
//!     bundle-id: com.example.app
//!   - cmd: wait-for
//!     selector: id:btn-login
//!     timeout: 5           # seconds, default 5
//!   - cmd: tap
//!     selector: id:btn-login
//!   - cmd: fill
//!     selector: id:input-email
//!     text: alice@example.com
//!   - cmd: press-key
//!     key: return
//!   - cmd: hide-keyboard
//!   - cmd: scroll
//!     selector: id:v2-list-row-50
//!     direction: down
//!   - cmd: find
//!     selector: id:v2-list-row-50
//!   - cmd: tree
//!     json: true
//!   - cmd: describe
//!   - cmd: system-popups
//! ```
//!
//! Each step dispatches to the corresponding `crate::act::cmd_*` fn —
//! ergonomics + selector/key/direction shorthand are identical to the
//! standalone CLI invocations.

use crate::act::{
    self, cmd_describe, cmd_fill, cmd_find, cmd_hide_keyboard, cmd_press_key, cmd_scroll,
    cmd_system_popups, cmd_tap, cmd_tree, cmd_wait_for,
};
use serde_norway::Value;
use smix_simctl::SimctlClient;
use std::path::{Path, PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum ScriptError {
    #[error("read script {path}: {source}")]
    Read {
        path: String,
        source: std::io::Error,
    },
    #[error("parse script {path}: {reason}")]
    Parse { path: String, reason: String },
    #[error("step {idx} ({cmd}): {source}")]
    Step {
        idx: usize,
        cmd: String,
        source: act::ActError,
    },
    #[error("step {idx} ({cmd}): simctl: {reason}")]
    Simctl {
        idx: usize,
        cmd: String,
        reason: String,
    },
}

#[derive(Debug, PartialEq)]
pub enum Step {
    Tap {
        selector: String,
    },
    Find {
        selector: String,
    },
    WaitFor {
        selector: String,
        timeout: u64,
    },
    Fill {
        selector: String,
        text: String,
    },
    PressKey {
        key: String,
    },
    Scroll {
        selector: String,
        direction: String,
    },
    HideKeyboard,
    Tree {
        json: bool,
    },
    Describe {
        json: bool,
    },
    SystemPopups {
        json: bool,
    },
    /// `cmd: launch` mirrors `smix sim launch <udid> <bundle-id>`.
    /// Runs through simctl (sim-side) — does NOT require an active runner.
    Launch {
        udid: String,
        bundle_id: String,
    },
    /// `cmd: terminate` mirrors `smix sim terminate`.
    Terminate {
        udid: String,
        bundle_id: String,
    },
    /// `cmd: install` mirrors `smix sim install <udid> <app-path>`.
    Install {
        udid: String,
        app_path: PathBuf,
    },
    /// `cmd: uninstall` mirrors `smix sim uninstall`.
    Uninstall {
        udid: String,
        bundle_id: String,
    },
}

impl Step {
    /// Cheap label for diagnostics; mirrors the yaml `cmd:` value.
    pub fn cmd_name(&self) -> &'static str {
        match self {
            Step::Tap { .. } => "tap",
            Step::Find { .. } => "find",
            Step::WaitFor { .. } => "wait-for",
            Step::Fill { .. } => "fill",
            Step::PressKey { .. } => "press-key",
            Step::Scroll { .. } => "scroll",
            Step::HideKeyboard => "hide-keyboard",
            Step::Tree { .. } => "tree",
            Step::Describe { .. } => "describe",
            Step::SystemPopups { .. } => "system-popups",
            Step::Launch { .. } => "launch",
            Step::Terminate { .. } => "terminate",
            Step::Install { .. } => "install",
            Step::Uninstall { .. } => "uninstall",
        }
    }
}

pub fn parse_script(yaml: &str) -> Result<Vec<Step>, String> {
    let root: Value = serde_norway::from_str(yaml).map_err(|e| format!("yaml: {e}"))?;
    let steps_v = root
        .get(Value::String("steps".into()))
        .ok_or_else(|| "missing top-level `steps:` array".to_string())?;
    let steps_seq = steps_v
        .as_sequence()
        .ok_or_else(|| "`steps:` must be a yaml sequence".to_string())?;
    let mut out = Vec::with_capacity(steps_seq.len());
    for (idx, step_v) in steps_seq.iter().enumerate() {
        out.push(parse_step(idx, step_v)?);
    }
    Ok(out)
}

fn parse_step(idx: usize, v: &Value) -> Result<Step, String> {
    let map = v
        .as_mapping()
        .ok_or_else(|| format!("step {idx} must be a mapping"))?;
    let cmd = map
        .get(Value::String("cmd".into()))
        .and_then(Value::as_str)
        .ok_or_else(|| format!("step {idx} missing `cmd:` key"))?;
    let get_str = |key: &str| -> Option<String> {
        map.get(Value::String(key.into()))
            .and_then(Value::as_str)
            .map(|s| s.to_string())
    };
    let get_u64 =
        |key: &str| -> Option<u64> { map.get(Value::String(key.into())).and_then(|v| v.as_u64()) };
    let get_bool =
        |key: &str| -> Option<bool> { map.get(Value::String(key.into())).and_then(Value::as_bool) };
    let required_str = |key: &str| -> Result<String, String> {
        get_str(key).ok_or_else(|| format!("step {idx} ({cmd}) missing `{key}:` key"))
    };
    let step = match cmd {
        "tap" => Step::Tap {
            selector: required_str("selector")?,
        },
        "find" => Step::Find {
            selector: required_str("selector")?,
        },
        "wait-for" => Step::WaitFor {
            selector: required_str("selector")?,
            timeout: get_u64("timeout").unwrap_or(5),
        },
        "fill" => Step::Fill {
            selector: required_str("selector")?,
            text: required_str("text")?,
        },
        "press-key" => Step::PressKey {
            key: required_str("key")?,
        },
        "scroll" => Step::Scroll {
            selector: required_str("selector")?,
            direction: required_str("direction")?,
        },
        "hide-keyboard" => Step::HideKeyboard,
        "tree" => Step::Tree {
            json: get_bool("json").unwrap_or(false),
        },
        "describe" => Step::Describe {
            json: get_bool("json").unwrap_or(false),
        },
        "system-popups" => Step::SystemPopups {
            json: get_bool("json").unwrap_or(false),
        },
        "launch" => Step::Launch {
            udid: required_str("udid")?,
            bundle_id: required_str("bundle-id")?,
        },
        "terminate" => Step::Terminate {
            udid: required_str("udid")?,
            bundle_id: required_str("bundle-id")?,
        },
        "install" => Step::Install {
            udid: required_str("udid")?,
            app_path: PathBuf::from(required_str("app-path")?),
        },
        "uninstall" => Step::Uninstall {
            udid: required_str("udid")?,
            bundle_id: required_str("bundle-id")?,
        },
        other => return Err(format!("step {idx} unknown cmd `{other}`")),
    };
    Ok(step)
}

/// `smix run-script <path> [--port <port>]` — dispatch each step in order.
/// First step error aborts; subsequent steps are not run.
pub async fn cmd_run_script(
    path: &Path,
    port: u16,
    platform: smix_driver::Platform,
) -> Result<(), ScriptError> {
    let yaml = std::fs::read_to_string(path).map_err(|source| ScriptError::Read {
        path: path.display().to_string(),
        source,
    })?;
    let steps = parse_script(&yaml).map_err(|reason| ScriptError::Parse {
        path: path.display().to_string(),
        reason,
    })?;
    for (idx, step) in steps.iter().enumerate() {
        run_step(idx, step, port, platform).await?;
    }
    Ok(())
}

async fn run_step(
    idx: usize,
    step: &Step,
    port: u16,
    platform: smix_driver::Platform,
) -> Result<(), ScriptError> {
    let cmd = step.cmd_name();
    // Lifecycle steps (launch / terminate / install / uninstall) go through
    // simctl, not the runner; they don't take a port. Branch on those first
    // to surface a clear ScriptError::Simctl variant on failure.
    match step {
        Step::Launch { udid, bundle_id } => {
            let client = SimctlClient::new();
            let res = client
                .launch(udid, bundle_id)
                .await
                .map_err(|e| ScriptError::Simctl {
                    idx,
                    cmd: cmd.to_string(),
                    reason: format!("{e}"),
                })?;
            println!("launched: {bundle_id} on {udid} (pid {})", res.pid);
            return Ok(());
        }
        Step::Terminate { udid, bundle_id } => {
            let client = SimctlClient::new();
            client
                .terminate(udid, bundle_id)
                .await
                .map_err(|e| ScriptError::Simctl {
                    idx,
                    cmd: cmd.to_string(),
                    reason: format!("{e}"),
                })?;
            println!("terminated: {bundle_id} on {udid}");
            return Ok(());
        }
        Step::Install { udid, app_path } => {
            let client = SimctlClient::new();
            client
                .install(udid, &app_path.display().to_string())
                .await
                .map_err(|e| ScriptError::Simctl {
                    idx,
                    cmd: cmd.to_string(),
                    reason: format!("{e}"),
                })?;
            println!("installed: {} on {udid}", app_path.display());
            return Ok(());
        }
        Step::Uninstall { udid, bundle_id } => {
            let client = SimctlClient::new();
            client
                .uninstall(udid, bundle_id)
                .await
                .map_err(|e| ScriptError::Simctl {
                    idx,
                    cmd: cmd.to_string(),
                    reason: format!("{e}"),
                })?;
            println!("uninstalled: {bundle_id} on {udid}");
            return Ok(());
        }
        _ => {}
    }
    let res = match step {
        Step::Tap { selector } => cmd_tap(selector.clone(), port, platform, Vec::new()).await,
        Step::Find { selector } => cmd_find(selector.clone(), port, platform, Vec::new()).await,
        // Flow yaml's `wait-for` is the presence form; absence is
        // `extendedWaitUntil: { notVisible: … }`, a different step.
        Step::WaitFor { selector, timeout } => {
            cmd_wait_for(
                selector.clone(),
                *timeout,
                port,
                platform,
                false,
                Vec::new(),
            )
            .await
        }
        Step::Fill { selector, text } => {
            cmd_fill(selector.clone(), text.clone(), port, platform).await
        }
        Step::PressKey { key } => cmd_press_key(key.clone(), port, platform).await,
        Step::Scroll {
            selector,
            direction,
        } => cmd_scroll(selector.clone(), direction.clone(), port, platform).await,
        Step::HideKeyboard => cmd_hide_keyboard(port, platform).await,
        Step::Tree { json } => cmd_tree(*json, port, false).await,
        Step::Describe { json } => cmd_describe(*json, port).await,
        Step::SystemPopups { json } => cmd_system_popups(*json, port).await,
        // Lifecycle variants handled above.
        Step::Launch { .. }
        | Step::Terminate { .. }
        | Step::Install { .. }
        | Step::Uninstall { .. } => unreachable!(),
    };
    res.map_err(|source| ScriptError::Step {
        idx,
        cmd: cmd.to_string(),
        source,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_minimal_script() {
        let yaml = "steps:\n  - cmd: hide-keyboard\n";
        let steps = parse_script(yaml).unwrap();
        assert_eq!(steps, vec![Step::HideKeyboard]);
    }

    #[test]
    fn parse_full_script() {
        let yaml = r#"
steps:
  - cmd: wait-for
    selector: "id:btn-login"
    timeout: 5
  - cmd: tap
    selector: "id:btn-login"
  - cmd: fill
    selector: "id:input-email"
    text: "alice@example.com"
  - cmd: press-key
    key: return
  - cmd: hide-keyboard
  - cmd: scroll
    selector: "id:v2-list-row-50"
    direction: down
  - cmd: find
    selector: "id:v2-list-row-50"
  - cmd: tree
    json: true
  - cmd: describe
  - cmd: system-popups
"#;
        let steps = parse_script(yaml).unwrap();
        assert_eq!(steps.len(), 10);
        assert!(
            matches!(steps[0], Step::WaitFor { ref selector, timeout: 5 } if selector == "id:btn-login")
        );
        assert!(matches!(steps[3], Step::PressKey { ref key } if key == "return"));
        assert!(matches!(steps[5], Step::Scroll { ref direction, .. } if direction == "down"));
        assert!(matches!(steps[7], Step::Tree { json: true }));
    }

    #[test]
    fn parse_script_rejects_missing_steps() {
        let yaml = "foo: bar\n";
        let err = parse_script(yaml).unwrap_err();
        assert!(err.contains("missing top-level `steps:` array"));
    }

    #[test]
    fn parse_script_rejects_unknown_cmd() {
        let yaml = "steps:\n  - cmd: dance\n";
        let err = parse_script(yaml).unwrap_err();
        assert!(err.contains("unknown cmd `dance`"));
    }

    #[test]
    fn parse_script_rejects_missing_required_field() {
        let yaml = "steps:\n  - cmd: fill\n    selector: \"id:x\"\n";
        let err = parse_script(yaml).unwrap_err();
        assert!(err.contains("missing `text:` key"));
    }

    #[test]
    fn wait_for_default_timeout_is_5() {
        let yaml = "steps:\n  - cmd: wait-for\n    selector: \"id:x\"\n";
        let steps = parse_script(yaml).unwrap();
        assert_eq!(
            steps,
            vec![Step::WaitFor {
                selector: "id:x".into(),
                timeout: 5
            }]
        );
    }

    #[test]
    fn parse_lifecycle_steps() {
        let yaml = r#"
steps:
  - cmd: install
    udid: "5D087114-ECB3-443C-8DDB-40EEF9CFB90C"
    app-path: "/tmp/x.app"
  - cmd: launch
    udid: "5D087114-ECB3-443C-8DDB-40EEF9CFB90C"
    bundle-id: "com.example.app"
  - cmd: terminate
    udid: "5D087114-ECB3-443C-8DDB-40EEF9CFB90C"
    bundle-id: "com.example.app"
  - cmd: uninstall
    udid: "5D087114-ECB3-443C-8DDB-40EEF9CFB90C"
    bundle-id: "com.example.app"
"#;
        let steps = parse_script(yaml).unwrap();
        assert_eq!(steps.len(), 4);
        assert!(matches!(
            steps[0],
            Step::Install { ref udid, ref app_path }
                if udid == "5D087114-ECB3-443C-8DDB-40EEF9CFB90C"
                    && app_path.to_string_lossy() == "/tmp/x.app"
        ));
        assert!(matches!(
            steps[1],
            Step::Launch { ref bundle_id, .. } if bundle_id == "com.example.app"
        ));
        assert!(matches!(
            steps[2],
            Step::Terminate { ref bundle_id, .. } if bundle_id == "com.example.app"
        ));
        assert!(matches!(
            steps[3],
            Step::Uninstall { ref bundle_id, .. } if bundle_id == "com.example.app"
        ));
    }

    #[test]
    fn parse_lifecycle_step_missing_field() {
        let yaml = "steps:\n  - cmd: launch\n    udid: \"abc\"\n";
        let err = parse_script(yaml).unwrap_err();
        assert!(err.contains("missing `bundle-id:` key"));
    }
}

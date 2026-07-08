#![doc = include_str!("../README.md")]
#![deny(missing_docs)]
#![deny(rustdoc::broken_intra_doc_links)]

//! smix-simctl — xcrun simctl child_process wrapper (outer crate).
//!
//! All operations shell out to `xcrun simctl <subcommand>`; JSON-formatted
//! outputs (list runtimes, list devices, screenshot binary) are parsed with
//! serde_json / raw bytes. Tokio's `process::Command` is the async spawn
//! primitive.
//!
//! This is an outer crate — allowed to depend on the wider tokio ecosystem.
//! Use it from cement (smix-cli / smix-mcp) or from a higher-level driver
//! wrapper.

#![doc(html_root_url = "https://docs.smix.dev/smix-simctl")]

pub mod registry;

use serde::{Deserialize, Serialize};
use std::io;
use std::time::Duration;
use thiserror::Error;
use tokio::process::Command;
use tokio::time::sleep;

/// Failure variants for any `xcrun simctl` invocation.
#[derive(Debug, Error)]
pub enum SimctlError {
    /// Failed to spawn the `xcrun` process itself (PATH lookup / fork failure).
    #[error("spawn xcrun simctl failed: {0}")]
    Spawn(#[from] io::Error),
    /// `xcrun simctl <sub>` exited non-zero.
    #[error("xcrun simctl {subcommand} exited {code}: {stderr}")]
    NonZeroExit {
        /// Subcommand name (e.g. `"boot"`, `"launch"`).
        subcommand: String,
        /// Exit code from `xcrun simctl`.
        code: i32,
        /// Captured stderr (truncated for log-friendliness).
        stderr: String,
    },
    /// `xcrun simctl <sub>` exited 0 but stdout didn't match the expected shape.
    #[error("xcrun simctl {subcommand} returned malformed output: {detail}")]
    Malformed {
        /// Subcommand name.
        subcommand: String,
        /// Parser-side detail.
        detail: String,
    },
    /// `xcrun simctl <sub>` did not complete within the deadline.
    #[error("xcrun simctl {subcommand} timed out after {ms}ms")]
    Timeout {
        /// Subcommand name.
        subcommand: String,
        /// Deadline that was exceeded (milliseconds).
        ms: u64,
    },
}

/// Handle to an active `xcrun simctl io recordVideo` child process. Pair
/// with [`SimctlClient::record_video_stop`] for SIGINT-and-wait shutdown
/// (so the mp4 trailer is flushed). Dropping the handle without `stop`
/// would tokio-SIGKILL on Drop and truncate the output file.
#[derive(Debug)]
pub struct RecordingHandle {
    pub(crate) child: tokio::process::Child,
    /// Output mp4 path verbatim as passed to `record_video_start`.
    pub path: String,
    /// Wall-clock start time for "recording in progress for Xs" diagnostics.
    pub started_at: std::time::Instant,
}

// -------------------- types ----------------------------------------------

/// One iOS / watchOS / tvOS runtime installed on the host.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimctlRuntime {
    /// Fully-qualified runtime identifier (e.g. `"com.apple.CoreSimulator.SimRuntime.iOS-17-0"`).
    pub identifier: String,
    /// Human-readable name (e.g. `"iOS 17.0"`).
    pub name: String,
    /// Version string (e.g. `"17.0"`).
    pub version: String,
    /// Whether the runtime is available for booting devices.
    pub is_available: bool,
}

/// One simulator device known to `xcrun simctl`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimctlDevice {
    /// Device UDID (stable identifier).
    pub udid: String,
    /// Human-readable name.
    pub name: String,
    /// Current state (`"Booted"` / `"Shutdown"` / `"Creating"` / etc.).
    pub state: String,
    /// Whether the device is available for booting.
    pub is_available: bool,
    /// Device-type identifier (e.g. `"com.apple.CoreSimulator.SimDeviceType.iPhone-15"`).
    #[serde(rename = "deviceTypeIdentifier", default)]
    pub device_type_identifier: String,
    /// Runtime identifier this device was created against.
    #[serde(rename = "runtimeIdentifier", default)]
    pub runtime_identifier: String,
}

/// Permission names accepted by `xcrun simctl privacy <udid> grant <name>`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimctlPermission {
    /// Camera access.
    Camera,
    /// Photos library access.
    Photos,
    /// Location access (while-in-use).
    Location,
    /// Background location access (always).
    LocationAlways,
    /// Notification posting permission.
    Notifications,
    /// Microphone access.
    Microphone,
    /// Contacts access.
    Contacts,
    /// Calendar events access.
    Calendar,
    /// Reminders access.
    Reminders,
    /// Media library (music / video) access.
    Media,
    /// Motion / fitness sensor access.
    Motion,
    /// HomeKit accessory access.
    HomeKit,
    /// HealthKit data access.
    Health,
    /// Bluetooth device discovery / connection.
    Bluetooth,
    /// FaceID / TouchID biometric prompt.
    Faceid,
    /// Address-book (deprecated alias for `Contacts`).
    AddressBook,
}

impl SimctlPermission {
    /// Wire string used by `xcrun simctl privacy <udid> grant <name>`.
    pub fn as_str(self) -> &'static str {
        match self {
            SimctlPermission::Camera => "camera",
            SimctlPermission::Photos => "photos",
            SimctlPermission::Location => "location",
            SimctlPermission::LocationAlways => "location-always",
            SimctlPermission::Notifications => "notifications",
            SimctlPermission::Microphone => "microphone",
            SimctlPermission::Contacts => "contacts",
            SimctlPermission::Calendar => "calendar",
            SimctlPermission::Reminders => "reminders",
            SimctlPermission::Media => "media-library",
            SimctlPermission::Motion => "motion",
            SimctlPermission::HomeKit => "homekit",
            SimctlPermission::Health => "health",
            SimctlPermission::Bluetooth => "bluetooth",
            SimctlPermission::Faceid => "faceid",
            SimctlPermission::AddressBook => "addressbook",
        }
    }
}

/// UI appearance mode for `xcrun simctl ui <udid> appearance`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    /// Light mode.
    Light,
    /// Dark mode.
    Dark,
}

impl Appearance {
    /// Wire string used by `xcrun simctl ui <udid> appearance <mode>`.
    pub fn as_str(self) -> &'static str {
        match self {
            Appearance::Light => "light",
            Appearance::Dark => "dark",
        }
    }
}

/// Launched-app result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LaunchResult {
    /// Process ID of the launched app.
    pub pid: u32,
}

// -------------------- raw spawn primitive --------------------------------

/// Execute `xcrun simctl <args>` and capture stdout/stderr.
async fn simctl_capture(args: &[&str]) -> Result<(Vec<u8>, String), SimctlError> {
    simctl_capture_env(args, &[]).await
}

/// `simctl_capture` with extra envp pairs set on the spawned process.
/// The `xcrun simctl launch` subcommand uses this to inject
/// `SIMCTL_CHILD_<KEY>=<VAL>` vars that the launched app sees as
/// `ProcessInfo().environment["KEY"]`. `env` entries here are passed
/// verbatim — caller composes the `SIMCTL_CHILD_` prefix via
/// [`compose_child_env`].
async fn simctl_capture_env(
    args: &[&str],
    env: &[(String, String)],
) -> Result<(Vec<u8>, String), SimctlError> {
    let mut cmd = Command::new("xcrun");
    cmd.arg("simctl");
    for a in args {
        cmd.arg(a);
    }
    for (k, v) in env {
        cmd.env(k, v);
    }
    let output = cmd.output().await?;
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    if !output.status.success() {
        return Err(SimctlError::NonZeroExit {
            subcommand: args.first().map(|s| s.to_string()).unwrap_or_default(),
            code: output.status.code().unwrap_or(-1),
            stderr,
        });
    }
    Ok((output.stdout, stderr))
}

async fn simctl_run(args: &[&str]) -> Result<String, SimctlError> {
    let (stdout, _) = simctl_capture(args).await?;
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

/// Like [`simctl_run`] but injects `child_env` envp on the spawned
/// process. Used by the env-aware launch path so the launched app can
/// read deploy-time secrets / endpoints via `ProcessInfo`.
async fn simctl_run_env(args: &[&str], env: &[(String, String)]) -> Result<String, SimctlError> {
    let (stdout, _) = simctl_capture_env(args, env).await?;
    Ok(String::from_utf8_lossy(&stdout).into_owned())
}

/// Compose user-provided `(key, value)` pairs into the `SIMCTL_CHILD_*`
/// envp that `xcrun simctl launch` strips and delivers to the launched
/// app. Idempotent: a key that already starts with `SIMCTL_CHILD_` is
/// passed through unchanged.
///
/// # Example
///
/// ```
/// use smix_simctl::compose_child_env;
/// let composed = compose_child_env(&[("SMIX_PERF_RECEIVER_URL", "http://h:9999")]);
/// assert_eq!(
///     composed,
///     vec![(
///         "SIMCTL_CHILD_SMIX_PERF_RECEIVER_URL".to_string(),
///         "http://h:9999".to_string(),
///     )]
/// );
/// ```
pub fn compose_child_env(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| {
            let key = if k.starts_with("SIMCTL_CHILD_") {
                (*k).to_string()
            } else {
                format!("SIMCTL_CHILD_{k}")
            };
            (key, (*v).to_string())
        })
        .collect()
}

// -------------------- client --------------------------------------------

/// Stateless wrapper around xcrun simctl. Methods are free functions
/// in spirit (no instance state beyond optionally-cached `xcrun` path);
/// kept as a struct for API ergonomics + future caching.
#[derive(Debug, Default)]
pub struct SimctlClient {}

impl SimctlClient {
    /// Construct a new client (stateless — equivalent to `default()`).
    pub fn new() -> Self {
        SimctlClient {}
    }

    // ---- inventory ------------------------------------------------------

    /// `xcrun simctl list runtimes -j` → `Vec<SimctlRuntime>`.
    pub async fn list_runtimes(&self) -> Result<Vec<SimctlRuntime>, SimctlError> {
        let raw = simctl_run(&["list", "runtimes", "-j"]).await?;
        #[derive(Deserialize)]
        struct Wrap {
            runtimes: Vec<RawRuntime>,
        }
        #[derive(Deserialize)]
        struct RawRuntime {
            identifier: String,
            name: String,
            version: String,
            #[serde(rename = "isAvailable", default)]
            is_available: bool,
        }
        let w: Wrap = serde_json::from_str(&raw).map_err(|e| SimctlError::Malformed {
            subcommand: "list runtimes".into(),
            detail: e.to_string(),
        })?;
        Ok(w.runtimes
            .into_iter()
            .map(|r| SimctlRuntime {
                identifier: r.identifier,
                name: r.name,
                version: r.version,
                is_available: r.is_available,
            })
            .collect())
    }

    /// `xcrun simctl list devices -j` → flattened `Vec<SimctlDevice>`.
    pub async fn list_devices(&self) -> Result<Vec<SimctlDevice>, SimctlError> {
        let raw = simctl_run(&["list", "devices", "-j"]).await?;
        #[derive(Deserialize)]
        struct Wrap {
            devices: std::collections::BTreeMap<String, Vec<RawDevice>>,
        }
        #[derive(Deserialize)]
        struct RawDevice {
            udid: String,
            name: String,
            state: String,
            #[serde(rename = "isAvailable", default)]
            is_available: bool,
            #[serde(rename = "deviceTypeIdentifier", default)]
            device_type_identifier: String,
        }
        let w: Wrap = serde_json::from_str(&raw).map_err(|e| SimctlError::Malformed {
            subcommand: "list devices".into(),
            detail: e.to_string(),
        })?;
        let mut out = Vec::new();
        for (runtime_id, devices) in w.devices {
            for d in devices {
                out.push(SimctlDevice {
                    udid: d.udid,
                    name: d.name,
                    state: d.state,
                    is_available: d.is_available,
                    device_type_identifier: d.device_type_identifier,
                    runtime_identifier: runtime_id.clone(),
                });
            }
        }
        Ok(out)
    }

    // ---- lifecycle ------------------------------------------------------

    /// `xcrun simctl boot <udid>` — fire-and-forget boot request.
    pub async fn boot(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["boot", udid]).await?;
        Ok(())
    }

    /// `xcrun simctl shutdown <udid>`.
    pub async fn shutdown(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["shutdown", udid]).await?;
        Ok(())
    }

    /// Read the sim's current BCP-47 locale (first entry of
    /// `NSGlobalDomain AppleLanguages`). Returns `Ok(None)` when the
    /// preference is unset (defaults read exits non-zero) or unparseable.
    /// Wire format: `simctl spawn <udid> defaults read -g AppleLanguages`
    /// stdout looks like `"(\n    \"en-US\"\n)\n"`; we extract the first
    /// quoted token.
    pub async fn current_locale(&self, udid: &str) -> Result<Option<String>, SimctlError> {
        let out =
            match simctl_run(&["spawn", udid, "defaults", "read", "-g", "AppleLanguages"]).await {
                Ok(s) => s,
                // `defaults read` returns non-zero when the key is unset; that
                // is a legitimate "no opinion" state, not an error.
                Err(SimctlError::NonZeroExit { .. }) => return Ok(None),
                Err(e) => return Err(e),
            };
        // First quoted substring.
        if let Some(start) = out.find('"') {
            let rest = &out[start + 1..];
            if let Some(end) = rest.find('"') {
                return Ok(Some(rest[..end].to_string()));
            }
        }
        Ok(None)
    }

    /// Write `AppleLanguages` (array) + `AppleLocale` (scalar) to the
    /// sim's NSGlobalDomain so SpringBoard + apps re-localize on next
    /// launch. AppleLocale is BCP-47 with hyphen replaced by underscore
    /// (`en_US`); AppleLanguages is the BCP-47 tag verbatim.
    /// **The caller must shutdown + reboot the sim for the change to
    /// take effect** — running apps cache the locale at process start.
    pub async fn set_locale(&self, udid: &str, locale: &str) -> Result<(), SimctlError> {
        simctl_run(&[
            "spawn",
            udid,
            "defaults",
            "write",
            "-g",
            "AppleLanguages",
            "-array",
            locale,
        ])
        .await?;
        let locale_underscore = locale.replace('-', "_");
        simctl_run(&[
            "spawn",
            udid,
            "defaults",
            "write",
            "-g",
            "AppleLocale",
            &locale_underscore,
        ])
        .await?;
        Ok(())
    }

    /// Boot + poll device state == "Booted" within timeout. Tries every
    /// 500 ms until success or `timeout_ms` elapses. Idempotent on
    /// already-booted devices (`xcrun simctl boot` returns non-zero when
    /// the device is already booted; we swallow that).
    pub async fn boot_and_wait(&self, udid: &str, timeout: Duration) -> Result<(), SimctlError> {
        // Issue boot; ignore already-booted error (the only friendly path).
        let _ = simctl_run(&["boot", udid]).await;
        let start = std::time::Instant::now();
        loop {
            let devices = self.list_devices().await?;
            if devices
                .iter()
                .any(|d| d.udid == udid && d.state == "Booted")
            {
                return Ok(());
            }
            if start.elapsed() > timeout {
                return Err(SimctlError::Timeout {
                    subcommand: format!("boot {}", udid),
                    ms: timeout.as_millis() as u64,
                });
            }
            sleep(Duration::from_millis(500)).await;
        }
    }

    /// `xcrun simctl erase <udid>` — wipe device contents.
    pub async fn erase(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["erase", udid]).await?;
        Ok(())
    }

    /// `xcrun simctl install <udid> <app-path>` — install a `.app` bundle.
    pub async fn install(&self, udid: &str, app_path: &str) -> Result<(), SimctlError> {
        simctl_run(&["install", udid, app_path]).await?;
        Ok(())
    }

    /// `xcrun simctl uninstall <udid> <bundle-id>`.
    pub async fn uninstall(&self, udid: &str, bundle_id: &str) -> Result<(), SimctlError> {
        simctl_run(&["uninstall", udid, bundle_id]).await?;
        Ok(())
    }

    /// `xcrun simctl terminate <udid> <bundle-id>` — kill a running app.
    pub async fn terminate(&self, udid: &str, bundle_id: &str) -> Result<(), SimctlError> {
        simctl_run(&["terminate", udid, bundle_id]).await?;
        Ok(())
    }

    /// `xcrun simctl launch <udid> <bundleId>` → parse `"<bundle>: <pid>"`.
    pub async fn launch(&self, udid: &str, bundle_id: &str) -> Result<LaunchResult, SimctlError> {
        self.launch_with_args(udid, bundle_id, &[]).await
    }

    /// `xcrun simctl launch <udid> <bundleId> -- <arg>...` — launch with a
    /// process-level argument vector. Empty `args` is equivalent to
    /// [`Self::launch`]. Mirrors maestro yaml `launchApp.arguments`.
    pub async fn launch_with_args(
        &self,
        udid: &str,
        bundle_id: &str,
        args: &[String],
    ) -> Result<LaunchResult, SimctlError> {
        self.launch_with_args_and_env(udid, bundle_id, args, &[])
            .await
    }

    /// Like [`Self::launch_with_args`] but also sets `SIMCTL_CHILD_*`
    /// envp on the simctl process so the launched app can read
    /// deploy-time vars via `ProcessInfo().environment["KEY"]`.
    /// `child_env` keys without the `SIMCTL_CHILD_` prefix get it added
    /// automatically (per [`compose_child_env`] semantics). Useful for
    /// prelaunching an app before any `openLink` so iOS treats the
    /// subsequent URL handoff as in-app routing instead of cross-app,
    /// side-stepping the SpringBoard "Open in '`<App>`'?" confirmation
    /// dialog.
    pub async fn launch_with_args_and_env(
        &self,
        udid: &str,
        bundle_id: &str,
        args: &[String],
        child_env: &[(&str, &str)],
    ) -> Result<LaunchResult, SimctlError> {
        let mut argv: Vec<&str> = vec!["launch", udid, bundle_id];
        if !args.is_empty() {
            argv.push("--");
            for a in args {
                argv.push(a.as_str());
            }
        }
        let composed = compose_child_env(child_env);
        let out = simctl_run_env(&argv, &composed).await?;
        // Output format: `com.example.app: 12345\n`
        let pid_str =
            out.rsplit(':')
                .next()
                .map(str::trim)
                .ok_or_else(|| SimctlError::Malformed {
                    subcommand: "launch".into(),
                    detail: format!("unexpected stdout shape: {}", out.trim()),
                })?;
        let pid: u32 = pid_str.parse().map_err(|_| SimctlError::Malformed {
            subcommand: "launch".into(),
            detail: format!("non-numeric pid in stdout: {}", out.trim()),
        })?;
        Ok(LaunchResult { pid })
    }

    /// `xcrun simctl openurl <udid> <url>` — open a URL on the device.
    pub async fn open_url(&self, udid: &str, url: &str) -> Result<(), SimctlError> {
        simctl_run(&["openurl", udid, url]).await?;
        Ok(())
    }

    /// `xcrun simctl push <udid> <bundle-id> <apns-json-path>`.
    /// Deliver an APNS payload to a sim-installed app. The payload file is
    /// a JSON document whose top-level dictionary mirrors what an APNS
    /// provider would send; `aps.alert.body` / `aps.alert.title` surface
    /// as banner content and reach the app's
    /// `UNUserNotificationCenterDelegate`.
    pub async fn send_push(
        &self,
        udid: &str,
        bundle_id: &str,
        apns_json_path: &str,
    ) -> Result<(), SimctlError> {
        simctl_run(&["push", udid, bundle_id, apns_json_path]).await?;
        Ok(())
    }

    /// `xcrun simctl ui <udid> appearance <light|dark>` — set UI appearance.
    pub async fn set_appearance(&self, udid: &str, mode: Appearance) -> Result<(), SimctlError> {
        simctl_run(&["ui", udid, "appearance", mode.as_str()]).await?;
        Ok(())
    }

    /// `xcrun simctl privacy <udid> grant <perm> <bundle-id>`.
    pub async fn grant_permission(
        &self,
        udid: &str,
        permission: SimctlPermission,
        bundle_id: &str,
    ) -> Result<(), SimctlError> {
        simctl_run(&["privacy", udid, "grant", permission.as_str(), bundle_id]).await?;
        Ok(())
    }

    /// `xcrun simctl privacy <udid> revoke <perm> <bundle-id>` — explicitly
    /// deny the permission. Mirrors maestro yaml `permissions: { x: deny }`
    /// (the reverse of `grant`). Distinct from `reset`, which returns the
    /// permission to "not determined".
    pub async fn revoke_permission(
        &self,
        udid: &str,
        permission: SimctlPermission,
        bundle_id: &str,
    ) -> Result<(), SimctlError> {
        simctl_run(&["privacy", udid, "revoke", permission.as_str(), bundle_id]).await?;
        Ok(())
    }

    /// `xcrun simctl location <udid> set <lat>,<lng>` — set sim location
    /// to a fixed point. Mirrors maestro `setLocation`.
    pub async fn location_set(
        &self,
        udid: &str,
        latitude: f64,
        longitude: f64,
    ) -> Result<(), SimctlError> {
        let coord = format!("{latitude},{longitude}");
        simctl_run(&["location", udid, "set", &coord]).await?;
        Ok(())
    }

    /// `xcrun simctl location <udid> start [--speed=<m/s>] <waypoints>`
    /// — interpolate sim location along waypoints. Fire-and-return: simctl
    /// injects scenario and returns; sim continues interpolation in background.
    /// Mirrors maestro `travel`.
    pub async fn location_start(
        &self,
        udid: &str,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), SimctlError> {
        if points.len() < 2 {
            return Err(SimctlError::Malformed {
                subcommand: "location-start".into(),
                detail: format!("requires ≥2 waypoints, got {}", points.len()),
            });
        }
        let mut args: Vec<String> = vec!["location".into(), udid.into(), "start".into()];
        if let Some(s) = speed_mps {
            args.push(format!("--speed={s}"));
        }
        for (lat, lng) in points {
            args.push(format!("{lat},{lng}"));
        }
        let args_ref: Vec<&str> = args.iter().map(String::as_str).collect();
        simctl_run(&args_ref).await?;
        Ok(())
    }

    /// `xcrun simctl location <udid> clear` — reset active location
    /// scenario.
    pub async fn location_clear(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["location", udid, "clear"]).await?;
        Ok(())
    }

    /// `xcrun simctl addmedia <udid> <path>...` — add photos / videos /
    /// contacts to sim library. Mirrors maestro `addMedia` (scalar or
    /// array form already flattened on adapter side).
    pub async fn add_media(&self, udid: &str, paths: &[String]) -> Result<(), SimctlError> {
        if paths.is_empty() {
            return Err(SimctlError::Malformed {
                subcommand: "addmedia".into(),
                detail: "no paths supplied".into(),
            });
        }
        let mut args: Vec<&str> = vec!["addmedia", udid];
        for p in paths {
            args.push(p.as_str());
        }
        simctl_run(&args).await?;
        Ok(())
    }

    /// Start recording sim display to `path`. Spawns
    /// `xcrun simctl io <udid> recordVideo <path>` as a long-running child;
    /// returns handle immediately. Caller must pair with
    /// [`Self::record_video_stop`] for clean SIGINT-and-wait shutdown —
    /// dropping the handle would SIGKILL via tokio + lose mp4 trailer.
    pub async fn record_video_start(
        &self,
        udid: &str,
        path: &str,
    ) -> Result<RecordingHandle, SimctlError> {
        let child = tokio::process::Command::new("xcrun")
            .args(["simctl", "io", udid, "recordVideo", path])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn()?;
        // brief settle for simctl to initialize encoder + open output file.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        Ok(RecordingHandle {
            child,
            path: path.to_string(),
            started_at: std::time::Instant::now(),
        })
    }

    /// Stop a recording via SIGINT + wait (≤10s). SIGINT lets simctl
    /// trap and flush the mp4 trailer; SIGKILL would corrupt output.
    /// Timeout escalates to SIGKILL with explicit error mentioning truncation.
    pub async fn record_video_stop(&self, mut handle: RecordingHandle) -> Result<(), SimctlError> {
        let pid = handle.child.id().ok_or_else(|| SimctlError::Malformed {
            subcommand: "recordVideo-stop".into(),
            detail: "child already reaped".into(),
        })?;
        // SAFETY: libc::kill is a thin POSIX syscall wrapper; pid is owned by
        // this Child instance (no race) and SIGINT is signal-safe.
        let rc = unsafe { libc::kill(pid as i32, libc::SIGINT) };
        if rc != 0 {
            return Err(SimctlError::Malformed {
                subcommand: "recordVideo-stop".into(),
                detail: format!(
                    "kill SIGINT failed: errno={}",
                    std::io::Error::last_os_error()
                ),
            });
        }
        let wait_result =
            tokio::time::timeout(std::time::Duration::from_secs(10), handle.child.wait()).await;
        match wait_result {
            Ok(Ok(_status)) => Ok(()),
            Ok(Err(e)) => Err(SimctlError::Malformed {
                subcommand: "recordVideo-stop".into(),
                detail: format!("wait failed: {e}"),
            }),
            Err(_timeout) => {
                let _ = handle.child.kill().await;
                Err(SimctlError::Malformed {
                    subcommand: "recordVideo-stop".into(),
                    detail: "SIGINT timeout (10s) — escalated SIGKILL; output mp4 likely truncated. Inspect simctl recordVideo stderr.".into(),
                })
            }
        }
    }

    /// `xcrun simctl privacy <udid> reset <perm> <bundle-id>` — return the
    /// permission to "not determined" so the next request re-prompts.
    /// May terminate a running instance of the target app (Apple
    /// behavior) — call before launch, not mid-flow.
    pub async fn reset_permission(
        &self,
        udid: &str,
        permission: SimctlPermission,
        bundle_id: &str,
    ) -> Result<(), SimctlError> {
        simctl_run(&["privacy", udid, "reset", permission.as_str(), bundle_id]).await?;
        Ok(())
    }

    /// `xcrun simctl keychain <udid> reset` — clear all keychain entries.
    pub async fn keychain_reset(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["keychain", udid, "reset"]).await?;
        Ok(())
    }

    /// `xcrun simctl pbpaste <udid>` — read clipboard contents.
    pub async fn pasteboard_get(&self, udid: &str) -> Result<String, SimctlError> {
        simctl_run(&["pbpaste", udid]).await
    }

    /// `xcrun simctl pbcopy <udid>` — write clipboard contents (via piped stdin).
    pub async fn pasteboard_set(&self, udid: &str, text: &str) -> Result<(), SimctlError> {
        // pbcopy reads stdin — we pipe via shell echo for simplicity.
        // Long-term: spawn with stdin pipe.
        use tokio::io::AsyncWriteExt;
        let mut cmd = Command::new("xcrun");
        cmd.arg("simctl").arg("pbcopy").arg(udid);
        cmd.stdin(std::process::Stdio::piped());
        let mut child = cmd.spawn()?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(text.as_bytes()).await?;
            drop(stdin); // close stdin so pbcopy returns
        }
        let status = child.wait().await?;
        if !status.success() {
            return Err(SimctlError::NonZeroExit {
                subcommand: "pbcopy".into(),
                code: status.code().unwrap_or(-1),
                stderr: String::new(),
            });
        }
        Ok(())
    }

    /// Toggle "Reduce Motion" accessibility setting via `defaults write`.
    pub async fn set_reduce_motion(&self, udid: &str, enabled: bool) -> Result<(), SimctlError> {
        let val = if enabled { "1" } else { "0" };
        // `defaults write` lives under spawn; routed via simctl spawn.
        simctl_run(&[
            "spawn",
            udid,
            "defaults",
            "write",
            "com.apple.UIKit",
            "UIAccessibilityReduceMotionEnabled",
            "-bool",
            val,
        ])
        .await?;
        Ok(())
    }

    /// `xcrun simctl io <udid> screenshot <tmpfile>` → raw PNG bytes.
    ///
    /// Goes through a temp file: current Xcode's `screenshot -` does not
    /// treat `-` as stdout — it writes a literal file named `-` in cwd
    /// and emits nothing on stdout (observed on Xcode/iOS 26.5).
    pub async fn screenshot(&self, udid: &str) -> Result<Vec<u8>, SimctlError> {
        let tmp =
            std::env::temp_dir().join(format!("smix-screenshot-{udid}-{}.png", std::process::id()));
        let tmp_str = tmp.display().to_string();
        let result = simctl_capture(&["io", udid, "screenshot", &tmp_str]).await;
        let bytes = result.and_then(|_| {
            std::fs::read(&tmp).map_err(|e| SimctlError::Malformed {
                subcommand: "screenshot".into(),
                detail: format!("read {tmp_str}: {e}"),
            })
        });
        let _ = std::fs::remove_file(&tmp);
        let bytes = bytes?;
        if bytes.len() < 8 {
            return Err(SimctlError::Malformed {
                subcommand: "screenshot".into(),
                detail: format!("screenshot file too short: {} bytes", bytes.len()),
            });
        }
        Ok(bytes)
    }

    /// `xcrun simctl create <name> <device-type-id> <runtime-id>` → udid.
    pub async fn create_device(
        &self,
        name: &str,
        device_type: &str,
        runtime_id: &str,
    ) -> Result<String, SimctlError> {
        let out = simctl_run(&["create", name, device_type, runtime_id]).await?;
        Ok(out.trim().to_string())
    }

    /// `xcrun simctl delete <udid>` — delete a simulator device.
    pub async fn delete_device(&self, udid: &str) -> Result<(), SimctlError> {
        simctl_run(&["delete", udid]).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compose_child_env_adds_prefix() {
        let composed = compose_child_env(&[
            ("SMIX_PERF_RECEIVER_URL", "http://127.0.0.1:9999"),
            ("LAUNCH_FORCE_PUSH", "true"),
        ]);
        assert_eq!(
            composed,
            vec![
                (
                    "SIMCTL_CHILD_SMIX_PERF_RECEIVER_URL".to_string(),
                    "http://127.0.0.1:9999".to_string(),
                ),
                (
                    "SIMCTL_CHILD_LAUNCH_FORCE_PUSH".to_string(),
                    "true".to_string(),
                ),
            ]
        );
    }

    #[test]
    fn compose_child_env_already_prefixed_passes_through() {
        // Defensive: caller may pre-prefix; we must not double-prefix.
        let composed = compose_child_env(&[("SIMCTL_CHILD_FOO", "bar")]);
        assert_eq!(
            composed,
            vec![("SIMCTL_CHILD_FOO".to_string(), "bar".to_string())]
        );
    }

    #[test]
    fn compose_child_env_empty_input_is_empty_output() {
        assert!(compose_child_env(&[]).is_empty());
    }
}

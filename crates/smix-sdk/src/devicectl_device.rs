//! `DeviceControl` for a physical iOS device, over `xcrun devicectl`.
//!
//! This implementation is mostly refusals, and that is the honest shape.
//! A simulator can be erased, have its keychain reset, have media pushed
//! into its library and its screen recorded — because it is a directory
//! on a Mac wearing a phone costume. A phone is a phone. Apple exposes
//! six of those operations to `devicectl` and no more. A per-capability
//! survey on 2026-08-06 measured the rest: of the 25 device operations
//! smix offers on a simulator, six are reachable through `devicectl`, two
//! come from the runner, two do not apply to a phone at all (`boot` and
//! `shutdown` — a phone that is on is on), and **fifteen have no
//! equivalent**. Not a harder path: no path. `erase`, `recordVideo`,
//! `location_set` and the pasteboard are among the fifteen.
//!
//! So the design question is not "how do we cover the gap" but "what does
//! smix say when asked for something a phone cannot do". §9#1's third
//! constraint answers it: **loud error, never a silent no-op**. Quietly
//! doing nothing on a device is worse than refusing — the caller gets a
//! success, believes the state changed, and every assertion after that is
//! measuring a lie.
//!
//! What is *not* missing is the part that matters most: sense and act
//! both go through the XCUITest runner, not through here. Tapping,
//! typing, reading the tree and taking screenshots work on a phone
//! exactly as they do on a simulator — proven end to end on 2026-08-06.

use std::path::Path;

use async_trait::async_trait;
use smix_simctl::{DeviceControlError, SimctlClient};

use crate::device_control::{DeviceControl, Permission, PermissionAction};

/// A physical iOS device, driven through `xcrun devicectl`.
pub struct DevicectlClient {
    udid: String,
    /// `stop_recording` takes no device and no path — the trait assumes
    /// the implementation remembers. What there is to remember is the
    /// `devicectl … screen-record` child, which writes until it is told
    /// to stop. Same shape as the simulator's and Android's.
    recording: tokio::sync::Mutex<Option<DevicectlRecording>>,
}

/// A recording in progress: the child doing it, where it writes, and
/// where what it says goes.
struct DevicectlRecording {
    child: tokio::process::Child,
    /// Held, not read: the child prints a few lines when it stops, and
    /// a closed pipe would hand it SIGPIPE while it is saving the movie.
    _said: tokio::io::BufReader<tokio::process::ChildStdout>,
    path: std::path::PathBuf,
    /// The child's stderr, in a file rather than a pipe: a pipe nobody
    /// reads fills, and a full pipe stops the recording it belongs to.
    complaints: std::path::PathBuf,
}

/// CoreDevice's name for "this device can be screenshotted".
pub const CAPABILITY_SCREENSHOT: &str = "com.apple.coredevice.feature.capturescreenshot";
/// CoreDevice's name for "this device's screen can be recorded". Measured
/// 2026-09-19: a booted simulator lists it and records; a connected phone
/// on iOS 26.6.2 does not list it and devicectl refuses to record it. A
/// device that is shut down or disconnected lists almost nothing, so the
/// question only means something once the device is reachable.
pub const CAPABILITY_SCREEN_RECORDING: &str = "com.apple.coredevice.feature.screenrecording";

/// Does the device with this UDID list `feature` among its capabilities?
///
/// Read from `devicectl list devices --json-output`: `properties.hardware.
/// udid` to find the device — not `hardwareProperties`, which JSON version
/// 5 deprecates — and `capabilities[].featureIdentifier` for the answer.
/// A device that is not in the list is an error, not a "no": the list did
/// not say the device cannot, it said nothing about the device at all.
pub fn device_offers(json: &str, udid: &str, feature: &str) -> Result<bool, DeviceControlError> {
    let malformed = |detail: String| DeviceControlError::Malformed {
        subcommand: "devicectl list devices".into(),
        detail,
    };
    let doc: serde_json::Value =
        serde_json::from_str(json).map_err(|e| malformed(e.to_string()))?;
    let devices = doc["result"]["devices"]
        .as_array()
        .ok_or_else(|| malformed("result.devices is missing or not a list".into()))?;
    let device = devices
        .iter()
        .find(|d| d["properties"]["hardware"]["udid"].as_str() == Some(udid))
        .ok_or_else(|| {
            malformed(format!(
                "{udid} is not among the {} device(s) devicectl lists",
                devices.len()
            ))
        })?;
    let offered = device["capabilities"].as_array().is_some_and(|caps| {
        caps.iter()
            .any(|c| c["featureIdentifier"].as_str() == Some(feature))
    });
    Ok(offered)
}

/// May a recording be started on this device? Asked before the child is
/// spawned, because a spawned child that dies at once still looks like a
/// started recording to whoever spawned it.
fn recording_is_offered(list_devices_json: &str, udid: &str) -> Result<(), DeviceControlError> {
    if device_offers(list_devices_json, udid, CAPABILITY_SCREEN_RECORDING)? {
        return Ok(());
    }
    Err(DeviceControlError::non_zero_exit(
        "start_recording",
        -1,
        format!(
            "{udid} does not offer the Screen Recording capability \
             ({CAPABILITY_SCREEN_RECORDING}), so devicectl would refuse to record it. \
             Which devices offer it is theirs to say — `xcrun devicectl list devices \
             --json-output -` lists each device's capabilities."
        )
        .as_str(),
    ))
}

/// Is this the line `screen-record` prints once it is taking frames?
///
/// `start_recording` returns on this and not on the spawn. Measured
/// 2026-09-19: the line comes 0.6 s after the spawn on a quiet machine and
/// seconds later on a busy one, and a recording interrupted before it
/// exits 3 having written nothing and said nothing — a three-second
/// recording on a loaded machine was simply lost.
fn says_recording_started(line: &str) -> bool {
    line.trim() == "Recording started."
}

/// How long `screen-record` gets to say it has started. It connects to
/// the device first; on a machine at load 50 that took a few seconds.
const RECORDING_START_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Is this a path `screen-record` will take? It accepts `.mp4` and
/// nothing else, and says so with a usage error after the fact — so the
/// question is asked here, before a child exists to fail.
fn recording_destination(path: &Path) -> Result<(), DeviceControlError> {
    let is_mp4 = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("mp4"));
    if is_mp4 {
        return Ok(());
    }
    Err(DeviceControlError::non_zero_exit(
        "start_recording",
        -1,
        format!(
            "`devicectl device capture screen-record` writes .mp4 and nothing else; \
             {} would be refused with a usage error",
            path.display()
        )
        .as_str(),
    ))
}

/// May a recording start? One at a time: a second `screen-record` on the
/// same device would fight the first for the encoder.
fn may_begin_recording(in_progress: bool) -> Result<(), DeviceControlError> {
    if in_progress {
        return Err(DeviceControlError::non_zero_exit(
            "start_recording",
            -1,
            "a recording is already in progress (call stop_recording first)",
        ));
    }
    Ok(())
}

/// The recording `stop_recording` is about to end, or the reason there is
/// none.
fn recording_to_end<T>(slot: Option<T>) -> Result<T, DeviceControlError> {
    slot.ok_or_else(|| {
        DeviceControlError::non_zero_exit(
            "stop_recording",
            -1,
            "no recording in progress (call start_recording first)",
        )
    })
}

/// Say no, with the three things a refusal owes the reader.
///
/// What was refused, why it cannot work here, and what to do instead. A
/// message with only the first is a dead end; smix's own guards learned
/// this twice already (`adb-guard`'s remedy line, the destructive-action
/// gate naming `allow-destructive`).
fn refused(action: &str) -> DeviceControlError {
    use crate::device_control::{Availability, availability};
    use smix_simctl::registry::DeviceKind;

    match availability(action, DeviceKind::PhysicalIos) {
        Some(Availability::RefusedByName { why, instead }) => DeviceControlError::non_zero_exit(
            action,
            -1,
            format!(
                "{action} is not available on a physical device: {why}\n\
                 Instead: {instead}"
            )
            .as_str(),
        ),
        // The table says this works here and this code refuses it. One of
        // them is wrong and neither can be trusted, so the message says
        // so rather than inventing a reason — the seventeen sentences
        // that used to live in these method bodies were exactly the kind
        // of second copy that drifts (`code/derive-dont-copy`).
        other => DeviceControlError::non_zero_exit(
            action,
            -1,
            format!(
                "{action} was refused on a physical device, but the platform table \
                 says {other:?}. The table and this code disagree; fix one."
            )
            .as_str(),
        ),
    }
}

impl DevicectlClient {
    /// Bind to one device by UDID.
    ///
    /// The UDID is the usbmux serial, not the CoreDevice UUID that
    /// `devicectl list devices` prints in its Identifier column — the
    /// same phone answers to both, and only the former matches what a
    /// registry entry holds.
    #[must_use]
    pub fn new(udid: impl Into<String>) -> Self {
        Self {
            udid: udid.into(),
            recording: tokio::sync::Mutex::new(None),
        }
    }

    /// The device this client drives.
    #[must_use]
    pub fn udid(&self) -> &str {
        &self.udid
    }

    /// What `devicectl list devices` says right now, as JSON.
    async fn list_devices_json(&self) -> Result<String, DeviceControlError> {
        let (_, listing) = capture_scratch("unused");
        let listed = async {
            run(&self.argv(DevicectlVerb::ListDevices {
                json_output: &listing.to_string_lossy(),
            }))
            .await?;
            Ok::<_, DeviceControlError>(tokio::fs::read_to_string(&listing).await?)
        }
        .await;
        let _ = tokio::fs::remove_file(&listing).await; // scratch; absent when the listing failed
        listed
    }

    /// Does this device list `feature` among its capabilities right now?
    ///
    /// "Right now" matters: a device that is shut down or disconnected
    /// lists almost nothing, so the answer is about a reachable device.
    ///
    /// # Errors
    /// When devicectl cannot be run, or does not list this device at all.
    pub async fn offers(&self, feature: &str) -> Result<bool, DeviceControlError> {
        device_offers(&self.list_devices_json().await?, &self.udid, feature)
    }

    /// argv for `devicectl`, after the `xcrun devicectl` words.
    ///
    /// Every form names the device. A `devicectl` invocation without
    /// `--device` acts on whichever paired device it feels like, which is
    /// the same failure mode as an `adb` command with no `-s` — and that
    /// one has already wiped a phone in this project's history.
    #[must_use]
    pub fn argv(&self, verb: DevicectlVerb<'_>) -> Vec<String> {
        let d = self.udid.clone();
        match verb {
            DevicectlVerb::Launch { bundle_id, args } => {
                let mut v = vec![
                    "device".into(),
                    "process".into(),
                    "launch".into(),
                    "--device".into(),
                    d,
                    bundle_id.to_string(),
                ];
                v.extend(args.iter().map(ToString::to_string));
                v
            }
            DevicectlVerb::OpenUrl { bundle_id, url } => vec![
                "device".into(),
                "process".into(),
                "launch".into(),
                "--device".into(),
                d,
                "--payload-url".into(),
                url.to_string(),
                bundle_id.to_string(),
            ],
            DevicectlVerb::Terminate { pid } => vec![
                "device".into(),
                "process".into(),
                "terminate".into(),
                "--device".into(),
                d,
                "--pid".into(),
                pid.to_string(),
            ],
            DevicectlVerb::Install { app_path } => vec![
                "device".into(),
                "install".into(),
                "app".into(),
                "--device".into(),
                d,
                app_path.to_string(),
            ],
            DevicectlVerb::Uninstall { bundle_id } => vec![
                "device".into(),
                "uninstall".into(),
                "app".into(),
                "--device".into(),
                d,
                bundle_id.to_string(),
            ],
            DevicectlVerb::ListApps => vec![
                "device".into(),
                "info".into(),
                "apps".into(),
                "--device".into(),
                d,
            ],
            DevicectlVerb::Screenshot {
                destination,
                json_output,
            } => vec![
                "device".into(),
                "capture".into(),
                "screenshot".into(),
                "--device".into(),
                d,
                "--destination".into(),
                destination.to_string(),
                "--json-output".into(),
                json_output.to_string(),
            ],
            // The one form with no `--device`: it is the question of which
            // devices there are.
            DevicectlVerb::ListDevices { json_output } => vec![
                "list".into(),
                "devices".into(),
                "--json-output".into(),
                json_output.to_string(),
            ],
            // No `--duration`: it records until it is interrupted, and
            // `stop_recording` is what interrupts it.
            DevicectlVerb::ScreenRecord { destination } => vec![
                "device".into(),
                "capture".into(),
                "screen-record".into(),
                "--device".into(),
                d,
                "--destination".into(),
                destination.to_string(),
            ],
        }
    }
}

/// The `devicectl` operations smix uses.
///
/// A closed set, and no count written beside it — a count is a list with
/// one entry, and this one went stale the day Xcode 27's devicectl grew
/// `capture`. Naming them as a closed set keeps the argv builder honest:
/// a new one has to be added here, and adding it means finding it in
/// `devicectl --help` first.
#[derive(Debug, Clone, Copy)]
pub enum DevicectlVerb<'a> {
    /// Start an app.
    Launch {
        /// Bundle id.
        bundle_id: &'a str,
        /// Process arguments.
        args: &'a [String],
    },
    /// Start an app on a URL. The only deeplink path a phone has.
    OpenUrl {
        /// Bundle id.
        bundle_id: &'a str,
        /// The URL to hand it.
        url: &'a str,
    },
    /// Stop a process by pid.
    Terminate {
        /// Process id on the device.
        pid: u32,
    },
    /// Install a `.app` bundle.
    Install {
        /// Path on the host.
        app_path: &'a str,
    },
    /// Remove an app.
    Uninstall {
        /// Bundle id.
        bundle_id: &'a str,
    },
    /// List what is installed.
    ListApps,
    /// Write the screen to a file. `devicectl` refuses without a
    /// destination, and says what it captured only in the JSON.
    Screenshot {
        /// Where the PNG goes, on this machine.
        destination: &'a str,
        /// Where devicectl writes what it did.
        json_output: &'a str,
    },
    /// Every device devicectl knows, with what each can do.
    ListDevices {
        /// Where devicectl writes the list.
        json_output: &'a str,
    },
    /// Record the screen to a file until interrupted.
    ScreenRecord {
        /// Where the movie goes, on this machine.
        destination: &'a str,
    },
}

/// What `devicectl device capture screenshot` says it wrote.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenshotResult {
    /// A `file://` URL, not a path.
    pub destination: String,
    /// Pixels.
    pub width: u32,
    /// Pixels.
    pub height: u32,
    /// `png` on everything measured so far.
    pub image_format: String,
}

/// Read a screenshot's `--json-output`.
///
/// Only `info.outcome` and `result` are read. JSON version 5 deprecates
/// `hardwareProperties`, `deviceProperties` and `connectionProperties`
/// and says they will be removed; nothing here may come to depend on a
/// dictionary with a leaving date.
///
/// An outcome other than `success` is an error whatever the exit code
/// was: the exit code says devicectl ran, the outcome says what happened.
pub fn parse_screenshot_result(json: &str) -> Result<ScreenshotResult, DeviceControlError> {
    let malformed = |detail: String| DeviceControlError::Malformed {
        subcommand: "devicectl device capture screenshot".into(),
        detail,
    };
    let doc: serde_json::Value =
        serde_json::from_str(json).map_err(|e| malformed(e.to_string()))?;
    let outcome = doc["info"]["outcome"].as_str().unwrap_or("(absent)");
    if outcome != "success" {
        return Err(malformed(format!(
            "outcome is {outcome:?}, not \"success\""
        )));
    }
    let result = &doc["result"];
    let text = |key: &str| {
        result[key]
            .as_str()
            .map(str::to_string)
            .ok_or_else(|| malformed(format!("result.{key} is missing or not a string")))
    };
    let pixels = |key: &str| {
        result[key]
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| malformed(format!("result.{key} is missing or not a pixel count")))
    };
    Ok(ScreenshotResult {
        destination: text("destination")?,
        width: pixels("width")?,
        height: pixels("height")?,
        image_format: text("imageFormat")?,
    })
}

/// Two paths in the temp dir that nothing else is using: the pid keeps
/// processes apart and the nanoseconds keep calls apart.
fn capture_scratch(extension: &str) -> (std::path::PathBuf, std::path::PathBuf) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let stem = format!("smix-devicectl-{}-{nanos}", std::process::id());
    let dir = std::env::temp_dir();
    (
        dir.join(format!("{stem}.{extension}")),
        dir.join(format!("{stem}.json")),
    )
}

#[async_trait]
impl DeviceControl for DevicectlClient {
    fn platform(&self) -> smix_driver::Platform {
        smix_driver::Platform::Ios
    }

    fn as_ios_simctl(&self) -> Option<&SimctlClient> {
        // Not a simulator. A caller reaching for simctl through this
        // would be reaching for a machine that is not there.
        None
    }

    // === The six that exist ==============================================

    async fn launch(&self, _udid: &str, bundle_id: &str) -> Result<u32, DeviceControlError> {
        let out = run(&self.argv(DevicectlVerb::Launch {
            bundle_id,
            args: &[],
        }))
        .await?;
        Ok(parse_pid(&out).unwrap_or(0))
    }

    async fn launch_with_args(
        &self,
        _udid: &str,
        bundle_id: &str,
        args: &[String],
        _activity: Option<&str>,
    ) -> Result<u32, DeviceControlError> {
        let out = run(&self.argv(DevicectlVerb::Launch { bundle_id, args })).await?;
        Ok(parse_pid(&out).unwrap_or(0))
    }

    async fn install(&self, _udid: &str, app_path: &str) -> Result<(), DeviceControlError> {
        run(&self.argv(DevicectlVerb::Install { app_path })).await?;
        Ok(())
    }

    async fn uninstall(&self, _udid: &str, bundle_id: &str) -> Result<(), DeviceControlError> {
        run(&self.argv(DevicectlVerb::Uninstall { bundle_id })).await?;
        Ok(())
    }

    async fn open_url(&self, _udid: &str, url: &str) -> Result<(), DeviceControlError> {
        // devicectl launches an app *on* a URL; there is no "open this
        // URL with whatever handles it". The bundle is required, and the
        // caller that has one should say so — this default targets Safari
        // because a bare URL on a phone is a web link often enough that
        // refusing outright would be unhelpful.
        run(&self.argv(DevicectlVerb::OpenUrl {
            bundle_id: "com.apple.mobilesafari",
            url,
        }))
        .await?;
        Ok(())
    }

    async fn terminate(&self, _udid: &str, _bundle_id: &str) -> Result<(), DeviceControlError> {
        // `devicectl` terminates by pid, not by bundle id, and finding
        // the pid needs a listing that only reports installed apps —
        // not running ones. The runner ends the app under test through
        // XCUIApplication, which is the path smix actually uses.
        Err(refused("terminate"))
    }

    // === The fifteen that do not ========================================

    async fn set_animations_quiet(
        &self,
        _id: &str,
        _quiet: bool,
    ) -> Result<(), DeviceControlError> {
        // Overridden precisely because the trait's default is `Ok(())`.
        //
        // On a simulator that default is harmless — the caller that does
        // not care gets a no-op. Here it would be the silent success
        // §9#1 forbids: the animations keep running, the flow starts
        // racing them, and the failures land somewhere else entirely.
        // A parity test in this file caught it, which is what that test
        // is for.
        Err(refused("set_animations_quiet"))
    }

    async fn keychain_reset(&self, _udid: &str) -> Result<(), DeviceControlError> {
        Err(refused("keychain_reset"))
    }

    async fn privacy_reset_all(
        &self,
        _udid: &str,
        _bundle_id: &str,
    ) -> Result<(), DeviceControlError> {
        Err(refused("privacy_reset_all"))
    }

    async fn clear_app_sandbox(
        &self,
        _udid: &str,
        _bundle_id: &str,
    ) -> Result<(), DeviceControlError> {
        Err(refused("clear_app_sandbox"))
    }

    async fn user_defaults_delete(
        &self,
        _udid: &str,
        _bundle_id: &str,
        _key: &str,
    ) -> Result<bool, DeviceControlError> {
        Err(refused("user_defaults_delete"))
    }

    async fn send_push(
        &self,
        _udid: &str,
        _bundle_id: &str,
        _payload_path: &str,
    ) -> Result<(), DeviceControlError> {
        Err(refused("send_push"))
    }

    async fn set_permission(
        &self,
        _udid: &str,
        _bundle_id: &str,
        _permission: Permission,
        _action: PermissionAction,
    ) -> Result<(), DeviceControlError> {
        Err(refused("set_permission"))
    }

    async fn pasteboard_set(&self, _udid: &str, _text: &str) -> Result<(), DeviceControlError> {
        Err(refused("pasteboard_set"))
    }

    async fn pasteboard_get(&self, _udid: &str) -> Result<String, DeviceControlError> {
        Err(refused("pasteboard_get"))
    }

    async fn add_media(&self, _udid: &str, _paths: &[String]) -> Result<(), DeviceControlError> {
        Err(refused("add_media"))
    }

    async fn location_set(
        &self,
        _udid: &str,
        _lat: f64,
        _lon: f64,
    ) -> Result<(), DeviceControlError> {
        Err(refused("location_set"))
    }

    async fn location_start(
        &self,
        _udid: &str,
        _points: &[(f64, f64)],
        _speed_mps: Option<f64>,
    ) -> Result<(), DeviceControlError> {
        Err(refused("location_start"))
    }

    async fn start_recording(
        &self,
        _udid: &str,
        output_path: &Path,
    ) -> Result<(), DeviceControlError> {
        recording_destination(output_path)?;
        let mut slot = self.recording.lock().await;
        may_begin_recording(slot.is_some())?;
        recording_is_offered(&self.list_devices_json().await?, &self.udid)?;
        let (complaints, _) = capture_scratch("stderr");
        let stderr = std::fs::File::create(&complaints)?;
        // Spawned, not `run`: `run` waits for the exit, and this one does
        // not exit until `stop_recording` interrupts it.
        let child = tokio::process::Command::new("xcrun")
            .arg("devicectl")
            .args(self.argv(DevicectlVerb::ScreenRecord {
                destination: &output_path.to_string_lossy(),
            }))
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(stderr)
            .spawn()?;
        let mut child = child;
        let mut said = tokio::io::BufReader::new(
            child
                .stdout
                .take()
                .expect("stdout was requested as a pipe two lines up"),
        );
        // Wait for the child to say it is recording — or to end, which
        // is how it says it will not.
        let started = tokio::time::timeout(RECORDING_START_TIMEOUT, async {
            use tokio::io::AsyncBufReadExt;
            let mut line = String::new();
            loop {
                line.clear();
                if said.read_line(&mut line).await? == 0 {
                    return Ok::<bool, std::io::Error>(false);
                }
                if says_recording_started(&line) {
                    return Ok(true);
                }
            }
        })
        .await;
        if !matches!(started, Ok(Ok(true))) {
            let _ = child.kill().await; // already gone in the ended case; ending it is the point in the timed-out one
            let complaint = tokio::fs::read_to_string(&complaints)
                .await
                .unwrap_or_default();
            let _ = tokio::fs::remove_file(&complaints).await; // scratch
            let why = match started {
                Err(_) => format!(
                    "devicectl did not say \"Recording started.\" within {} s",
                    RECORDING_START_TIMEOUT.as_secs()
                ),
                _ => "devicectl ended before it started recording".to_string(),
            };
            return Err(DeviceControlError::non_zero_exit(
                "start_recording",
                -1,
                format!(
                    "{why}. devicectl said: {}",
                    if complaint.trim().is_empty() {
                        "(nothing)"
                    } else {
                        complaint.trim()
                    }
                )
                .as_str(),
            ));
        }
        *slot = Some(DevicectlRecording {
            child,
            _said: said,
            path: output_path.to_path_buf(),
            complaints,
        });
        Ok(())
    }

    async fn stop_recording(&self) -> Result<(), DeviceControlError> {
        let mut rec = recording_to_end(self.recording.lock().await.take())?;
        // SIGINT, not kill: the encoder writes the movie's trailer when it
        // is interrupted, and a killed recording is a file no player
        // opens. Dropping the child would be the same loss.
        if let Some(pid) = rec.child.id() {
            // SAFETY: `libc::kill` is a thin POSIX wrapper. The pid belongs
            // to the `Child` held in `rec`, which has not been waited on,
            // so it cannot have been recycled; SIGINT is signal-safe.
            unsafe { libc::kill(pid as i32, libc::SIGINT) };
        }
        let status = rec.child.wait().await?;
        let said = tokio::fs::read_to_string(&rec.complaints)
            .await
            .unwrap_or_default();
        let _ = tokio::fs::remove_file(&rec.complaints).await; // scratch; nothing to report if it is gone
        let written = tokio::fs::metadata(&rec.path)
            .await
            .map(|m| m.len())
            .unwrap_or(0);
        if written == 0 {
            return Err(DeviceControlError::non_zero_exit(
                "stop_recording",
                status.code().unwrap_or(-1),
                format!(
                    "the recording ended and {} is empty or absent. devicectl said: {}",
                    rec.path.display(),
                    if said.trim().is_empty() {
                        "(nothing)"
                    } else {
                        said.trim()
                    }
                )
                .as_str(),
            ));
        }
        Ok(())
    }

    async fn screenshot(&self, _udid: &str) -> Result<Vec<u8>, DeviceControlError> {
        // devicectl writes a file and reports it in JSON; the trait
        // hands back bytes. So: two scratch paths, one call, read, clean
        // up — on the error paths too, since a failed capture can still
        // have left a file behind.
        let (image, report) = capture_scratch("png");
        let outcome = async {
            run(&self.argv(DevicectlVerb::Screenshot {
                destination: &image.to_string_lossy(),
                json_output: &report.to_string_lossy(),
            }))
            .await?;
            parse_screenshot_result(&tokio::fs::read_to_string(&report).await?)?;
            Ok(tokio::fs::read(&image).await?)
        }
        .await;
        let _ = tokio::fs::remove_file(&image).await; // absent on the error paths; nothing to report
        let _ = tokio::fs::remove_file(&report).await; // same
        outcome
    }

    async fn capture_bgra(
        &self,
        _udid: &str,
    ) -> Result<smix_simctl::surface_capture::CapturedFrame, DeviceControlError> {
        Err(refused("capture_bgra"))
    }
}

/// Run `xcrun devicectl` and return stdout.
///
/// One funnel, like `simctl_capture_env` — so there is one place that
/// knows how the tool is invoked, and one place to record it.
async fn run(args: &[String]) -> Result<String, DeviceControlError> {
    let mut cmd = tokio::process::Command::new("xcrun");
    cmd.arg("devicectl");
    for a in args {
        cmd.arg(a);
    }
    let out = cmd.output().await?;
    if !out.status.success() {
        return Err(DeviceControlError::non_zero_exit(
            args.first().map_or("devicectl", String::as_str),
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).as_ref(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Pull a pid out of `devicectl process launch` output.
fn parse_pid(stdout: &str) -> Option<u32> {
    stdout
        .lines()
        .find_map(|l| l.split("pid").nth(1))
        .and_then(|rest| {
            rest.trim_start_matches([':', ' ', '='])
                .split_whitespace()
                .next()
        })
        .and_then(|t| t.trim_end_matches('.').parse().ok())
}

#[cfg(test)]
mod tests {
    #[test]
    fn every_refusal_this_backend_makes_comes_from_the_table() {
        // Walks the table rather than a list of method names: a list
        // here would be the second copy the table was built to remove,
        // and it would be the copy that went stale.
        use crate::device_control::{ACTION_PLATFORMS, Availability};
        use smix_simctl::registry::DeviceKind;

        let idx = DeviceKind::ALL
            .iter()
            .position(|k| *k == DeviceKind::PhysicalIos)
            .expect("PhysicalIos is a kind");

        let mut checked = 0;
        for (action, row) in ACTION_PLATFORMS {
            if let Availability::RefusedByName { why, instead } = row[idx] {
                let msg = format!("{}", refused(action));
                assert!(
                    !msg.contains("disagree"),
                    "{action}: the table and this backend disagree — {msg}"
                );
                assert!(
                    msg.contains(why),
                    "{action}: the refusal lost its reason — {msg}"
                );
                assert!(
                    msg.contains(instead),
                    "{action}: the refusal lost the way out — {msg}"
                );
                checked += 1;
            }
        }
        // Exact, not a floor. `>=` would have let a row quietly vanish
        // and still passed. The number is a fact about the devices —
        // fifteen actions with no counterpart
        // (research/physical-device-obtainability.md §R4), plus
        // `terminate` and `capture_bgra`, which devicectl refuses for
        // reasons of its own — so if it moves, either a gap closed (say
        // so here, deliberately) or a row went missing.
        //
        // 17 until 10.2. Three gaps closed then, deliberately: Xcode 27's
        // devicectl grew `capture screenshot` and `capture screen-record`,
        // and `screenshot`, `start_recording` and `stop_recording` are
        // driven through them — the first read back on an iPhone, the
        // pair refused by name on a phone that does not offer recording.
        assert_eq!(
            checked, 14,
            "the phone refuses 14 of these; this says {checked}"
        );
    }

    use super::*;

    // The shape of a phone's UDID and the identity of none. These tests
    // used to carry a real one, and the day a method went from refusing
    // to working the suite dialled the phone it named — a screenshot, then
    // a recording. A test that must not reach a device should not be able
    // to.
    const UDID: &str = "00000000-0000000000000000";

    fn client() -> DevicectlClient {
        DevicectlClient::new(UDID)
    }

    #[test]
    fn every_form_names_the_device() {
        // A devicectl call without --device acts on whichever paired
        // device it likes. The adb equivalent of this mistake has already
        // wiped a phone in this project's history.
        let c = client();
        let forms = [
            c.argv(DevicectlVerb::Launch {
                bundle_id: "com.example.app",
                args: &[],
            }),
            c.argv(DevicectlVerb::OpenUrl {
                bundle_id: "com.example.app",
                url: "example://x",
            }),
            c.argv(DevicectlVerb::Terminate { pid: 42 }),
            c.argv(DevicectlVerb::Install {
                app_path: "/tmp/a.app",
            }),
            c.argv(DevicectlVerb::Uninstall {
                bundle_id: "com.example.app",
            }),
            c.argv(DevicectlVerb::ListApps),
        ];
        for f in &forms {
            let i = f.iter().position(|a| a == "--device");
            assert!(i.is_some(), "form without --device: {f:?}");
            assert_eq!(f[i.unwrap() + 1], UDID, "wrong device in {f:?}");
        }
    }

    #[test]
    fn a_deeplink_goes_through_payload_url() {
        // The only path a phone has for deeplinks.
        let a = client().argv(DevicectlVerb::OpenUrl {
            bundle_id: "com.example.app",
            url: "example://open?id=7",
        });
        let i = a
            .iter()
            .position(|x| x == "--payload-url")
            .expect("--payload-url");
        assert_eq!(a[i + 1], "example://open?id=7");
    }

    #[test]
    fn install_and_uninstall_use_the_app_subcommand() {
        let c = client();
        let ins = c.argv(DevicectlVerb::Install {
            app_path: "/tmp/a.app",
        });
        assert_eq!(&ins[..3], &["device", "install", "app"]);
        let un = c.argv(DevicectlVerb::Uninstall {
            bundle_id: "com.example.app",
        });
        assert_eq!(&un[..3], &["device", "uninstall", "app"]);
    }

    #[tokio::test]
    async fn what_a_phone_cannot_do_is_refused_with_a_reason_and_a_way_forward() {
        // Three things every refusal owes: what, why, and what instead.
        // A message with only the first is a dead end.
        let c = client();
        let refusals: Vec<(&str, String)> = vec![
            ("keychain_reset", err(c.keychain_reset(UDID).await)),
            // `start_recording` left this list when it stopped being a
            // refusal. Calling it here started a real recording on a
            // phone that happened to be plugged in — a unit test may
            // only call what refuses before dialling.
            ("add_media", err(c.add_media(UDID, &[]).await)),
            ("location_set", err(c.location_set(UDID, 1.0, 2.0).await)),
            (
                "pasteboard_get",
                err(c.pasteboard_get(UDID).await.map(|_| ())),
            ),
            ("send_push", err(c.send_push(UDID, "b", "p").await)),
        ];
        for (name, msg) in refusals {
            assert!(
                msg.contains("not available on a physical device"),
                "{name} did not say it is a device limit: {msg}"
            );
            assert!(
                msg.contains("Instead:"),
                "{name} gave no way forward: {msg}"
            );
            assert!(
                msg.len() > 80,
                "{name}'s refusal is too terse to act on: {msg}"
            );
        }
    }

    #[tokio::test]
    async fn nothing_returns_a_success_it_did_not_earn() {
        // The rule §9#1 exists for: a silent no-op hands back a success,
        // the caller believes the state changed, and every assertion
        // after that is measuring a lie. No device is touched here —
        // these all refuse before reaching one.
        let c = client();
        assert!(c.keychain_reset(UDID).await.is_err());
        assert!(c.privacy_reset_all(UDID, "b").await.is_err());
        assert!(c.clear_app_sandbox(UDID, "b").await.is_err());
        assert!(c.user_defaults_delete(UDID, "b", "k").await.is_err());
        assert!(c.pasteboard_set(UDID, "x").await.is_err());
        assert!(c.stop_recording().await.is_err());
        // `screenshot` left this list when it stopped being a refusal:
        // an implemented method dials the device, and nothing in a unit
        // test may. Its argv and its parser are tested below instead.
    }

    #[test]
    fn it_does_not_pretend_to_be_a_simulator() {
        assert!(client().as_ios_simctl().is_none());
        assert_eq!(client().platform(), smix_driver::Platform::Ios);
    }

    #[test]
    fn a_pid_is_read_out_of_the_launch_output() {
        assert_eq!(
            parse_pid("Launched application with com.example.app bundle identifier, pid: 1234."),
            Some(1234)
        );
        assert_eq!(parse_pid("nothing useful here"), None);
    }

    fn err<T>(r: Result<T, DeviceControlError>) -> String {
        match r {
            Ok(_) => panic!("expected a refusal, got Ok"),
            Err(e) => e.to_string(),
        }
    }
}

#[cfg(test)]
mod parity_tests {
    use super::*;
    use crate::device_control::{ACTION_LEVELS, ActionLevel};

    /// Which methods this impl carries out for real.
    ///
    /// Listed here rather than derived so that adding one is a deliberate
    /// edit — the kind that makes someone check `devicectl --help` first.
    /// A method on this list is NOT called by the test below: calling an
    /// implemented method dials the device, and the first time this list
    /// was one entry short the unit suite took a screenshot of a phone
    /// that happened to be plugged in.
    const IMPLEMENTED: &[&str] = &[
        "screenshot",
        "start_recording",
        "stop_recording",
        "launch",
        "launch_with_args",
        "install",
        "uninstall",
        "open_url",
        // Metadata, not an action on the device.
        "platform",
        "as_ios_simctl",
        "recording_pid",
    ];

    #[test]
    fn what_this_backend_carries_out_is_what_the_table_says_works_on_a_phone() {
        // Both directions, because each alone has a quiet way to be
        // wrong. A method implemented here while the table still refuses
        // it is a capability nobody is told about — that was the state of
        // `screenshot` for an afternoon in v10.2, with every test green.
        // A cell that says Works with nothing behind it is the older and
        // worse failure.
        use crate::device_control::{ACTION_PLATFORMS, Availability};
        use smix_simctl::registry::DeviceKind;
        let idx = DeviceKind::ALL
            .iter()
            .position(|k| *k == DeviceKind::PhysicalIos)
            .expect("PhysicalIos is a kind");
        let works: Vec<&str> = ACTION_PLATFORMS
            .iter()
            .filter(|(_, row)| matches!(row[idx], Availability::Works))
            .map(|(name, _)| *name)
            .collect();
        let unannounced: Vec<&&str> = IMPLEMENTED.iter().filter(|n| !works.contains(n)).collect();
        let unbacked: Vec<&&str> = works.iter().filter(|n| !IMPLEMENTED.contains(n)).collect();
        assert!(
            unannounced.is_empty() && unbacked.is_empty(),
            "implemented here but refused in the table: {unannounced:?}; \
             Works in the table with nothing here: {unbacked:?}"
        );
    }

    #[tokio::test]
    async fn every_classified_action_is_either_done_or_refused() {
        // The third constraint of §9#1, made checkable: there is no
        // third state. A method that is neither implemented nor refusing
        // is one that returns a success it did not earn, and that is the
        // exact failure this whole implementation exists to avoid.
        //
        // Read from ACTION_LEVELS rather than a second hand-written
        // list, so a new trait method cannot slip past by being absent
        // from a table nobody updated.
        let c = DevicectlClient::new("00000000-0000000000000000");
        let udid = c.udid().to_string();
        let mut unaccounted = Vec::new();

        for (name, level) in ACTION_LEVELS {
            if IMPLEMENTED.contains(name) {
                continue;
            }
            // Observe-level metadata needs no refusal.
            if *level == ActionLevel::Observe && !is_device_read(name) {
                continue;
            }
            let refusal = match *name {
                "terminate" => err_of(c.terminate(&udid, "b").await),
                "keychain_reset" => err_of(c.keychain_reset(&udid).await),
                "privacy_reset_all" => err_of(c.privacy_reset_all(&udid, "b").await),
                "clear_app_sandbox" => err_of(c.clear_app_sandbox(&udid, "b").await),
                "user_defaults_delete" => err_of(c.user_defaults_delete(&udid, "b", "k").await),
                "send_push" => err_of(c.send_push(&udid, "b", "p").await),
                "set_permission" => err_of(
                    c.set_permission(&udid, "b", Permission::Camera, PermissionAction::Grant)
                        .await,
                ),
                "pasteboard_set" => err_of(c.pasteboard_set(&udid, "x").await),
                "pasteboard_get" => err_of(c.pasteboard_get(&udid).await.map(|_| ())),
                "add_media" => err_of(c.add_media(&udid, &[]).await),
                "location_set" => err_of(c.location_set(&udid, 0.0, 0.0).await),
                "location_start" => err_of(c.location_start(&udid, &[], None).await),
                "start_recording" => err_of(
                    c.start_recording(&udid, std::path::Path::new("/tmp/x"))
                        .await,
                ),
                "stop_recording" => err_of(c.stop_recording().await),
                "screenshot" => err_of(c.screenshot(&udid).await.map(|_| ())),
                "capture_bgra" => err_of(c.capture_bgra(&udid).await.map(|_| ())),
                "set_animations_quiet" => err_of(c.set_animations_quiet(&udid, true).await),
                other => Some(format!("UNCHECKED: no case for {other}")),
            };
            match refusal {
                Some(msg) if msg.contains("physical device") => {}
                Some(msg) => unaccounted.push(format!("{name}: {msg}")),
                None => {
                    unaccounted.push(format!("{name}: returned Ok — a success it did not earn"))
                }
            }
        }

        assert!(
            unaccounted.is_empty(),
            "methods neither implemented nor refused:\n  {}",
            unaccounted.join("\n  ")
        );
    }

    /// Does this observe-level method actually read the device?
    ///
    /// `platform` and `as_ios_simctl` describe the binding; the rest go
    /// to hardware and so must answer for themselves.
    fn is_device_read(name: &str) -> bool {
        matches!(name, "screenshot" | "capture_bgra" | "pasteboard_get")
    }

    fn err_of<T>(r: Result<T, DeviceControlError>) -> Option<String> {
        r.err().map(|e| e.to_string())
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    const UDID: &str = "00000000-0000000000000000";

    // Recorded from `xcrun devicectl device capture screenshot` against a
    // booted simulator on 2026-09-19 (Xcode 27, JSON version 5). The
    // phone's answer has the same shape; devicectl does not distinguish.
    const SCREENSHOT_JSON: &str = include_str!("../tests/fixtures/devicectl/screenshot.sim.json");

    #[test]
    fn screenshot_argv_names_the_device_the_file_and_the_json() {
        let c = DevicectlClient::new(UDID);
        let v = c.argv(DevicectlVerb::Screenshot {
            destination: "/tmp/a.png",
            json_output: "/tmp/a.json",
        });
        assert_eq!(
            v,
            [
                "device",
                "capture",
                "screenshot",
                "--device",
                UDID,
                "--destination",
                "/tmp/a.png",
                "--json-output",
                "/tmp/a.json",
            ]
        );
    }

    #[test]
    fn a_recorded_screenshot_result_is_read_from_result_not_the_deprecated_dicts() {
        let r = parse_screenshot_result(SCREENSHOT_JSON).expect("the recorded answer parses");
        assert_eq!((r.width, r.height), (1206, 2622));
        assert_eq!(r.image_format, "png");
        assert!(r.destination.starts_with("file://"), "{}", r.destination);
    }

    #[test]
    fn a_recording_runs_until_it_is_told_to_stop() {
        // No `--duration`: the trait's pair is start…stop, so the caller
        // decides how long. A duration chosen at the start would hand
        // that decision to the moment that knows least.
        let c = DevicectlClient::new(UDID);
        let v = c.argv(DevicectlVerb::ScreenRecord {
            destination: "/tmp/a.mov",
        });
        assert_eq!(
            v,
            [
                "device",
                "capture",
                "screen-record",
                "--device",
                UDID,
                "--destination",
                "/tmp/a.mov",
            ]
        );
    }

    #[test]
    fn a_second_start_is_refused_and_a_stop_with_nothing_running_is_too() {
        assert!(may_begin_recording(false).is_ok());
        let again = may_begin_recording(true).expect_err("one recording at a time");
        assert!(again.to_string().contains("already in progress"), "{again}");

        let none = recording_to_end(None::<()>).expect_err("nothing to stop");
        assert!(
            none.to_string().contains("no recording in progress"),
            "{none}"
        );
        assert_eq!(
            recording_to_end(Some(7)).expect("the one that was running"),
            7
        );
    }

    #[test]
    fn a_destination_devicectl_would_reject_is_refused_before_anything_starts() {
        // `screen-record` accepts only `.mp4` and answers anything else
        // with a usage error (exit 64) — measured 2026-09-19 with a `.mov`
        // path, which left an empty file and no reason.
        assert!(recording_destination(std::path::Path::new("/tmp/a.mp4")).is_ok());
        assert!(recording_destination(std::path::Path::new("/tmp/A.MP4")).is_ok());
        for bad in ["/tmp/a.mov", "/tmp/a", "/tmp/a.mp4.part"] {
            let err = recording_destination(std::path::Path::new(bad))
                .expect_err("devicectl would refuse this");
            let msg = err.to_string();
            assert!(msg.contains(".mp4") && msg.contains(bad), "{msg}");
        }
    }

    // What a connected iPhone on iOS 26.6.2 told `devicectl list devices`
    // on 2026-09-19 — its 65 capabilities verbatim, its name, UDID, ECID
    // and serial number replaced or dropped. It offers screenshots, the
    // pasteboard and simulated location, and does not offer screen
    // recording: asked to record anyway, devicectl answered "The
    // capability "Screen Recording" is not supported by this device".
    const PHONE_IOS26: &str =
        include_str!("../tests/fixtures/devicectl/list-devices.phone-ios26.json");

    #[test]
    fn what_a_device_offers_is_read_from_its_capabilities() {
        assert!(device_offers(PHONE_IOS26, UDID, CAPABILITY_SCREENSHOT).expect("listed"));
        assert!(!device_offers(PHONE_IOS26, UDID, CAPABILITY_SCREEN_RECORDING).expect("listed"));
    }

    #[test]
    fn a_device_devicectl_does_not_list_is_an_error_not_a_no() {
        let err = device_offers(
            PHONE_IOS26,
            "11111111-1111111111111111",
            CAPABILITY_SCREENSHOT,
        )
        .expect_err("not in the list");
        assert!(
            err.to_string().contains("11111111-1111111111111111"),
            "{err}"
        );
    }

    #[test]
    fn a_phone_that_cannot_record_is_told_so_before_a_recording_is_started() {
        // `start_recording` spawns a child and returns; without this the
        // child died at once and the caller heard about it three seconds
        // later, from `stop_recording`. A success it had not earned.
        let err =
            recording_is_offered(PHONE_IOS26, UDID).expect_err("iOS 26.6.2 does not offer it");
        let msg = err.to_string();
        assert!(
            msg.contains("Screen Recording") && msg.contains(CAPABILITY_SCREEN_RECORDING),
            "{msg}"
        );
    }

    #[test]
    fn a_recording_has_started_when_devicectl_says_so_and_not_before() {
        // What `screen-record` prints, in order, measured 2026-09-19. The
        // first line arrives when frames are being taken — 0.6 s after the
        // spawn on a quiet machine, several seconds on a busy one — and an
        // interrupt before it leaves an empty file and no complaint.
        assert!(says_recording_started("Recording started."));
        assert!(!says_recording_started("Display: PurpleMain"));
        assert!(!says_recording_started(
            "Press Ctrl+C to stop recording and save the video."
        ));
        assert!(!says_recording_started(""));
    }

    #[tokio::test]
    async fn stopping_before_starting_never_reaches_the_device() {
        let err = DevicectlClient::new(UDID)
            .stop_recording()
            .await
            .expect_err("nothing was started");
        assert!(
            err.to_string().contains("no recording in progress"),
            "{err}"
        );
    }

    #[test]
    fn an_unsuccessful_outcome_is_an_error_even_with_exit_zero() {
        let err = parse_screenshot_result(r#"{"info":{"outcome":"failure"},"result":{}}"#)
            .expect_err("failure is not a screenshot");
        assert!(err.to_string().contains("outcome"), "{err}");
    }
}

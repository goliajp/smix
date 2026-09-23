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
//! `location_set` and the pasteboard were among the fifteen.
//!
//! That survey was of Xcode 26. Xcode 27's `devicectl` grew `capture`,
//! `pasteboard` and `simulate location`, and the screenshot, the
//! recording pair, the two pasteboard actions and the two location
//! actions are carried out here now; the table in
//! `device_control` holds the current count, and a test holds this file
//! against it.
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

use crate::device_control::{CrashReport, DeviceControl, Frontmost, Permission, PermissionAction};

/// A physical iOS device, driven through `xcrun devicectl`.
pub struct DevicectlClient {
    udid: String,
    /// `stop_recording` takes no device and no path — the trait assumes
    /// the implementation remembers. What there is to remember is the
    /// `devicectl … screen-record` child, which writes until it is told
    /// to stop. Same shape as the simulator's and Android's.
    recording: tokio::sync::Mutex<Option<DevicectlRecording>>,
    /// `None` is the general pasteboard — the one the user copies to.
    pasteboard: Option<String>,
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
            pasteboard: None,
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
            // `--latitude -33.8` is refused — devicectl takes the value for
            // another option — so every coordinate is attached with `=`.
            DevicectlVerb::LocationCoordinate {
                latitude,
                longitude,
                json_output,
            } => vec![
                "device".into(),
                "simulate".into(),
                "location".into(),
                "coordinate".into(),
                "--device".into(),
                d,
                format!("--latitude={latitude}"),
                format!("--longitude={longitude}"),
                "--json-output".into(),
                json_output.to_string(),
            ],
            // The file form, not `--waypoints`: that option is a variadic
            // array that swallows every argument after it, `--device`
            // included, and no spelling of a negative latitude got through
            // it. The file takes both.
            DevicectlVerb::LocationRoute {
                route_file,
                json_output,
            } => vec![
                "device".into(),
                "simulate".into(),
                "location".into(),
                "route".into(),
                "--device".into(),
                d,
                "--route-file".into(),
                route_file.to_string(),
                "--json-output".into(),
                json_output.to_string(),
            ],
            DevicectlVerb::LocationClear { json_output } => vec![
                "device".into(),
                "simulate".into(),
                "location".into(),
                "clear".into(),
                "--device".into(),
                d,
                "--json-output".into(),
                json_output.to_string(),
            ],
            DevicectlVerb::PasteboardCopy { json_output } => {
                self.pasteboard_argv("copy", d, json_output)
            }
            DevicectlVerb::PasteboardPaste { json_output } => {
                self.pasteboard_argv("paste", d, json_output)
            }
        }
    }

    /// `copy` and `paste` differ in one word. With no `--device-pasteboard`
    /// devicectl means the general one.
    fn pasteboard_argv(&self, verb: &str, device: String, json_output: &str) -> Vec<String> {
        let mut v = vec![
            "device".into(),
            "pasteboard".into(),
            verb.into(),
            "--device".into(),
            device,
        ];
        if let Some(name) = &self.pasteboard {
            v.push("--device-pasteboard".into());
            v.push(name.clone());
        }
        v.push("--json-output".into());
        v.push(json_output.to_string());
        v
    }

    /// Read and write the pasteboard of this name instead of the general
    /// one. A named pasteboard is the device's own notion: apps make them
    /// to pass data without touching what the user copied.
    #[must_use]
    pub fn on_pasteboard(mut self, name: &str) -> Self {
        self.pasteboard = Some(name.to_string());
        self
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
    /// Hold the device at one coordinate until cleared.
    LocationCoordinate {
        /// Degrees north.
        latitude: f64,
        /// Degrees east.
        longitude: f64,
        /// Where devicectl writes what it set.
        json_output: &'a str,
    },
    /// Move the device along the route a file describes. Returns at once;
    /// the device keeps travelling.
    LocationRoute {
        /// The route, in devicectl's own JSON.
        route_file: &'a str,
        /// Where devicectl writes what it started.
        json_output: &'a str,
    },
    /// Stop simulating, and give the device its own location back.
    LocationClear {
        /// Where devicectl writes what it did.
        json_output: &'a str,
    },
    /// Put text on the device's pasteboard. The text arrives on stdin.
    PasteboardCopy {
        /// Where devicectl writes what it did.
        json_output: &'a str,
    },
    /// Read text off the device's pasteboard. The text is stdout, which
    /// is why the JSON can only go to a file.
    PasteboardPaste {
        /// Where devicectl writes what it read.
        json_output: &'a str,
    },
}

/// The simulated-location capability, as a device lists it.
pub const CAPABILITY_SIMULATE_LOCATION: &str = "com.apple.coredevice.feature.simulatelocation";

/// What `simctl location start` does when told no speed and no update
/// rule (`simctl help location`): 20 m/s, a fix every second. devicectl
/// has no defaults — a route without all three is a bare validation
/// error — so the same trait call is given the same meaning here.
const ROUTE_DEFAULT_SPEED_MPS: f64 = 20.0;
const ROUTE_UPDATE_INTERVAL_S: f64 = 1.0;

/// How far an echoed coordinate may sit from the one sent and still be
/// the same one: a tenth of a metre, far inside what a decimal printed
/// and parsed again can drift and far outside a swapped pair.
const SAME_COORDINATE_DEG: f64 = 1e-6;

/// A route in the JSON `devicectl … route --route-file` reads.
///
/// devicectl takes a single waypoint; `simctl` and the flow parser do
/// not, and one point is not a journey on either backend.
fn route_file_json(
    points: &[(f64, f64)],
    speed_mps: Option<f64>,
) -> Result<String, DeviceControlError> {
    if points.len() < 2 {
        return Err(DeviceControlError::Malformed {
            subcommand: "devicectl device simulate location route".into(),
            detail: format!("requires ≥2 waypoints, got {}", points.len()),
        });
    }
    let waypoints: Vec<serde_json::Value> = points
        .iter()
        .map(|(latitude, longitude)| {
            serde_json::json!({ "latitude": latitude, "longitude": longitude })
        })
        .collect();
    Ok(serde_json::json!({
        "mode": "interval",
        "interval": ROUTE_UPDATE_INTERVAL_S,
        "speed": speed_mps.unwrap_or(ROUTE_DEFAULT_SPEED_MPS),
        "waypoints": waypoints,
    })
    .to_string())
}

/// devicectl's JSON, once it says the command succeeded.
///
/// The exit code says devicectl ran; `info.outcome` says what happened.
fn successful_result(
    json: &str,
    subcommand: &str,
) -> Result<serde_json::Value, DeviceControlError> {
    let malformed = |detail: String| DeviceControlError::Malformed {
        subcommand: subcommand.into(),
        detail,
    };
    let mut doc: serde_json::Value =
        serde_json::from_str(json).map_err(|e| malformed(e.to_string()))?;
    let outcome = doc["info"]["outcome"].as_str().unwrap_or("(absent)");
    if outcome != "success" {
        return Err(malformed(format!(
            "outcome is {outcome:?}, not \"success\""
        )));
    }
    Ok(doc["result"].take())
}

/// There is no verb that reads a device's simulated location back, so
/// what devicectl says it set is the only account there is. It is held
/// against what was sent.
fn location_echo_agrees(
    json: &str,
    latitude: f64,
    longitude: f64,
) -> Result<(), DeviceControlError> {
    const VERB: &str = "devicectl device simulate location coordinate";
    let result = successful_result(json, VERB)?;
    let said = |key: &str| {
        result[key]
            .as_f64()
            .ok_or_else(|| DeviceControlError::Malformed {
                subcommand: VERB.into(),
                detail: format!("result.{key} is missing or not a number"),
            })
    };
    let (said_lat, said_lon) = (said("latitude")?, said("longitude")?);
    if (said_lat - latitude).abs() > SAME_COORDINATE_DEG
        || (said_lon - longitude).abs() > SAME_COORDINATE_DEG
    {
        return Err(DeviceControlError::Malformed {
            subcommand: VERB.into(),
            detail: format!(
                "sent ({latitude}, {longitude}) and devicectl says it set ({said_lat}, {said_lon})"
            ),
        });
    }
    Ok(())
}

/// What devicectl says it did with a `clear`.
///
/// `cleared: true` is not a reading of the device — devicectl answers it
/// with nothing being simulated at all — so this checks that the
/// instruction was carried out and says exactly that much. A `false`, or
/// a result that has stopped carrying the field, is not a clear.
fn clear_echo_agrees(json: &str) -> Result<(), DeviceControlError> {
    const VERB: &str = "devicectl device simulate location clear";
    let result = successful_result(json, VERB)?;
    match result["cleared"].as_bool() {
        Some(true) => Ok(()),
        other => Err(DeviceControlError::Malformed {
            subcommand: VERB.into(),
            detail: format!("result.cleared is {other:?}, not true"),
        }),
    }
}

fn route_echo_agrees(
    json: &str,
    waypoints: usize,
    speed_mps: f64,
) -> Result<(), DeviceControlError> {
    const VERB: &str = "devicectl device simulate location route";
    let result = successful_result(json, VERB)?;
    let said_points = result["waypointsCount"].as_u64();
    let said_speed = result["speed"].as_f64();
    if said_points != Some(waypoints as u64) || said_speed != Some(speed_mps) {
        return Err(DeviceControlError::Malformed {
            subcommand: VERB.into(),
            detail: format!(
                "sent {waypoints} waypoint(s) at {speed_mps} m/s and devicectl says \
                 {said_points:?} at {said_speed:?}"
            ),
        });
    }
    Ok(())
}

/// The pasteboard capability, as a device lists it.
pub const CAPABILITY_PASTEBOARD: &str = "com.apple.coredevice.feature.pasteboard";

/// The text `devicectl device pasteboard paste` printed, checked against
/// what its JSON says it printed.
///
/// The bytes are the user's data, so nothing here is lossy: a replacement
/// character would make a round trip compare equal on bytes that were not.
/// A pasteboard holding no text is `""` — devicectl exits 0 with nothing
/// on stdout and a `contentSize` of 0.
pub fn pasteboard_text(stdout: Vec<u8>, json: &str) -> Result<String, DeviceControlError> {
    const VERB: &str = "devicectl device pasteboard paste";
    let malformed = |detail: String| DeviceControlError::Malformed {
        subcommand: VERB.into(),
        detail,
    };
    let result = successful_result(json, VERB)?;
    let said = result["contentSize"]
        .as_u64()
        .ok_or_else(|| malformed("result.contentSize is missing or not a byte count".into()))?;
    if said != stdout.len() as u64 {
        return Err(malformed(format!(
            "result.contentSize is {said} and stdout carried {} byte(s)",
            stdout.len()
        )));
    }
    String::from_utf8(stdout).map_err(|e| malformed(format!("the pasted bytes are not UTF-8: {e}")))
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
    const VERB: &str = "devicectl device capture screenshot";
    let malformed = |detail: String| DeviceControlError::Malformed {
        subcommand: VERB.into(),
        detail,
    };
    let result = successful_result(json, VERB)?;
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

    async fn pasteboard_set(&self, _udid: &str, text: &str) -> Result<(), DeviceControlError> {
        let (_, json) = capture_scratch("json");
        let fed = run_feeding(
            &self.argv(DevicectlVerb::PasteboardCopy {
                json_output: &json.to_string_lossy(),
            }),
            text.as_bytes(),
        )
        .await;
        // devicectl's account of the copy is not read; the file is only
        // there because `--json-output` keeps its prose off stdout.
        let _ = tokio::fs::remove_file(&json).await; // absent when devicectl failed before writing it
        fed.map(|_| ())
    }

    async fn pasteboard_get(&self, _udid: &str) -> Result<String, DeviceControlError> {
        let (_, json) = capture_scratch("json");
        let stdout = run_feeding(
            &self.argv(DevicectlVerb::PasteboardPaste {
                json_output: &json.to_string_lossy(),
            }),
            &[],
        )
        .await?;
        let said = tokio::fs::read_to_string(&json).await?;
        tokio::fs::remove_file(&json).await?;
        pasteboard_text(stdout, &said)
    }

    async fn add_media(&self, _udid: &str, _paths: &[String]) -> Result<(), DeviceControlError> {
        Err(refused("add_media"))
    }

    async fn location_set(
        &self,
        _udid: &str,
        lat: f64,
        lon: f64,
    ) -> Result<(), DeviceControlError> {
        let (_, json) = capture_scratch("json");
        run(&self.argv(DevicectlVerb::LocationCoordinate {
            latitude: lat,
            longitude: lon,
            json_output: &json.to_string_lossy(),
        }))
        .await?;
        let said = tokio::fs::read_to_string(&json).await?;
        tokio::fs::remove_file(&json).await?;
        location_echo_agrees(&said, lat, lon)
    }

    async fn location_start(
        &self,
        _udid: &str,
        points: &[(f64, f64)],
        speed_mps: Option<f64>,
    ) -> Result<(), DeviceControlError> {
        let route = route_file_json(points, speed_mps)?;
        let (route_file, json) = capture_scratch("route.json");
        tokio::fs::write(&route_file, route).await?;
        // Returns at once — measured at 0.18 s — and the device goes on
        // travelling, as `simctl location start` does.
        let ran = run(&self.argv(DevicectlVerb::LocationRoute {
            route_file: &route_file.to_string_lossy(),
            json_output: &json.to_string_lossy(),
        }))
        .await;
        tokio::fs::remove_file(&route_file).await?;
        ran?;
        let said = tokio::fs::read_to_string(&json).await?;
        tokio::fs::remove_file(&json).await?;
        route_echo_agrees(
            &said,
            points.len(),
            speed_mps.unwrap_or(ROUTE_DEFAULT_SPEED_MPS),
        )
    }

    async fn location_clear(&self, _udid: &str) -> Result<(), DeviceControlError> {
        let (_, json) = capture_scratch("json");
        run(&self.argv(DevicectlVerb::LocationClear {
            json_output: &json.to_string_lossy(),
        }))
        .await?;
        let said = tokio::fs::read_to_string(&json).await?;
        tokio::fs::remove_file(&json).await?;
        clear_echo_agrees(&said)
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

    async fn reverse_port(
        &self,
        _udid: &str,
        _device_port: u16,
        _host_port: u16,
    ) -> Result<(), DeviceControlError> {
        Err(refused("reverse_port"))
    }

    async fn reverse_port_remove(
        &self,
        _udid: &str,
        _device_port: u16,
    ) -> Result<(), DeviceControlError> {
        Err(refused("reverse_port_remove"))
    }

    async fn wake(&self, _udid: &str) -> Result<(), DeviceControlError> {
        Err(refused("wake"))
    }

    async fn set_stay_awake(&self, _udid: &str, _on: bool) -> Result<(), DeviceControlError> {
        Err(refused("set_stay_awake"))
    }

    async fn frontmost_app(&self, _udid: &str) -> Result<Option<Frontmost>, DeviceControlError> {
        Err(refused("frontmost_app"))
    }

    async fn crash_reports(&self, _udid: &str) -> Result<Vec<CrashReport>, DeviceControlError> {
        Err(refused("crash_reports"))
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
    let out = succeeded(args, cmd.output().await?)?;
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// `run` for the verbs whose stdin or stdout is the user's data: the
/// bytes go in and come back as they are, never through a lossy string.
async fn run_feeding(args: &[String], stdin: &[u8]) -> Result<Vec<u8>, DeviceControlError> {
    use tokio::io::AsyncWriteExt;

    let mut child = tokio::process::Command::new("xcrun")
        .arg("devicectl")
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    let mut pipe = child.stdin.take().expect("stdin was piped two lines up");
    let written = pipe.write_all(stdin).await;
    // devicectl reads to end of input; the pipe has to close for it to go on.
    drop(pipe);
    // A devicectl that refuses before reading closes the pipe, and its
    // stderr says why — a broken pipe does not. Its exit is asked first.
    let out = succeeded(args, child.wait_with_output().await?)?;
    written?;
    Ok(out.stdout)
}

fn succeeded(
    args: &[String],
    out: std::process::Output,
) -> Result<std::process::Output, DeviceControlError> {
    if out.status.success() {
        return Ok(out);
    }
    Err(DeviceControlError::non_zero_exit(
        args.first().map_or("devicectl", String::as_str),
        out.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&out.stderr).as_ref(),
    ))
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
        //
        // 14 until the same release closed two more: `pasteboard_set` and
        // `pasteboard_get` go through `device pasteboard copy` / `paste`,
        // written and read back byte for byte on an iPhone.
        //
        // 12 until `location_set` and `location_start` followed, through
        // `device simulate location coordinate` / `route`.
        //
        // 10 until `reverse_port` and `reverse_port_remove` arrived in
        // the same release. Those two move the count the other way and
        // they are not a gap: a phone refuses them because Apple's USB
        // channel has no reverse direction for anything to drive, so
        // there is no verb for devicectl to grow.
        //
        // 16 as of the release that added `wake`, `set_stay_awake`,
        // `frontmost_app` and `crash_reports`. Like the two above these
        // move the count up without being a gap: no devicectl verb
        // changes a device's power state or names its frontmost app,
        // and its crash reports stay on the device.
        assert_eq!(
            checked, 16,
            "the phone refuses 16 of these; this says {checked}"
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
        "pasteboard_set",
        "pasteboard_get",
        "location_set",
        "location_start",
        "location_clear",
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
                "add_media" => err_of(c.add_media(&udid, &[]).await),
                "start_recording" => err_of(
                    c.start_recording(&udid, std::path::Path::new("/tmp/x"))
                        .await,
                ),
                "stop_recording" => err_of(c.stop_recording().await),
                "screenshot" => err_of(c.screenshot(&udid).await.map(|_| ())),
                "capture_bgra" => err_of(c.capture_bgra(&udid).await.map(|_| ())),
                "set_animations_quiet" => err_of(c.set_animations_quiet(&udid, true).await),
                "reverse_port" => err_of(c.reverse_port(&udid, 8080, 8080).await),
                "reverse_port_remove" => err_of(c.reverse_port_remove(&udid, 8080).await),
                "wake" => err_of(c.wake(&udid).await),
                "set_stay_awake" => err_of(c.set_stay_awake(&udid, true).await),
                "frontmost_app" => err_of(c.frontmost_app(&udid).await.map(|_| ())),
                "crash_reports" => err_of(c.crash_reports(&udid).await.map(|_| ())),
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
        matches!(
            name,
            "screenshot"
                | "capture_bgra"
                | "pasteboard_get"
                // Reads of the device rather than of the binding: each
                // one asks the hardware a question, so each one owes an
                // answer or a refusal.
                | "frontmost_app"
                | "crash_reports"
        )
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

#[cfg(test)]
mod pasteboard_tests {
    use super::*;

    const UDID: &str = "00000000-0000000000000000";
    const PASTE_TEXT: &str =
        include_str!("../tests/fixtures/devicectl/pasteboard-paste.text.sim.json");
    const PASTE_NO_TEXT: &str =
        include_str!("../tests/fixtures/devicectl/pasteboard-paste.no-text.sim.json");

    // 30 bytes, as the fixture's contentSize says: multi-byte characters
    // and an inner newline, no trailing one.
    const THIRTY_BYTES: &str = "smix 剪贴板\nsecond line ✓";

    #[test]
    fn copy_and_paste_name_the_device_and_only_name_a_pasteboard_when_told_to() {
        let general = DevicectlClient::new(UDID);
        assert_eq!(
            general.argv(DevicectlVerb::PasteboardCopy {
                json_output: "/tmp/a.json"
            }),
            [
                "device",
                "pasteboard",
                "copy",
                "--device",
                UDID,
                "--json-output",
                "/tmp/a.json"
            ]
        );
        let named = DevicectlClient::new(UDID).on_pasteboard("smix.e2e.probe");
        assert_eq!(
            named.argv(DevicectlVerb::PasteboardPaste {
                json_output: "/tmp/a.json"
            }),
            [
                "device",
                "pasteboard",
                "paste",
                "--device",
                UDID,
                "--device-pasteboard",
                "smix.e2e.probe",
                "--json-output",
                "/tmp/a.json"
            ]
        );
    }

    #[test]
    fn what_was_pasted_is_the_bytes_devicectl_says_it_sent() {
        assert_eq!(THIRTY_BYTES.len(), 30);
        let text = pasteboard_text(THIRTY_BYTES.as_bytes().to_vec(), PASTE_TEXT).expect("30 of 30");
        assert_eq!(text, THIRTY_BYTES);

        let short = pasteboard_text(THIRTY_BYTES.as_bytes()[..29].to_vec(), PASTE_TEXT)
            .expect_err("29 bytes against a contentSize of 30");
        assert!(short.to_string().contains("contentSize"), "{short}");
    }

    #[test]
    fn a_pasteboard_with_no_text_on_it_reads_as_empty() {
        assert_eq!(
            pasteboard_text(Vec::new(), PASTE_NO_TEXT).expect("empty"),
            ""
        );
    }

    #[test]
    fn bytes_that_are_not_text_are_not_turned_into_text() {
        let json = r#"{"info":{"outcome":"success"},"result":{"contentSize":2}}"#;
        let err = pasteboard_text(vec![0xff, 0xfe], json).expect_err("not UTF-8");
        assert!(err.to_string().contains("UTF-8"), "{err}");
    }
}

#[cfg(test)]
mod location_tests {
    use super::*;

    const UDID: &str = "00000000-0000000000000000";
    const COORDINATE: &str =
        include_str!("../tests/fixtures/devicectl/location-coordinate.sim.json");
    const ROUTE: &str = include_str!("../tests/fixtures/devicectl/location-route.sim.json");

    #[test]
    fn a_negative_coordinate_is_attached_to_its_flag() {
        // `--longitude -122.4194` is refused: devicectl reads the value
        // as another option. Only a device could say so; this holds the
        // form that device accepted.
        let c = DevicectlClient::new(UDID);
        assert_eq!(
            c.argv(DevicectlVerb::LocationCoordinate {
                latitude: 37.7749,
                longitude: -122.4194,
                json_output: "/tmp/a.json"
            }),
            [
                "device",
                "simulate",
                "location",
                "coordinate",
                "--device",
                UDID,
                "--latitude=37.7749",
                "--longitude=-122.4194",
                "--json-output",
                "/tmp/a.json"
            ]
        );
        assert_eq!(
            c.argv(DevicectlVerb::LocationRoute {
                route_file: "/tmp/r.json",
                json_output: "/tmp/a.json"
            }),
            [
                "device",
                "simulate",
                "location",
                "route",
                "--device",
                UDID,
                "--route-file",
                "/tmp/r.json",
                "--json-output",
                "/tmp/a.json"
            ]
        );
    }

    #[test]
    fn clearing_a_location_names_the_device_and_asks_for_the_answer() {
        let c = DevicectlClient::new(UDID);
        assert_eq!(
            c.argv(DevicectlVerb::LocationClear {
                json_output: "/tmp/a.json"
            }),
            [
                "device",
                "simulate",
                "location",
                "clear",
                "--device",
                UDID,
                "--json-output",
                "/tmp/a.json"
            ]
        );
    }

    #[test]
    fn a_clear_that_the_device_did_not_accept_is_not_a_clear() {
        // `cleared: true` is devicectl saying it carried the instruction
        // out — it answers that with nothing being simulated too, so it
        // is not a reading of the device. What it can still catch is the
        // shape changing under us, which is why the field is read rather
        // than the exit code trusted.
        let ok = r#"{"info":{"outcome":"success"},"result":{"cleared":true}}"#;
        assert!(clear_echo_agrees(ok).is_ok());

        let refused = r#"{"info":{"outcome":"success"},"result":{"cleared":false}}"#;
        let said = format!("{:?}", clear_echo_agrees(refused).unwrap_err());
        assert!(
            said.contains("cleared"),
            "the failure does not say what devicectl answered: {said}"
        );

        let shapeless = r#"{"info":{"outcome":"success"},"result":{}}"#;
        assert!(
            clear_echo_agrees(shapeless).is_err(),
            "a result with no `cleared` field was read as a successful clear"
        );
    }

    #[test]
    fn a_route_is_written_the_way_devicectl_reads_one() {
        let points = [(-33.8688, 151.2093), (-33.9, 151.3)];
        let doc: serde_json::Value =
            serde_json::from_str(&route_file_json(&points, Some(5.0)).expect("two points"))
                .expect("it is JSON");
        assert_eq!(doc["mode"], "interval");
        assert_eq!(doc["interval"], 1.0);
        assert_eq!(doc["speed"], 5.0);
        assert_eq!(doc["waypoints"][0]["latitude"], -33.8688);
        assert_eq!(doc["waypoints"][1]["longitude"], 151.3);
        assert_eq!(doc["waypoints"].as_array().map(Vec::len), Some(2));

        let unhurried: serde_json::Value =
            serde_json::from_str(&route_file_json(&points, None).expect("two points"))
                .expect("it is JSON");
        assert_eq!(unhurried["speed"], 20.0);

        let one = route_file_json(&points[..1], None).expect_err("one point is not a route");
        assert!(one.to_string().contains("waypoints"), "{one}");
    }

    #[test]
    fn what_devicectl_says_it_set_is_held_against_what_was_sent() {
        assert!(location_echo_agrees(COORDINATE, 37.7749, -122.4194).is_ok());
        let swapped =
            location_echo_agrees(COORDINATE, -122.4194, 37.7749).expect_err("the other way round");
        let msg = swapped.to_string();
        assert!(
            msg.contains("37.7749") && msg.contains("-122.4194"),
            "{msg}"
        );

        assert!(route_echo_agrees(ROUTE, 2, 20.0).is_ok());
        assert!(route_echo_agrees(ROUTE, 3, 20.0).is_err());
        assert!(route_echo_agrees(ROUTE, 2, 5.0).is_err());
    }
}

//! Reading what devicectl answers: the JSON each verb writes with
//! `--json-output`, and the prose where the JSON does not carry it.

use smix_simctl::DeviceControlError;

use super::{ROUTE_DEFAULT_SPEED_MPS, ROUTE_UPDATE_INTERVAL_S};

/// How far an echoed coordinate may sit from the one sent and still be
/// the same one: a tenth of a metre, far inside what a decimal printed
/// and parsed again can drift and far outside a swapped pair.
pub(super) const SAME_COORDINATE_DEG: f64 = 1e-6;

/// A route in the JSON `devicectl … route --route-file` reads.
///
/// devicectl takes a single waypoint; `simctl` and the flow parser do
/// not, and one point is not a journey on either backend.
pub(super) fn route_file_json(
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
pub(super) fn successful_result(
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
pub(super) fn location_echo_agrees(
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
pub(super) fn clear_echo_agrees(json: &str) -> Result<(), DeviceControlError> {
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

pub(super) fn route_echo_agrees(
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

/// The launched process's pid, from `--json-output -`.
///
/// Xcode 27's devicectl prints no `pid:` in its prose (measured against an
/// iOS 27 simulator, 2026-09-29); the JSON carries it as
/// `result.process.processIdentifier`. The prose is still read when the
/// JSON is not there, which is what an older devicectl prints.
pub(super) fn launched_pid(stdout: &str) -> Option<u32> {
    serde_json::from_str::<serde_json::Value>(stdout)
        .ok()
        .and_then(|v| v["result"]["process"]["processIdentifier"].as_u64())
        .and_then(|n| u32::try_from(n).ok())
        .or_else(|| parse_pid(stdout))
}

pub(super) fn parse_pid(stdout: &str) -> Option<u32> {
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

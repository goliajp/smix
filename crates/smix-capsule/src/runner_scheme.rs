//! Which scheme builds the runner for a simulator.
//!
//! One scheme, `SmixRunner`, builds for iPhone, iPad, Vision Pro and Apple
//! TV: xcodebuild reads the platform from the destination. A watch app
//! cannot be one of a multiplatform target's destinations, so the watch
//! runner is its own pair of targets under `SmixRunnerWatch`.

use std::process::Command;

/// The scheme for `udid`: `SmixRunnerWatch` for a watchOS simulator,
/// `SmixRunner` for everything else — including a device simctl does not
/// list, such as a phone, which the other scheme has always built.
#[must_use]
pub fn for_device(udid: &str) -> &'static str {
    let listed = Command::new("xcrun")
        .args(["simctl", "list", "devices", "-j"])
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| serde_json::from_slice::<serde_json::Value>(&o.stdout).ok());
    match listed.and_then(|v| runtime_of(&v, udid)) {
        Some(runtime) if is_watch(&runtime) => "SmixRunnerWatch",
        _ => "SmixRunner",
    }
}

/// The runtime identifier `simctl list devices -j` files `udid` under.
fn runtime_of(listing: &serde_json::Value, udid: &str) -> Option<String> {
    listing["devices"]
        .as_object()?
        .iter()
        .find_map(|(runtime, devices)| {
            devices
                .as_array()?
                .iter()
                .any(|d| {
                    d["udid"]
                        .as_str()
                        .is_some_and(|u| u.eq_ignore_ascii_case(udid))
                })
                .then(|| runtime.clone())
        })
}

fn is_watch(runtime: &str) -> bool {
    runtime.contains("SimRuntime.watchOS")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listing() -> serde_json::Value {
        serde_json::json!({ "devices": {
            "com.apple.CoreSimulator.SimRuntime.watchOS-27-2": [{ "udid": "W-1", "name": "Watch" }],
            "com.apple.CoreSimulator.SimRuntime.tvOS-27-2": [{ "udid": "T-1", "name": "TV" }],
            "com.apple.CoreSimulator.SimRuntime.iOS-27-0": [{ "udid": "P-1", "name": "Phone" }]
        }})
    }

    #[test]
    fn a_watch_is_found_under_its_runtime_and_the_rest_are_not_watches() {
        let l = listing();
        assert!(is_watch(&runtime_of(&l, "w-1").unwrap()));
        assert!(!is_watch(&runtime_of(&l, "T-1").unwrap()));
        assert!(!is_watch(&runtime_of(&l, "P-1").unwrap()));
        assert_eq!(runtime_of(&l, "absent"), None);
    }
}

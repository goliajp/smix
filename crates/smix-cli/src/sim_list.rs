//! `smix sim list`: every simulator and Android device this host can see.

use smix_simctl::{DeviceControlError, SimctlClient};

use crate::{CliError, android_devices, may_ask_release};

pub(crate) async fn cmd_sim_list(simctl: &SimctlClient, json: bool) -> Result<(), CliError> {
    let devices = match simctl.list_devices().await {
        Ok(d) => d,
        // A host with no Xcode has no simulators to list, and its Android
        // devices are still there: the whole command used to fail here
        // before it reached them.
        Err(e) if no_xcrun(&e) => {
            eprintln!("no Xcode on this host (xcrun not found), so no iOS simulators are listed");
            Vec::new()
        }
        Err(e) => return Err(e.into()),
    };
    let android = android_devices();
    if json {
        let mut all: Vec<serde_json::Value> = Vec::new();
        for d in &devices {
            let mut v =
                serde_json::to_value(d).map_err(|e| CliError::Other(format!("serialize: {e}")))?;
            if let Some(obj) = v.as_object_mut() {
                obj.insert("platform".into(), "ios".into());
            }
            all.push(v);
        }
        for a in &android {
            all.push(serde_json::json!({
                "platform": "android",
                "udid": a.serial,
                "name": if a.model.is_empty() { a.serial.clone() } else { a.model.clone() },
                "state": a.state,
                "release": a.release,
                "registered": a.registered,
            }));
        }
        let out = serde_json::to_string_pretty(&all)
            .map_err(|e| CliError::Other(format!("serialize: {e}")))?;
        println!("{}", out);
        return Ok(());
    }
    // Compact human-readable table.
    println!("{:<40} {:<28} {:<10} RUNTIME", "UDID", "NAME", "STATE");
    for d in &devices {
        let runtime_short = d
            .runtime_identifier
            .rsplit('.')
            .next()
            .unwrap_or(d.runtime_identifier.as_str());
        println!(
            "{:<40} {:<28} {:<10} {runtime_short}",
            d.udid, d.name, d.state
        );
    }
    for a in &android {
        let name = if a.model.is_empty() {
            a.serial.as_str()
        } else {
            a.model.as_str()
        };
        let release = if !may_ask_release(&a.serial, a.registered) {
            "Android (not registered; smix does not address it)".to_string()
        } else if a.release.is_empty() {
            "Android".to_string()
        } else {
            format!("Android-{}", a.release)
        };
        println!("{:<40} {:<28} {:<10} {release}", a.serial, name, a.state);
    }
    Ok(())
}

/// Whether `simctl` could not be run because `xcrun` is not installed.
fn no_xcrun(e: &DeviceControlError) -> bool {
    matches!(e, DeviceControlError::Spawn(io) if io.kind() == std::io::ErrorKind::NotFound)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_xcrun_is_a_host_without_xcode() {
        let e = DeviceControlError::Spawn(std::io::Error::from(std::io::ErrorKind::NotFound));
        assert!(no_xcrun(&e));
    }

    #[test]
    fn any_other_failure_to_start_it_is_still_an_error() {
        let e =
            DeviceControlError::Spawn(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        assert!(!no_xcrun(&e));
    }
}

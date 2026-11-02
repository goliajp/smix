//! The app's own log on its device, as a live source for `expect.signal`:
//! the iOS unified log by subsystem, Android's logcat by the app's uid.

use crate::entry::FlowPlatform;

/// The program and arguments that print `bundle_id`'s log on `device` from
/// now on. On Android the app is named by its uid, which `uid` gives (read
/// with [`android_uid`]); `None` there means the package is not installed.
pub(crate) fn command(
    platform: FlowPlatform,
    device: &str,
    bundle_id: &str,
    uid: Option<u32>,
) -> Option<(String, Vec<String>)> {
    match platform {
        FlowPlatform::Ios => Some((
            "xcrun".into(),
            vec![
                "simctl".into(),
                "spawn".into(),
                device.into(),
                "log".into(),
                "stream".into(),
                "--level".into(),
                "debug".into(),
                "--style".into(),
                "compact".into(),
                "--predicate".into(),
                format!("subsystem == \"{}\"", bundle_id.replace('"', "")),
            ],
        )),
        FlowPlatform::Android => uid.map(|uid| {
            (
                "adb".into(),
                vec![
                    "-s".into(),
                    device.into(),
                    "logcat".into(),
                    "-v".into(),
                    "brief".into(),
                    // the newest line only, then what comes after it
                    "-T".into(),
                    "1".into(),
                    format!("--uid={uid}"),
                ],
            )
        }),
    }
}

/// `bundle_id`'s uid from `pm list packages -U` output.
pub(crate) fn uid_from_packages(listing: &str, bundle_id: &str) -> Option<u32> {
    listing.lines().find_map(|l| {
        let rest = l.trim().strip_prefix("package:")?;
        let (pkg, uid) = rest.split_once(" uid:")?;
        (pkg == bundle_id)
            .then(|| uid.trim().parse().ok())
            .flatten()
    })
}

/// The uid Android runs `bundle_id` as on `device`.
pub(crate) fn android_uid(device: &str, bundle_id: &str) -> Option<u32> {
    let out = std::process::Command::new("adb")
        .args([
            "-s", device, "shell", "pm", "list", "packages", "-U", bundle_id,
        ])
        .output()
        .ok()?;
    uid_from_packages(&String::from_utf8_lossy(&out.stdout), bundle_id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ios_streams_the_apps_subsystem_live() {
        let (prog, args) = command(FlowPlatform::Ios, "UDID", "com.example.shop", None).unwrap();
        assert_eq!(prog, "xcrun");
        assert!(args.contains(&"stream".to_string()), "live, not `log show`");
        assert_eq!(args.last().unwrap(), "subsystem == \"com.example.shop\"");
    }

    #[test]
    fn android_reads_the_apps_uid_from_now() {
        let listing =
            "package:com.example.shop.debug uid:10190\npackage:com.example.shop uid:10189\n";
        assert_eq!(uid_from_packages(listing, "com.example.shop"), Some(10189));
        assert_eq!(uid_from_packages(listing, "com.q"), None);
        let (_, args) = command(
            FlowPlatform::Android,
            "emulator-5554",
            "com.example.shop",
            Some(10189),
        )
        .unwrap();
        assert!(args.contains(&"--uid=10189".to_string()));
        assert!(command(FlowPlatform::Android, "emulator-5554", "x", None).is_none());
    }
}

//! The `formFactor` of a listed device: `phone`, `tablet`, `watch`, `tv` or
//! `vision`, or no field when the device does not say.

/// A simulator's form factor, from its device type identifier, e.g.
/// `com.apple.CoreSimulator.SimDeviceType.iPad-Pro-13-inch-M4-8GB`.
pub(crate) fn of_simulator(device_type_identifier: &str) -> Option<&'static str> {
    let kind = device_type_identifier.rsplit('.').next()?;
    [
        ("iPad", "tablet"),
        ("iPhone", "phone"),
        ("iPod", "phone"),
        ("Apple-Watch", "watch"),
        ("Apple-TV", "tv"),
        ("Apple-Vision", "vision"),
    ]
    .into_iter()
    .find_map(|(prefix, form)| kind.starts_with(prefix).then_some(form))
}

/// A running Android device's form factor, read in one shell.
pub(crate) fn of_android(serial: &str) -> Option<&'static str> {
    let out = std::process::Command::new(&smix_adb::adb_path().0)
        .args([
            "-s",
            serial,
            "shell",
            "getprop ro.build.characteristics; echo @; wm size; echo @; wm density",
        ])
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    of_android_answer(&String::from_utf8_lossy(&out.stdout))
}

/// The build's own word first (`tablet`, `tv`, `watch` in
/// `ro.build.characteristics`); a build that names none of them is a tablet
/// when its smallest width is 600 dp or more, which is the line Android's
/// own `sw600dp` resources draw.
fn of_android_answer(out: &str) -> Option<&'static str> {
    let mut parts = out.split('@');
    let characteristics = parts.next()?;
    let named = characteristics
        .trim()
        .split(',')
        .find_map(|c| match c.trim() {
            "tablet" => Some("tablet"),
            "tv" => Some("tv"),
            "watch" => Some("watch"),
            _ => None,
        });
    if named.is_some() {
        return named;
    }
    let (w, h) = last_value(parts.next()?, "size:")?
        .split_once('x')
        .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)))?;
    let density: u32 = last_value(parts.next()?, "density:")?.parse().ok()?;
    if density == 0 {
        return None;
    }
    let smallest_dp = w.min(h) * 160 / density;
    Some(if smallest_dp >= 600 {
        "tablet"
    } else {
        "phone"
    })
}

/// The value of the last line ending a `… size:` / `… density:` label: an
/// `Override` line follows the `Physical` one and is what apps are given.
fn last_value<'a>(block: &'a str, label: &str) -> Option<&'a str> {
    block
        .lines()
        .filter_map(|l| l.split_once(label).map(|(_, v)| v.trim()))
        .next_back()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_simulator_is_named_by_its_device_type() {
        let t = |s: &str| of_simulator(&format!("com.apple.CoreSimulator.SimDeviceType.{s}"));
        assert_eq!(t("iPad-Pro-13-inch-M4-8GB"), Some("tablet"));
        assert_eq!(t("iPad-mini-A17-Pro"), Some("tablet"));
        assert_eq!(t("iPhone-17-Pro"), Some("phone"));
        assert_eq!(t("Apple-Watch-Series-11-46mm"), Some("watch"));
        assert_eq!(t("Apple-TV-4K-3rd-generation-4K"), Some("tv"));
        assert_eq!(t("Apple-Vision-Pro"), Some("vision"));
        assert_eq!(of_simulator(""), None);
        assert_eq!(t("Something-Else"), None);
    }

    #[test]
    fn the_build_naming_tablet_is_a_tablet_whatever_its_width() {
        let out = "tablet\n@\nPhysical size: 1080x2340\n@\nPhysical density: 440\n";
        assert_eq!(of_android_answer(out), Some("tablet"));
        assert_eq!(
            of_android_answer(
                "nosdcard,tv\n@\nPhysical size: 1920x1080\n@\nPhysical density: 320\n"
            ),
            Some("tv")
        );
    }

    #[test]
    fn a_build_that_does_not_say_is_judged_by_its_smallest_width() {
        // 1080 px at 440 dpi is 392 dp
        let phone = "emulator\n@\nPhysical size: 1080x2340\n@\nPhysical density: 440\n";
        assert_eq!(of_android_answer(phone), Some("phone"));
        // 1600 px at 320 dpi is 800 dp, whichever way it is turned
        let tablet = "default\n@\nPhysical size: 2560x1600\n@\nPhysical density: 320\n";
        assert_eq!(of_android_answer(tablet), Some("tablet"));
    }

    #[test]
    fn an_override_is_what_apps_are_given() {
        let out = "phone\n@\nPhysical size: 1080x2340\nOverride size: 2000x2400\n@\nPhysical density: 440\nOverride density: 320\n";
        assert_eq!(of_android_answer(out), Some("tablet"));
    }

    #[test]
    fn an_answer_without_a_size_says_nothing() {
        assert_eq!(of_android_answer("phone\n@\n@\n"), None);
        assert_eq!(of_android_answer(""), None);
        assert_eq!(
            of_android_answer("phone\n@\nPhysical size: 1080x2340\n@\nPhysical density: 0\n"),
            None
        );
    }
}

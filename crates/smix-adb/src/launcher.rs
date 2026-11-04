//! Which activity starts an app, and whether the device lets it start.
//!
//! `cmd package resolve-activity` answers `No activity found` for every
//! app that is not direct-boot aware while the user is still locked (a
//! PIN set and not yet entered since the boot). The package's own filter
//! table in `dumpsys package` lists the launcher either way, and
//! `dumpsys user` says whether the user is locked.

/// The categories an app is started through, in the order tried: a
/// phone's launcher, then a TV's.
const LAUNCHERS: [&str; 2] = [
    "android.intent.category.LAUNCHER",
    "android.intent.category.LEANBACK_LAUNCHER",
];
const MAIN: &str = "android.intent.action.MAIN";

/// The activity `am start -n <pkg>/<this>` takes, from `dumpsys package
/// <pkg>`: the first MAIN filter in a launcher category of the
/// `Activity Resolver Table`.
#[must_use]
pub fn launcher_from_dumpsys(text: &str, pkg: &str) -> Option<String> {
    let component = format!("{pkg}/");
    // (action heading, activity, the filter's Action / Category lines)
    let mut filters: Vec<(&str, &str, Vec<&str>)> = Vec::new();
    let mut heading = "";
    let mut in_table = false;
    for line in text.lines() {
        let t = line.trim();
        if !line.starts_with(' ') && !t.is_empty() {
            in_table = t == "Activity Resolver Table:";
            continue;
        }
        if !in_table || t.is_empty() {
            continue;
        }
        let words: Vec<&str> = t.split_whitespace().collect();
        if let [_, name, "filter", _] = words[..] {
            filters.push((heading, name, Vec::new()));
        } else if let Some(h) = t.strip_suffix(':').filter(|h| !h.contains(' ')) {
            heading = h;
        } else if let Some((_, _, lines)) = filters.last_mut() {
            lines.push(t);
        }
    }
    let quoted = |key: &str, value: &str| format!("{key}: \"{value}\"");
    LAUNCHERS.iter().find_map(|category| {
        filters
            .iter()
            .filter(|(h, _, lines)| *h == MAIN || lines.contains(&quoted("Action", MAIN).as_str()))
            .find(|(_, _, lines)| lines.contains(&quoted("Category", category).as_str()))
            .and_then(|(_, name, _)| name.strip_prefix(&component))
            .map(relative)
    })
}

/// `.Main` names a class relative to the package and `com.x.ui.Main` one
/// in full; a bare `Main` is relative.
fn relative(activity: &str) -> String {
    if activity.contains('.') {
        activity.to_string()
    } else {
        format!(".{activity}")
    }
}

/// Whether the current user is still locked, from `dumpsys user`'s
/// `Started users state: [0=RUNNING_LOCKED]`. `None` when the line is
/// not there to read.
#[must_use]
pub fn user_is_locked(dumpsys_user: &str) -> Option<bool> {
    let line = dumpsys_user
        .lines()
        .find_map(|l| l.trim().strip_prefix("Started users state:"))?;
    let state = line.trim().trim_matches(['[', ']']).split(',').next()?;
    let (_, state) = state.trim().split_once('=')?;
    match state {
        "RUNNING_LOCKED" | "RUNNING_UNLOCKING" => Some(true),
        "RUNNING_UNLOCKED" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // `adb shell dumpsys package dev.smix.fixture` on an API 36 emulator
    // with a PIN set, rebooted and not unlocked (2026-11-04)
    const LOCKED: &str = include_str!("../tests/fixtures/dumpsys-package-locked-api36.txt");

    #[test]
    fn the_launcher_is_read_from_the_filter_table_while_locked() {
        assert_eq!(
            launcher_from_dumpsys(LOCKED, "dev.smix.fixture").as_deref(),
            Some(".MainActivity")
        );
        assert_eq!(launcher_from_dumpsys(LOCKED, "dev.smix.other"), None);
    }

    // the shape of a consumer app's table on API 36: a phone
    // entry and a TV entry, neither called `.MainActivity`
    const TWO_ENTRIES: &str = "Activity Resolver Table:
  Non-Data Actions:
      android.intent.action.MAIN:
        1b440a6 com.x/.nativeapp.tv.TvActivity filter bd64ae7
          Action: \"android.intent.action.MAIN\"
          Category: \"android.intent.category.LEANBACK_LAUNCHER\"
        42bca85 com.x/.nativeapp.MainActivity filter 637f0da
          Action: \"android.intent.action.MAIN\"
          Category: \"android.intent.category.LAUNCHER\"
      android.intent.action.VIEW:
        5aa0011 com.x/.Links filter 1100aa0
          Action: \"android.intent.action.VIEW\"
          Category: \"android.intent.category.LAUNCHER\"

Receiver Resolver Table:
  Non-Data Actions:
      android.intent.action.MAIN:
        77aa001 com.x/.Rx filter 0011aa7
          Category: \"android.intent.category.LAUNCHER\"
";

    #[test]
    fn a_phone_launcher_comes_before_a_tv_one_and_other_tables_do_not_count() {
        assert_eq!(
            launcher_from_dumpsys(TWO_ENTRIES, "com.x").as_deref(),
            Some(".nativeapp.MainActivity")
        );
        // the phone entry's category is the line before the VIEW heading
        let phone = "android.intent.category.LAUNCHER\"\n      android.intent.action.VIEW";
        assert_eq!(TWO_ENTRIES.matches(phone).count(), 1);
        let tv_only = TWO_ENTRIES.replace(
            phone,
            "android.intent.category.DEFAULT\"\n      android.intent.action.VIEW",
        );
        assert_eq!(
            launcher_from_dumpsys(&tv_only, "com.x").as_deref(),
            Some(".nativeapp.tv.TvActivity")
        );
    }

    #[test]
    fn the_user_state_reads_locked_unlocked_or_unknown() {
        // `dumpsys user` lines from the same emulator, locked then unlocked
        assert_eq!(
            user_is_locked("  Started users state: [0=RUNNING_LOCKED]\n"),
            Some(true)
        );
        assert_eq!(
            user_is_locked("  Started users state: [0=RUNNING_UNLOCKED]\n"),
            Some(false)
        );
        assert_eq!(user_is_locked("Users:\n  UserInfo{0:Owner:c13}\n"), None);
    }
}

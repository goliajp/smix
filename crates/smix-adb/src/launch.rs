//! Starting an activity, and knowing whether it got as far as drawing.
//!
//! `am start` returns once the intent is delivered. On a loaded device the
//! app can take longer than the next step's whole wait to draw its first
//! frame, and that step then fails reading a screen that still belongs to
//! the launcher or the status bar. `am start -W` returns when the activity
//! has drawn, which is when a launch is done.

use std::time::Duration;

use crate::{AdbClient, AdbError};

/// How long a launch waits for the app to draw before going on without it.
pub const LAUNCH_DRAWS_WITHIN: Duration = Duration::from_secs(20);

/// What became of a launch that was delivered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Launched {
    /// The activity drew its first frame, `total_ms` after the start
    /// (`None` when Android did not time it: an activity already in front).
    Drew { total_ms: Option<u64> },
    /// Android stopped waiting and said so (`Status: timeout`, or any status
    /// other than `ok`, kept verbatim).
    NotDrawn { status: String },
    /// `am start -W` was still waiting when [`LAUNCH_DRAWS_WITHIN`] ran out.
    StillWaiting,
}

impl AdbClient {
    /// `adb -s <serial> shell am start -n <pkg>/<activity> [args]` — launch
    /// app's activity. Returns once the intent is delivered.
    pub async fn start_activity(
        &self,
        serial: &str,
        package: &str,
        activity: &str,
        extras: &[(&str, &str)],
    ) -> Result<(), AdbError> {
        let args = start_args(false, package, activity, extras);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run_capture(Some(serial), "shell", &arg_refs).await?;
        Ok(())
    }

    /// `adb -s <serial> shell am start -W -n <pkg>/<activity> [args]` —
    /// launch app's activity and wait, up to [`LAUNCH_DRAWS_WITHIN`], for it
    /// to draw.
    pub async fn start_activity_until_drawn(
        &self,
        serial: &str,
        package: &str,
        activity: &str,
        extras: &[(&str, &str)],
    ) -> Result<Launched, AdbError> {
        let args = start_args(true, package, activity, extras);
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        match tokio::time::timeout(
            LAUNCH_DRAWS_WITHIN,
            self.run_capture(Some(serial), "shell", &arg_refs),
        )
        .await
        {
            Ok(done) => Ok(parse_wait_status(&done?.0)),
            Err(_) => Ok(Launched::StillWaiting),
        }
    }
}

fn start_args(wait: bool, package: &str, activity: &str, extras: &[(&str, &str)]) -> Vec<String> {
    let mut args = vec!["am".to_string(), "start".to_string()];
    if wait {
        args.push("-W".to_string());
    }
    args.push("-n".to_string());
    args.push(format!("{package}/{activity}"));
    for (k, v) in extras {
        args.push("--es".to_string());
        args.push((*k).to_string());
        args.push((*v).to_string());
    }
    args
}

/// Read `am start -W`'s report. Output without a `Status:` line (an older
/// `am`, or an `Error:` that `am` still exits 0 on) is taken as drawn: that
/// is what a launch without `-W` assumed, and an error there has always been
/// left to the next step to meet.
fn parse_wait_status(stdout: &str) -> Launched {
    let field = |name: &str| {
        stdout
            .lines()
            .find_map(|l| l.trim().strip_prefix(name).map(str::trim))
    };
    match field("Status:") {
        Some("ok") | None => Launched::Drew {
            total_ms: field("TotalTime:").and_then(|t| t.parse().ok()),
        },
        Some(other) => Launched::NotDrawn {
            status: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // emulator-5554, android-36, 2026-11-01
    const COLD: &str = "Starting: Intent { cmp=com.android.settings/.Settings }\n\
        Status: ok\nLaunchState: COLD\n\
        Activity: com.android.settings/.homepage.SettingsHomepageActivity\n\
        TotalTime: 1146\nWaitTime: 1148\nComplete\n";
    const IN_FRONT: &str = "Starting: Intent { cmp=com.android.settings/.Settings }\n\
        Status: ok\nLaunchState: UNKNOWN (0)\n\
        Activity: com.android.settings/.homepage.SettingsHomepageActivity\n\
        WaitTime: 19\nComplete\n";

    #[test]
    fn a_drawn_launch_carries_its_time() {
        assert_eq!(
            parse_wait_status(COLD),
            Launched::Drew {
                total_ms: Some(1146)
            }
        );
        assert_eq!(parse_wait_status(IN_FRONT), Launched::Drew { total_ms: None });
    }

    #[test]
    fn a_status_other_than_ok_is_kept_verbatim() {
        let out = COLD.replace("Status: ok", "Status: timeout");
        assert_eq!(
            parse_wait_status(&out),
            Launched::NotDrawn {
                status: "timeout".into()
            }
        );
    }

    #[test]
    fn no_status_line_is_what_a_plain_start_assumed() {
        assert_eq!(
            parse_wait_status("Starting: Intent { cmp=a/.B }\n"),
            Launched::Drew { total_ms: None }
        );
    }

    #[test]
    fn only_the_waiting_start_asks_android_to_wait() {
        assert_eq!(
            start_args(true, "p", ".A", &[("k", "v")]),
            ["am", "start", "-W", "-n", "p/.A", "--es", "k", "v"]
        );
        assert_eq!(start_args(false, "p", ".A", &[]), ["am", "start", "-n", "p/.A"]);
    }
}

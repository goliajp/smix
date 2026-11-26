//! `smix sim log`: an emulator's logcat, narrowed by app, time, text and
//! priority, printed once or followed for a set time.

use std::time::Duration;

use chrono::{DateTime, Local, NaiveTime, TimeZone};

use crate::CliError;

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct LogArgs {
    /// Which emulator, by serial or registry alias.
    pub device: String,
    /// Only lines the app logged: the package name, matched by the uid it
    /// runs as. The app must be installed. A system app that shares a uid
    /// (Settings runs as 1000) gets every line of that uid.
    #[arg(long)]
    pub app: Option<String>,
    /// From this time on: a span back from now (`90s`, `10m`, `2h`) or a
    /// time today on this Mac's clock (`14:05`, `14:05:30`). Without it,
    /// everything the log buffer still holds (and, with `--follow`, only
    /// what comes after now).
    #[arg(long)]
    pub since: Option<String>,
    /// Only lines matching this regular expression (logcat's own `-e`).
    #[arg(long)]
    pub grep: Option<String>,
    /// The lowest priority shown: V, D, I, W, E or F.
    #[arg(long, value_parser = parse_level)]
    pub level: Option<char>,
    /// Keep printing new lines until `--timeout` runs out.
    #[arg(long, requires = "timeout")]
    pub follow: bool,
    /// With `--follow`: how many seconds to keep printing.
    #[arg(long, requires = "follow")]
    pub timeout: Option<u64>,
}

fn parse_level(s: &str) -> Result<char, String> {
    let c = s.trim().to_ascii_uppercase();
    match c.as_str() {
        "V" | "D" | "I" | "W" | "E" | "F" => Ok(c.chars().next().unwrap_or('V')),
        _ => Err(format!("expected one of V, D, I, W, E, F; got `{s}`")),
    }
}

/// `--since` as logcat's `sss.mmm` epoch form, read against `now`.
pub(crate) fn since_epoch(text: &str, now: DateTime<Local>) -> Result<String, String> {
    let t = text.trim();
    let span = |n: &str, unit: i64| n.parse::<i64>().ok().map(|n| n * unit);
    let back = match t.char_indices().last() {
        Some((i, 's')) => span(&t[..i], 1),
        Some((i, 'm')) => span(&t[..i], 60),
        Some((i, 'h')) => span(&t[..i], 3600),
        _ => None,
    };
    let at = if let Some(secs) = back {
        now - chrono::Duration::seconds(secs)
    } else {
        let time = NaiveTime::parse_from_str(t, "%H:%M:%S")
            .or_else(|_| NaiveTime::parse_from_str(t, "%H:%M"))
            .map_err(|_| {
                format!(
                    "--since {text:?} is neither a span back from now (90s, 10m, 2h) \
                     nor a time today (14:05, 14:05:30)"
                )
            })?;
        Local
            .from_local_datetime(&now.date_naive().and_time(time))
            .earliest()
            .ok_or_else(|| format!("--since {text:?} does not exist today on this clock"))?
    };
    if at > now {
        return Err(format!("--since {text:?} is later than now"));
    }
    Ok(format!(
        "{}.{:03}",
        at.timestamp(),
        at.timestamp_subsec_millis()
    ))
}

/// logcat's arguments, after `adb -s <serial>`.
pub(crate) fn logcat_argv(
    since: Option<&str>,
    uid: Option<u32>,
    grep: Option<&str>,
    level: Option<char>,
    follow: bool,
) -> Vec<String> {
    let mut argv: Vec<String> = ["logcat", "-v", "threadtime"].map(String::from).to_vec();
    if !follow {
        argv.push("-d".into());
    }
    match (since, follow) {
        (Some(t), _) => argv.extend(["-T".into(), t.to_string()]),
        // followed from now: the newest line, then what comes after it
        (None, true) => argv.extend(["-T".into(), "1".into()]),
        (None, false) => {}
    }
    if let Some(uid) = uid {
        argv.push(format!("--uid={uid}"));
    }
    if let Some(re) = grep {
        argv.extend(["-e".into(), re.to_string()]);
    }
    if let Some(level) = level {
        argv.push(format!("*:{level}"));
    }
    argv
}

/// The uid `package` runs as, from `pm list packages -U` output.
fn uid_of(listing: &str, package: &str) -> Option<u32> {
    listing.lines().find_map(|l| {
        let (pkg, uid) = l.trim().strip_prefix("package:")?.split_once(" uid:")?;
        (pkg == package).then(|| uid.trim().parse().ok()).flatten()
    })
}

pub async fn run(args: LogArgs) -> Result<(), CliError> {
    let fail = |s: String| CliError::Other(s);
    let serial = crate::resolve_android_serial(&args.device)?;
    let since = args
        .since
        .as_deref()
        .map(|t| since_epoch(t, Local::now()))
        .transpose()
        .map_err(fail)?;
    let uid = match &args.app {
        None => None,
        Some(pkg) => {
            let listing = smix_adb::AdbClient::new()
                .shell(&serial, &["pm", "list", "packages", "-U", pkg])
                .await
                .map_err(|e| fail(e.to_string()))?;
            Some(uid_of(&listing, pkg).ok_or_else(|| {
                fail(format!(
                    "{pkg} is not installed on {serial}, so it has no log lines"
                ))
            })?)
        }
    };
    let argv = logcat_argv(
        since.as_deref(),
        uid,
        args.grep.as_deref(),
        args.level,
        args.follow,
    );
    let mut cmd: tokio::process::Command = smix_adb::std_command().map_err(fail)?.into();
    cmd.arg("-s").arg(&serial).args(&argv).kill_on_drop(true);
    let mut child = cmd
        .spawn()
        .map_err(|e| fail(format!("could not run adb: {e}")))?;
    let status = match args.timeout.filter(|_| args.follow) {
        Some(secs) => match tokio::time::timeout(Duration::from_secs(secs), child.wait()).await {
            Ok(status) => status,
            Err(_) => {
                child.kill().await.map_err(|e| fail(e.to_string()))?;
                eprintln!("log: stopped following after {secs}s");
                return Ok(());
            }
        },
        None => child.wait().await,
    }
    .map_err(|e| fail(e.to_string()))?;
    if status.success() {
        Ok(())
    } else {
        Err(fail(format!(
            "adb -s {serial} {} exited {status}",
            argv.join(" ")
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> DateTime<Local> {
        Local.with_ymd_and_hms(2026, 11, 27, 14, 30, 0).unwrap()
    }

    #[test]
    fn since_is_a_span_back_or_a_time_today() {
        let n = now().timestamp();
        assert_eq!(since_epoch("90s", now()), Ok(format!("{}.000", n - 90)));
        assert_eq!(since_epoch("10m", now()), Ok(format!("{}.000", n - 600)));
        assert_eq!(since_epoch("2h", now()), Ok(format!("{}.000", n - 7200)));
        assert_eq!(
            since_epoch("14:05", now()),
            Ok(format!("{}.000", n - 25 * 60))
        );
        assert_eq!(
            since_epoch("14:29:30", now()),
            Ok(format!("{}.000", n - 30))
        );
        for bad in ["", "10", "m", "ten minutes", "25:00", "-5m"] {
            assert!(since_epoch(bad, now()).is_err(), "{bad:?} was accepted");
        }
        assert!(since_epoch("15:00", now()).is_err(), "later than now");
    }

    #[test]
    fn logcat_is_asked_for_exactly_what_was_narrowed() {
        assert_eq!(
            logcat_argv(None, None, None, None, false),
            ["logcat", "-v", "threadtime", "-d"]
        );
        assert_eq!(
            logcat_argv(
                Some("1.000"),
                Some(10123),
                Some("Reconnect|TLS"),
                Some('W'),
                false
            ),
            [
                "logcat",
                "-v",
                "threadtime",
                "-d",
                "-T",
                "1.000",
                "--uid=10123",
                "-e",
                "Reconnect|TLS",
                "*:W"
            ]
        );
        assert_eq!(
            logcat_argv(None, None, None, None, true),
            ["logcat", "-v", "threadtime", "-T", "1"]
        );
    }

    #[test]
    fn the_apps_uid_is_read_from_the_package_listing() {
        let listing = "package:com.x.app.debug uid:10200\npackage:com.x.app uid:10199\n";
        assert_eq!(uid_of(listing, "com.x.app"), Some(10199));
        assert_eq!(uid_of(listing, "com.x"), None);
        assert_eq!(parse_level("w"), Ok('W'));
        assert!(parse_level("warn").is_err());
    }
}

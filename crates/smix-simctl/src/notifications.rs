//! A simulator's notifications: what an app was delivered, whether it may
//! be delivered any, and handing it a push.
//!
//! The delivered ones and the authorization are files the simulator keeps
//! on this Mac, so reading them needs no runner and changes nothing on
//! screen.

use crate::bplist::{self, Value};
use crate::poster_store::data_dir;
use crate::{DeviceControlError, SimctlClient, simctl_run};
use std::path::Path;
use std::time::{Duration, Instant};

const STORE: &str = "Library/UserNotifications";
const SECTIONS: &str = "Library/BulletinBoard/VersionedSectionInfo.plist";
/// SpringBoard's record for an app that was asked and answered, as iOS
/// 26.5 and 27 write it; the two answers differ in the one number.
const SECTION: &str = include_str!("section_info.plist.xml");
/// 2001-01-01 UTC in Unix seconds, where a property list's dates start.
const APPLE_EPOCH: f64 = 978_307_200.0;

/// One notification an app was delivered and has not been dismissed.
#[derive(Debug, Clone, PartialEq)]
pub struct Delivered {
    /// The simulator's own id for it.
    pub id: String,
    /// As shown; empty when it has none, like the two below.
    pub title: String,
    /// The line under the title.
    pub subtitle: String,
    /// The message.
    pub body: String,
    /// Unix seconds, when the simulator recorded one.
    pub at: Option<f64>,
}

/// Whether an app may post notifications.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authorization {
    /// It has not asked, or its answer was reset.
    NotDetermined,
    /// Asked, and refused.
    Denied,
    /// Allowed, in full or provisionally.
    Allowed,
}

fn refuse(what: &str, why: String) -> DeviceControlError {
    DeviceControlError::non_zero_exit(what, 1, why)
}

/// A property list file, `None` when there is none. SpringBoard writes
/// some as XML and some as binary; `plutil` reads either.
fn plist(path: &Path) -> Result<Option<Value>, String> {
    let failed = |e: &dyn std::fmt::Display| format!("{}: {e}", path.display());
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(failed(&e)),
    };
    let binary = if bytes.starts_with(b"bplist") {
        bytes
    } else {
        let out = std::process::Command::new("plutil")
            .args(["-convert", "binary1", "-o", "-"])
            .arg(path)
            .output()
            .map_err(|e| failed(&e))?;
        if !out.status.success() {
            let said = [out.stdout, out.stderr].concat();
            return Err(failed(&String::from_utf8_lossy(&said).trim()));
        }
        out.stdout
    };
    bplist::parse(&binary).map(Some).map_err(|e| failed(&e))
}

/// The notifications `bundle` was delivered, newest first, from the store
/// under `data`.
pub(crate) fn delivered_in(data: &Path, bundle: &str) -> Result<Vec<Delivered>, String> {
    let store = data.join(STORE);
    let Some(library) = plist(&store.join("Library.plist"))? else {
        return Ok(Vec::new());
    };
    let library = bplist::unarchive(&library)?;
    let Some(dir) = library.get(bundle).and_then(Value::as_str) else {
        return Ok(Vec::new());
    };
    let Some(file) = plist(&store.join(dir).join("DeliveredNotifications.plist"))? else {
        return Ok(Vec::new());
    };
    let Value::Array(items) = bplist::unarchive(&file)? else {
        return Ok(Vec::new());
    };
    let mut out: Vec<Delivered> = items
        .iter()
        .map(|n| {
            let text = |k: &str| {
                n.get(k)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            Delivered {
                id: text("AppNotificationIdentifier"),
                title: text("AppNotificationTitle"),
                subtitle: text("AppNotificationSubtitle"),
                body: text("AppNotificationMessage"),
                at: match n.get("AppNotificationCreationDate") {
                    Some(Value::Date(t)) => Some(t + APPLE_EPOCH),
                    _ => None,
                },
            }
        })
        .collect();
    out.sort_by(|a, b| b.at.partial_cmp(&a.at).unwrap_or(std::cmp::Ordering::Equal));
    Ok(out)
}

/// Whether `bundle` may post notifications, by the record under `data`.
pub(crate) fn authorization_in(data: &Path, bundle: &str) -> Result<Authorization, String> {
    let Some(sections) = plist(&data.join(SECTIONS))? else {
        return Ok(Authorization::NotDetermined);
    };
    let Some(Value::Data(archive)) = sections.get("sectionInfo").and_then(|s| s.get(bundle)) else {
        return Ok(Authorization::NotDetermined);
    };
    let section = bplist::unarchive(&bplist::parse(archive)?)?;
    let status = section
        .get("sectionInfoSettings")
        .and_then(|s| s.get("authorizationStatus"))
        .and_then(Value::as_int);
    match status {
        Some(0) => Ok(Authorization::NotDetermined),
        Some(1) => Ok(Authorization::Denied),
        // 2 authorized, 3 provisional, 4 ephemeral
        Some(2..=4) => Ok(Authorization::Allowed),
        other => Err(format!(
            "{bundle}'s notification authorization reads {other:?}, which is none this knows"
        )),
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = c
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (u32::from(*b) << (16 - 8 * i)));
        for i in 0..4 {
            if i <= c.len() {
                out.push(char::from(ALPHABET[(n >> (18 - 6 * i) & 63) as usize]));
            } else {
                out.push('=');
            }
        }
    }
    out
}

fn xml_escaped(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

async fn plutil(args: &[&str], stdin: Option<&[u8]>) -> Result<Vec<u8>, DeviceControlError> {
    use tokio::io::AsyncWriteExt;
    let failed = |e: std::io::Error| refuse("plutil", e.to_string());
    let mut child = tokio::process::Command::new("plutil")
        .args(args)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(failed)?;
    if let Some(mut pipe) = child.stdin.take() {
        pipe.write_all(stdin.unwrap_or_default())
            .await
            .map_err(failed)?;
    }
    let out = child.wait_with_output().await.map_err(failed)?;
    if !out.status.success() {
        let said = [out.stdout, out.stderr].concat();
        return Err(refuse("plutil", String::from_utf8_lossy(&said).into()));
    }
    Ok(out.stdout)
}

impl SimctlClient {
    /// The notifications `bundle` was delivered and not dismissed, newest
    /// first.
    pub async fn delivered_notifications(
        &self,
        udid: &str,
        bundle: &str,
    ) -> Result<Vec<Delivered>, DeviceControlError> {
        let data =
            data_dir(udid).ok_or_else(|| refuse("notifications", "HOME is not set".into()))?;
        // a file being replaced reads as damaged; the next look settles it
        match delivered_in(&data, bundle) {
            Ok(n) => Ok(n),
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(200)).await;
                delivered_in(&data, bundle).map_err(|e| refuse("notifications", e))
            }
        }
    }

    /// Whether `bundle` may post notifications.
    pub async fn notification_authorization(
        &self,
        udid: &str,
        bundle: &str,
    ) -> Result<Authorization, DeviceControlError> {
        let data =
            data_dir(udid).ok_or_else(|| refuse("notifications", "HOME is not set".into()))?;
        authorization_in(&data, bundle).map_err(|e| refuse("notifications", e))
    }

    /// Set whether `bundle` may post notifications, as if its prompt had
    /// been answered (or never shown, for `NotDetermined`).
    ///
    /// Written to SpringBoard's record, which it reads when it starts, so
    /// SpringBoard is restarted: whatever app is open ends, and so does a
    /// UI-test session. Nothing is written when it already is as asked.
    ///
    /// Returns whether it had to change anything.
    pub async fn set_notification_authorization(
        &self,
        udid: &str,
        bundle: &str,
        name: &str,
        to: Authorization,
    ) -> Result<bool, DeviceControlError> {
        let data =
            data_dir(udid).ok_or_else(|| refuse("notifications", "HOME is not set".into()))?;
        let is = authorization_in(&data, bundle).map_err(|e| refuse("notifications", e))?;
        if is == to {
            return Ok(false);
        }
        let file = data.join(SECTIONS);
        let path = file.to_string_lossy().into_owned();
        let key = format!("sectionInfo.{}", bundle.replace('.', "\\."));
        let tmp = format!("{path}.smix");
        match to {
            Authorization::NotDetermined => {
                plutil(&["-remove", &key, "-o", &tmp, &path], None).await?;
            }
            Authorization::Denied | Authorization::Allowed => {
                let status = if to == Authorization::Allowed {
                    "2"
                } else {
                    "1"
                };
                let xml = SECTION
                    .replace("@BUNDLE@", &xml_escaped(bundle))
                    .replace("@NAME@", &xml_escaped(name))
                    .replace("@STATUS@", status);
                let archive = plutil(
                    &["-convert", "binary1", "-o", "-", "-"],
                    Some(xml.as_bytes()),
                )
                .await?;
                let b64 = base64(&archive);
                plutil(&["-replace", &key, "-data", &b64, "-o", &tmp, &path], None).await?;
            }
        }
        std::fs::rename(&tmp, &file)
            .map_err(|e| refuse("notifications", format!("{path}: {e}")))?;
        self.restart_springboard(udid).await?;
        let now = authorization_in(&data, bundle).map_err(|e| refuse("notifications", e))?;
        if now == to {
            return Ok(true);
        }
        Err(refuse(
            "notifications",
            format!("written as {to:?}, but after SpringBoard restarted {bundle} reads {now:?}"),
        ))
    }

    async fn springboard_pid(&self, udid: &str) -> Result<Option<u32>, DeviceControlError> {
        let list = simctl_run(&["spawn", udid, "launchctl", "list"]).await?;
        Ok(pid_of(&list, "com.apple.SpringBoard"))
    }

    /// Restart SpringBoard and return once a new one has its home screen's
    /// services up.
    async fn restart_springboard(&self, udid: &str) -> Result<(), DeviceControlError> {
        let before = self.springboard_pid(udid).await?;
        simctl_run(&["spawn", udid, "launchctl", "stop", "com.apple.SpringBoard"]).await?;
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let looked = Instant::now();
            let pid = self.springboard_pid(udid).await?;
            if pid.is_some() && pid != before {
                break;
            }
            if looked >= deadline {
                return Err(refuse(
                    "launchctl stop",
                    format!("{udid}: SpringBoard did not come back within 60 s"),
                ));
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
        // answers once SpringBoard can be asked to open apps
        simctl_run(&["bootstatus", udid]).await?;
        Ok(())
    }

    /// The name `bundle` shows on the home screen, from its Info.plist.
    pub async fn app_name(&self, udid: &str, bundle: &str) -> Result<String, DeviceControlError> {
        let app = simctl_run(&["get_app_container", udid, bundle, "app"]).await?;
        let info = Path::new(app.trim()).join("Info.plist");
        let info = plist(&info)
            .map_err(|e| refuse("notifications", e))?
            .unwrap_or(Value::Null);
        Ok(["CFBundleDisplayName", "CFBundleName"]
            .iter()
            .find_map(|k| info.get(k).and_then(Value::as_str))
            .unwrap_or(bundle)
            .to_string())
    }

    /// `simctl push`, with `payload` as JSON text.
    pub async fn push_payload(
        &self,
        udid: &str,
        bundle: &str,
        payload: &str,
    ) -> Result<(), DeviceControlError> {
        let file = std::env::temp_dir().join(format!(
            "smix-push-{}-{}.json",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos())
        ));
        std::fs::write(&file, payload)
            .map_err(|e| refuse("push", format!("{}: {e}", file.display())))?;
        let sent = simctl_run(&["push", udid, bundle, &file.to_string_lossy()]).await;
        std::fs::remove_file(&file)
            .map_err(|e| refuse("push", format!("{}: {e}", file.display())))?;
        sent.map(drop)
    }
}

/// `launchctl list`'s pid for `label`: `pid  status  label`.
fn pid_of(list: &str, label: &str) -> Option<u32> {
    list.lines().find_map(|l| {
        let mut f = l.split_whitespace();
        let (pid, _status, name) = (f.next()?, f.next()?, f.next()?);
        (name == label).then(|| pid.parse().ok())?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const APP: &str = "jp.golia.smix.fixture";

    /// Stores a simulator wrote: two notifications, and the prompt allowed.
    fn data() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/notification-data")
    }

    #[test]
    fn what_an_app_was_delivered_reads_newest_first() {
        let n = delivered_in(&data(), APP).expect("read");
        let words: Vec<(&str, &str)> = n
            .iter()
            .map(|d| (d.title.as_str(), d.body.as_str()))
            .collect();
        assert_eq!(words, [("T2", "B1"), ("T1", "B1")]);
        assert!(n[0].at > n[1].at);
        assert!(n[0].at.unwrap() > 1_700_000_000.0);
        assert_ne!(n[0].id, n[1].id);
    }

    #[test]
    fn an_app_delivered_nothing_and_a_simulator_with_no_store_read_empty() {
        assert_eq!(delivered_in(&data(), "com.example.none"), Ok(vec![]));
        assert_eq!(
            delivered_in(&data().join("no-such-simulator"), APP),
            Ok(vec![])
        );
    }

    #[test]
    fn authorization_reads_from_the_record_and_its_absence() {
        assert_eq!(authorization_in(&data(), APP), Ok(Authorization::Allowed));
        assert_eq!(
            authorization_in(&data(), "com.example.none"),
            Ok(Authorization::NotDetermined)
        );
    }

    #[test]
    fn the_written_record_says_what_it_is_asked_to() {
        assert!(SECTION.contains("@BUNDLE@") && SECTION.contains("@NAME@"));
        assert_eq!(SECTION.matches("@STATUS@").count(), 1);
        assert_eq!(xml_escaped("A & <B>"), "A &amp; &lt;B&gt;");
    }

    #[test]
    fn base64_pads() {
        assert_eq!(base64(b"a"), "YQ==");
        assert_eq!(base64(b"ab"), "YWI=");
        assert_eq!(base64(b"abc"), "YWJj");
        assert_eq!(base64(&[0xFB, 0xFF]), "+/8=");
    }

    #[test]
    fn a_pid_is_the_labels_own() {
        let list =
            "PID\tStatus\tLabel\n-\t0\tcom.apple.SpringBoardX\n34835\t-9\tcom.apple.SpringBoard\n";
        assert_eq!(pid_of(list, "com.apple.SpringBoard"), Some(34835));
        assert_eq!(pid_of(list, "com.apple.SpringBoardX"), None);
    }
}

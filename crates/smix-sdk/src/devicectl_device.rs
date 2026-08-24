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
//! `location_set` and the pasteboard are among the fifteen.
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

use crate::device_control::{DeviceControl, Permission, PermissionAction};

/// A physical iOS device, driven through `xcrun devicectl`.
pub struct DevicectlClient {
    udid: String,
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
        Self { udid: udid.into() }
    }

    /// The device this client drives.
    #[must_use]
    pub fn udid(&self) -> &str {
        &self.udid
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
        }
    }
}

/// The `devicectl` operations smix uses.
///
/// Six, because six is what exists. Naming them as a closed set keeps the
/// argv builder honest: a seventh would have to be added here, and adding
/// it would mean finding it in `devicectl --help` first.
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

    async fn pasteboard_set(&self, _udid: &str, _text: &str) -> Result<(), DeviceControlError> {
        Err(refused("pasteboard_set"))
    }

    async fn pasteboard_get(&self, _udid: &str) -> Result<String, DeviceControlError> {
        Err(refused("pasteboard_get"))
    }

    async fn add_media(&self, _udid: &str, _paths: &[String]) -> Result<(), DeviceControlError> {
        Err(refused("add_media"))
    }

    async fn location_set(
        &self,
        _udid: &str,
        _lat: f64,
        _lon: f64,
    ) -> Result<(), DeviceControlError> {
        Err(refused("location_set"))
    }

    async fn location_start(
        &self,
        _udid: &str,
        _points: &[(f64, f64)],
        _speed_mps: Option<f64>,
    ) -> Result<(), DeviceControlError> {
        Err(refused("location_start"))
    }

    async fn start_recording(
        &self,
        _udid: &str,
        _output_path: &Path,
    ) -> Result<(), DeviceControlError> {
        Err(refused("start_recording"))
    }

    async fn stop_recording(&self) -> Result<(), DeviceControlError> {
        Err(refused("stop_recording"))
    }

    async fn screenshot(&self, _udid: &str) -> Result<Vec<u8>, DeviceControlError> {
        // Not a gap in the product, only in this layer: the runner takes
        // screenshots through XCUIScreen on both platforms, and that is
        // the path smix uses. Refusing here points at it rather than
        // implying the capability is missing.
        Err(refused("screenshot"))
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
    let out = cmd.output().await?;
    if !out.status.success() {
        return Err(DeviceControlError::non_zero_exit(
            args.first().map_or("devicectl", String::as_str),
            out.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&out.stderr).as_ref(),
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
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
        assert_eq!(
            checked, 17,
            "the phone refuses 17 of these; this says {checked}"
        );
    }

    use super::*;

    const UDID: &str = "00008120-001410C11A42201E";

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
            (
                "start_recording",
                err(c.start_recording(UDID, Path::new("/tmp/x.mov")).await),
            ),
            ("add_media", err(c.add_media(UDID, &[]).await)),
            ("location_set", err(c.location_set(UDID, 1.0, 2.0).await)),
            (
                "pasteboard_get",
                err(c.pasteboard_get(UDID).await.map(|_| ())),
            ),
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
        assert!(c.pasteboard_set(UDID, "x").await.is_err());
        assert!(c.stop_recording().await.is_err());
        assert!(c.screenshot(UDID).await.is_err());
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
    /// Six, and the list is here rather than derived so that adding a
    /// seventh is a deliberate edit — the kind that makes someone check
    /// `devicectl --help` first.
    const IMPLEMENTED: &[&str] = &[
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
        let c = DevicectlClient::new("00008120-001410C11A42201E");
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
                "pasteboard_set" => err_of(c.pasteboard_set(&udid, "x").await),
                "pasteboard_get" => err_of(c.pasteboard_get(&udid).await.map(|_| ())),
                "add_media" => err_of(c.add_media(&udid, &[]).await),
                "location_set" => err_of(c.location_set(&udid, 0.0, 0.0).await),
                "location_start" => err_of(c.location_start(&udid, &[], None).await),
                "start_recording" => err_of(
                    c.start_recording(&udid, std::path::Path::new("/tmp/x"))
                        .await,
                ),
                "stop_recording" => err_of(c.stop_recording().await),
                "screenshot" => err_of(c.screenshot(&udid).await.map(|_| ())),
                "capture_bgra" => err_of(c.capture_bgra(&udid).await.map(|_| ())),
                "set_animations_quiet" => err_of(c.set_animations_quiet(&udid, true).await),
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
        matches!(name, "screenshot" | "capture_bgra" | "pasteboard_get")
    }

    fn err_of<T>(r: Result<T, DeviceControlError>) -> Option<String> {
        r.err().map(|e| e.to_string())
    }
}

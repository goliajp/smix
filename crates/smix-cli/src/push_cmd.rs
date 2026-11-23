//! `smix sim push` — a whole push payload handed to an app: APNs on a
//! simulator, FCM on an emulator.

use smix_sdk::android_fcm::{self, PushRefused};
use smix_simctl::notifications::Authorization;
use smix_simctl::registry::DeviceKind;
use smix_simctl::{DeviceControlError, SimctlClient};
use std::io::Read;
use std::process::ExitCode;

/// What `simctl push` takes, in bytes.
const APNS_LIMIT: usize = 4096;

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct PushArgs {
    /// Which simulator or emulator, by UDID, serial or registry alias.
    pub device: String,
    /// The app it is for.
    #[arg(long)]
    pub bundle: String,
    /// A JSON file, or `-` for standard input. On iOS an APNs payload
    /// (`aps` and the app's own keys, nested as they are); on Android an
    /// FCM message (`notification`, `data`, `android`), with or without
    /// the outer `message`.
    #[arg(long, value_name = "FILE")]
    pub payload: String,
}

fn code(why: &PushRefused) -> u8 {
    match why {
        PushRefused::Device(_) => 1,
        PushRefused::NotInstalled(_) => 4,
        PushRefused::NotAllowed(_) => 5,
        PushRefused::Payload(_) => 6,
    }
}

fn read(from: &str) -> Result<String, PushRefused> {
    let read = if from == "-" {
        let mut s = String::new();
        std::io::stdin().read_to_string(&mut s).map(|_| s)
    } else {
        std::fs::read_to_string(from)
    };
    read.map_err(|e| PushRefused::Payload(format!("cannot read {from}: {e}")))
}

/// The payload as `simctl push` takes it: the key that names the app is
/// dropped, since `--bundle` says which.
fn apns(payload: &str, bundle: &str) -> Result<String, String> {
    let root: serde_json::Value =
        serde_json::from_str(payload).map_err(|e| format!("not JSON: {e}"))?;
    let serde_json::Value::Object(mut root) = root else {
        return Err("not a JSON object".into());
    };
    if let Some(named) = root.remove("Simulator Target Bundle")
        && named != bundle
    {
        return Err(format!(
            "its `Simulator Target Bundle` is {named}, and --bundle is {bundle}"
        ));
    }
    match root.get("aps") {
        Some(serde_json::Value::Object(_)) => {}
        Some(_) => return Err("`aps` is not an object".into()),
        None => {
            return Err("no `aps` object, which is what makes it an APNs payload".into());
        }
    }
    let text = serde_json::Value::Object(root).to_string();
    if text.len() > APNS_LIMIT {
        return Err(format!(
            "it is {} bytes, and APNs takes {APNS_LIMIT}",
            text.len()
        ));
    }
    Ok(text)
}

async fn to_simulator(
    simctl: &SimctlClient,
    udid: &str,
    bundle: &str,
    payload: &str,
) -> Result<Vec<String>, PushRefused> {
    let payload = apns(payload, bundle).map_err(PushRefused::Payload)?;
    let device = |e: DeviceControlError| PushRefused::Device(e.to_string());
    if !simctl
        .app_is_installed(udid, bundle)
        .await
        .map_err(device)?
    {
        return Err(PushRefused::NotInstalled(format!(
            "{bundle} is not installed on {udid}"
        )));
    }
    match simctl.push_payload(udid, bundle, &payload).await {
        Ok(()) => Ok(Vec::new()),
        // simctl's words for a push to an app that may not post one
        Err(e) if e.to_string().contains("Source is not authorized") => {
            let is = match simctl.notification_authorization(udid, bundle).await {
                Ok(Authorization::Denied) => "its user refused",
                Ok(Authorization::NotDetermined) => "it has not asked",
                Ok(Authorization::Allowed) | Err(_) => "simctl says it is not authorized",
            };
            Err(PushRefused::NotAllowed(format!(
                "{bundle} may not show notifications on {udid} ({is}), so the simulator drops the \
                 push; `smix sim permission {udid} {bundle} notifications allow` allows it"
            )))
        }
        Err(e) => Err(device(e)),
    }
}

pub async fn run(simctl: &SimctlClient, args: PushArgs) -> Result<ExitCode, crate::CliError> {
    let ios = crate::device_kind_of(&args.device) == DeviceKind::Simulator;
    let bundle = args.bundle.as_str();
    let device = if ios {
        crate::resolve_device(&args.device)?
    } else {
        crate::resolve_android_serial(&args.device)?
    };
    let sent = match read(&args.payload) {
        Err(e) => Err(e),
        Ok(payload) if ios => to_simulator(simctl, &device, bundle, &payload).await,
        Ok(payload) => {
            // the SDK drops a message whose id it has seen
            let id = format!(
                "smix:{}-{}",
                std::process::id(),
                chrono::Utc::now().timestamp_micros()
            );
            android_fcm::push(&smix_adb::AdbClient::new(), &device, bundle, &payload, &id).await
        }
    };
    match sent {
        Ok(notes) => {
            println!("push: {device} {bundle}");
            for n in notes {
                eprintln!("note: {n}");
            }
            Ok(ExitCode::SUCCESS)
        }
        Err(why) => {
            let kind = match &why {
                PushRefused::Payload(_) => "payload: ",
                _ => "",
            };
            eprintln!("error: push: {kind}{why}");
            Ok(ExitCode::from(code(&why)))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nested_payload_is_kept_and_the_target_key_dropped() {
        let out = apns(
            r#"{"Simulator Target Bundle":"a.b","aps":{"alert":{"title":"T"}},"body":{"x":{"y":[1]}}}"#,
            "a.b",
        )
        .unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["body"]["x"]["y"][0], 1);
        assert!(v.get("Simulator Target Bundle").is_none());
    }

    #[test]
    fn a_payload_apns_would_not_take_says_why() {
        let why = |p: &str| apns(p, "a.b").unwrap_err();
        assert!(why("nope").contains("not JSON"));
        assert!(why("[]").contains("not a JSON object"));
        assert!(why("{}").contains("`aps`"));
        assert!(why(r#"{"aps":"x"}"#).contains("not an object"));
        assert!(why(r#"{"aps":{},"Simulator Target Bundle":"c"}"#).contains("--bundle"));
        let long = format!(r#"{{"aps":{{}},"x":"{}"}}"#, "a".repeat(5000));
        assert!(why(&long).contains("bytes"));
    }

    #[test]
    fn each_refusal_has_an_exit_code_of_its_own() {
        let all = [
            PushRefused::Device(String::new()),
            PushRefused::NotInstalled(String::new()),
            PushRefused::NotAllowed(String::new()),
            PushRefused::Payload(String::new()),
        ];
        assert_eq!(all.each_ref().map(code), [1, 4, 5, 6]);
    }
}

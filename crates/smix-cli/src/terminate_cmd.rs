//! `smix sim terminate` — stop an app, or end it the way the system does.

use smix_sdk::device_control::DeviceControl;
use smix_simctl::registry::DeviceKind;
use std::time::{Duration, Instant};

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct TerminateArgs {
    /// Which device, by UDID or registry alias.
    pub device: String,
    /// The app to stop.
    pub bundle_id: String,
    /// End a background app the way the system does to take its memory
    /// back. On Android that leaves its notifications and lets pushes still
    /// reach it, which stopping it does not; refused while it is in front.
    /// On a simulator it is the same as without it.
    #[arg(long)]
    pub reclaim: bool,
}

pub async fn run(args: TerminateArgs) -> Result<(), crate::CliError> {
    let failed = |e: String| crate::CliError::Other(e);
    let kind = crate::device_kind_of(&args.device);
    let app = args.bundle_id.as_str();
    if args.reclaim && kind == DeviceKind::Emulator {
        let serial = crate::resolve_android_serial(&args.device)?;
        let word = reclaim(&serial, app).await.map_err(failed)?;
        println!("{word}: {app} on {serial}");
        return Ok(());
    }
    if args.reclaim && kind != DeviceKind::Simulator {
        return Err(failed(format!(
            "--reclaim is for simulators and emulators, and {} is a phone",
            args.device
        )));
    }
    let udid = crate::resolve_device(&args.device)?;
    crate::device_control_for(kind, &udid)
        .terminate(&udid, app)
        .await
        .map_err(|e| failed(e.to_string()))?;
    println!("terminated: {app} on {udid}");
    Ok(())
}

async fn running(adb: &smix_adb::AdbClient, serial: &str, app: &str) -> Result<bool, String> {
    // `pidof` exits 1 for none, which is an answer rather than a failure
    let said = adb
        .shell(serial, &["pidof", app, "||", "true"])
        .await
        .map_err(|e| e.to_string())?;
    Ok(!said.trim().is_empty())
}

async fn reclaim(serial: &str, app: &str) -> Result<&'static str, String> {
    let adb = smix_adb::AdbClient::new();
    if !running(&adb, serial, app).await? {
        return Ok("not running");
    }
    let front = smix_sdk::android_device::AndroidDeviceControl::new()
        .frontmost_app(serial)
        .await
        .map_err(|e| e.to_string())?;
    if front.is_some_and(|f| f.package == app) {
        return Err(format!(
            "{app} is in front, and the system never takes the app in front; send it to the \
             background first (`smix press-key home --device {serial}`)"
        ));
    }
    // an app that has just left the front counts as in use for a moment,
    // and `am kill` passes over it without saying so
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        // a look that began before the deadline still counts
        let looked = Instant::now();
        adb.shell(serial, &["am", "kill", app])
            .await
            .map_err(|e| e.to_string())?;
        tokio::time::sleep(Duration::from_millis(300)).await;
        if !running(&adb, serial, app).await? {
            return Ok("reclaimed");
        }
        if looked >= deadline {
            return Err(format!(
                "{app} is still running 15 s on: the system does not reclaim an app that holds \
                 something in use, such as a foreground service"
            ));
        }
    }
}

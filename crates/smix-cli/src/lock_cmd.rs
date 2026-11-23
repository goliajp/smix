//! The screen: `smix sim lock` turns it off, which locks it when the device
//! has a lock screen; `wake` and `stay-awake` turn it on and keep it on.

use smix_simctl::SimctlClient;
use smix_simctl::registry::DeviceKind;
use std::time::{Duration, Instant};

/// What the command line said.
#[derive(clap::Args, Debug)]
pub struct LockArgs {
    /// Which simulator or emulator, by UDID, serial or registry alias.
    pub device: String,
}

pub async fn run(simctl: &SimctlClient, args: LockArgs) -> Result<(), crate::CliError> {
    if crate::device_kind_of(&args.device) == DeviceKind::Simulator {
        let udid = crate::resolve_device(&args.device)?;
        let word = if simctl.lock(&udid).await? {
            "locked"
        } else {
            "already locked"
        };
        println!("lock: {udid} {word}");
        return Ok(());
    }
    let serial = crate::resolve_android_serial(&args.device)?;
    let adb = smix_adb::AdbClient::new();
    let failed = |e: smix_adb::AdbError| crate::CliError::Other(e.to_string());
    adb.shell(&serial, &["input", "keyevent", "KEYCODE_SLEEP"])
        .await
        .map_err(failed)?;
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // a look that began before the deadline still counts
        let looked = Instant::now();
        if adb.wakefulness(&serial).await.map_err(failed)? != Some(smix_adb::Wakefulness::Awake) {
            break;
        }
        if looked >= deadline {
            return Err(crate::CliError::Other(format!(
                "{serial}: the screen is still on 5 s after being put to sleep"
            )));
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    let none = adb
        .shell(&serial, &["locksettings", "get-disabled"])
        .await
        .map_err(failed)?;
    if none.trim() == "true" {
        println!("lock: {serial} screen off (it has no lock screen)");
    } else {
        println!("lock: {serial} locked");
    }
    Ok(())
}

pub async fn wake(device: &str) -> Result<(), crate::CliError> {
    let serial = crate::resolve_android_serial(device)?;
    let control = smix_sdk::android_device::AndroidDeviceControl::new();
    crate::with_device_lease(&control, &serial, |leased| async move {
        leased.wake().await?;
        Ok(((), leased))
    })
    .await?;
    println!(
        "wake: {serial} screen is on \
         (a passcode lock is untouched — this lights the screen, it does not unlock)"
    );

    Ok(())
}

pub async fn stay_awake(device: &str, state: bool) -> Result<(), crate::CliError> {
    let serial = crate::resolve_android_serial(device)?;
    let control = smix_sdk::android_device::AndroidDeviceControl::new();
    crate::with_device_lease(&control, &serial, |leased| async move {
        leased.set_stay_awake(state).await?;
        Ok(((), leased))
    })
    .await?;
    let worded = if state { "on" } else { "off" };
    println!(
        "stay-awake: {serial} {worded} \
         (a device setting — it outlives this run until set back)"
    );

    Ok(())
}

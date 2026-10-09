//! The device's animation settings, for the length of a run.
//!
//! Quietened before the app comes up, so its first frame is drawn under
//! the settings the rest of the run uses, and put back once the flow is
//! over — after a failed flow and after the signal cascade too. Both
//! platforms are asked: how far each can be pushed differs, but neither
//! is exempt from being asked. A phone is: its animation scales are its
//! owner's, and a run that wrote them and then died would have changed
//! somebody's device for good.

use crate::entry::FlowPlatform;

/// Quieten `udid`'s animations unless `keep_device_settings`.
///
/// `Ok(true)` when the run now holds the change and owes [`put_back`],
/// `Ok(false)` when nothing was changed.
///
/// # Errors
///
/// When the device would not be quietened; the reason has been printed.
pub(crate) async fn quieten(
    app: &smix_sdk::App,
    platform: FlowPlatform,
    udid: Option<&str>,
    keep_device_settings: bool,
) -> Result<bool, ()> {
    let Some(id) = udid else {
        return Ok(false);
    };
    if keep_device_settings {
        return Ok(false);
    }
    let device_is_someones = match platform {
        FlowPlatform::Ios => false,
        FlowPlatform::Android => !smix_sdk::android_serial_is_emulator(id),
    };
    if device_is_someones {
        eprintln!(
            "animations: {id} is a physical device; its own animation settings \
             are left as they are and the flow runs with them"
        );
        return Ok(false);
    }
    if let Err(e) = app.set_animations_quiet(true).await {
        eprintln!(
            "error: could not quieten animations on {id}: {e}\n\
             hint: pass --animations to run with the device's own settings"
        );
        return Err(());
    }
    Ok(true)
}

/// Put the device's animation settings back.
///
/// Under the same kind of deadline as the session close, so a ctrl-C
/// still terminates when the device has gone; a device that is still
/// there answers three `settings` writes well inside it. A failure is a
/// warning: the flow's result is already decided, and the record of what
/// the settings were stays on the machine for the next run.
pub(crate) async fn put_back(app: &smix_sdk::App) {
    const NEXT_RUN: &str = "the next run on this device takes the recorded settings as the \
                            device's own and puts them back when it ends";
    let restore = app.set_animations_quiet(false);
    match tokio::time::timeout(std::time::Duration::from_secs(10), restore).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            eprintln!("WARN: the device's animation settings were not put back: {e}\n{NEXT_RUN}");
        }
        Err(_) => eprintln!(
            "WARN: putting the device's animation settings back timed out after 10s\n{NEXT_RUN}"
        ),
    }
}

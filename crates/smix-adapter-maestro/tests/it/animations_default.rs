//! A run quietens the device unless told not to.
//!
//! The default is the point. `waitForAnimationToEnd` appears seventeen
//! times across the guides, and every one is compensation for a screen
//! moving when nobody wanted it to. Making the still screen the paved
//! road, rather than an opt-in, is what removes the compensation.
//!
//! # WHAT THIS CANNOT SEE
//!
//! Whether the device obeyed. That is `DeviceControl::set_animations_
//! quiet`'s read-back, judged by `animation_settings_verified` and
//! tested against captured device output in smix-sdk. This checks the
//! step happens and which way the flag points it — a source-level
//! reading, because the alternative is thirteen stub methods per mock
//! to observe one call.

const ENTRY: &str = include_str!("../../src/entry.rs");
const ANIMATIONS: &str = include_str!("../../src/animations.rs");

#[test]
fn the_run_asks_for_quiet_when_the_flag_is_absent() {
    assert!(
        ENTRY.contains("args.animations,"),
        "the run no longer hands the flag to the animation step"
    );
    assert!(
        ANIMATIONS.contains("if keep_device_settings {\n        return Ok(false);"),
        "the flag no longer keeps the device's own settings"
    );
    assert!(
        ANIMATIONS.contains("set_animations_quiet(true)"),
        "the run no longer asks for quiet"
    );
}

/// Before the app is foregrounded, so its first frame is already drawn
/// under the settings the rest of the run uses.
#[test]
fn it_happens_before_the_app_comes_up() {
    let prepare = ENTRY
        .find("crate::animations::quieten(")
        .expect("the run still prepares animations");
    let foreground = ENTRY
        .find(".foreground(&bundle_id)")
        .expect("the run still foregrounds the app");
    assert!(
        prepare < foreground,
        "animations are quietened after the app is already on screen, \
         so its first frame is drawn under the old settings"
    );
}

/// Both platforms are asked. How far each can be pushed differs; being
/// asked does not. A device is exempt by being somebody's phone, which
/// is decided by serial, and only an Android serial can name one.
#[test]
fn neither_platform_is_exempt() {
    let ios = ANIMATIONS
        .find("FlowPlatform::Ios => false")
        .expect("an iOS device is never taken for somebody's phone");
    let android = ANIMATIONS
        .find("FlowPlatform::Android => !smix_sdk::android_serial_is_emulator(id)")
        .expect("an Android device is somebody's phone by its serial alone");
    let asked = ANIMATIONS
        .find("set_animations_quiet(true)")
        .expect("the run still asks for quiet");
    assert!(ios < asked && android < asked);
}

/// What was quietened is put back when the run ends, after the flow
/// has finished or been abandoned, and before the holder is consumed.
#[test]
fn the_run_puts_the_settings_back_after_the_flow() {
    let prepare = ENTRY
        .find("crate::animations::quieten(")
        .expect("the run still prepares animations");
    let restore = ENTRY
        .find("crate::animations::put_back(holder.app())")
        .expect("the run no longer puts the device's animation settings back");
    let flow_done = ENTRY
        .find("drop(adapter);")
        .expect("the run still drops the adapter once the flow is over");
    let holder_consumed = ENTRY
        .find("if let AppHolder::Session(session) = holder")
        .expect("the run still closes the session at the end");
    assert!(prepare < restore, "the restore comes before the change");
    assert!(
        flow_done < restore && restore < holder_consumed,
        "the restore is not between the end of the flow and the session close, \
         so an interrupted run would skip it or have nothing to call it on"
    );
    assert!(
        ENTRY.contains("if holds_animations {"),
        "the restore is no longer keyed to the change having happened"
    );
    assert!(
        ANIMATIONS.contains("set_animations_quiet(false)"),
        "putting back no longer asks the device to restore"
    );
}

/// A phone's animation scales are its owner's. A run leaves them alone
/// and says so once.
#[test]
fn a_physical_android_device_is_left_as_it_is() {
    let someones = ANIMATIONS
        .find("if device_is_someones {")
        .expect("the run no longer tells a phone apart");
    let asked = ANIMATIONS
        .find("set_animations_quiet(true)")
        .expect("the run still asks for quiet");
    assert!(someones < asked, "a phone is asked before it is left alone");
    assert!(
        ANIMATIONS.contains("is a physical device; its own animation settings"),
        "the run no longer says it left a phone's settings alone"
    );
}

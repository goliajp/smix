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

#[test]
fn the_run_asks_for_quiet_when_the_flag_is_absent() {
    assert!(
        ENTRY.contains("if !args.animations"),
        "the run no longer keys device preparation off the flag"
    );
    assert!(
        ENTRY.contains("set_animations_quiet(true)"),
        "the run no longer asks for quiet"
    );
}

/// Before the app is foregrounded, so its first frame is already drawn
/// under the settings the rest of the run uses.
#[test]
fn it_happens_before_the_app_comes_up() {
    let prepare = ENTRY
        .find("set_animations_quiet(true)")
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
/// is decided above the block and by serial, not by platform.
#[test]
fn neither_platform_is_exempt() {
    let block_start = ENTRY
        .find("if !args.animations")
        .expect("the preparation block is still there");
    let block = &ENTRY[block_start..block_start + 400];
    assert!(
        !block.contains("FlowPlatform::Ios") && !block.contains("FlowPlatform::Android"),
        "the animation step gained a platform condition — the foreground \
         call below it has one for its own reason, and this must not \
         inherit it by proximity"
    );
}

/// What was quietened is put back when the run ends, after the flow
/// has finished or been abandoned, and before the holder is consumed.
#[test]
fn the_run_puts_the_settings_back_after_the_flow() {
    let prepare = ENTRY
        .find("set_animations_quiet(true)")
        .expect("the run still prepares animations");
    let restore = ENTRY
        .find("set_animations_quiet(false)")
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
        ENTRY.contains("holds_animations = true"),
        "the restore is no longer keyed to the change having happened"
    );
}

/// A phone's animation scales are its owner's. A run leaves them alone
/// and says so once.
#[test]
fn a_physical_android_device_is_left_as_it_is() {
    assert!(
        ENTRY.contains("!smix_sdk::android_serial_is_emulator(serial)"),
        "the run no longer tells a phone from an emulator by its serial"
    );
    assert!(
        ENTRY.contains("is a physical device; its own animation settings"),
        "the run no longer says it left a phone's settings alone"
    );
    let block_start = ENTRY
        .find("if !args.animations")
        .expect("the preparation block is still there");
    let block = &ENTRY[block_start..block_start + 200];
    assert!(
        block.contains("&& !device_is_someones"),
        "the change is no longer withheld from a physical device"
    );
}

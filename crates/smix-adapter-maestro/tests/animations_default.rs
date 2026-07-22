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

const ENTRY: &str = include_str!("../src/entry.rs");

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
/// asked does not.
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

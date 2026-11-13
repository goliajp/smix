use super::*;

fn denied(text: &str) -> bool {
    matches!(judge(text), Verdict::Deny(_))
}

#[test]
fn every_lifecycle_step_outside_smix_is_refused() {
    let refused = [
        // the command that started this
        "emulator -avd qip-insight-01 -port 5560 -no-snapshot-save -no-audio -no-boot-anim",
        "$ANDROID_HOME/emulator/emulator @sim-android-01 -no-window",
        "\"$SDK/emulator/emulator\" -avd tab &",
        "/opt/sdk/emulator/qemu/darwin-aarch64/qemu-system-aarch64 -avd phone",
        "adb -s emulator-5554 emu kill",
        "avdmanager create avd -n x -k 'system-images;android-35;google_apis;arm64-v8a'",
        "avdmanager delete avd -n x",
        "xcrun simctl boot 0A1B2C3D-0000-4000-8000-00000000000A",
        "xcrun simctl shutdown 0A1B2C3D-0000-4000-8000-00000000000A",
        "xcrun simctl erase 0A1B2C3D-0000-4000-8000-00000000000A",
        "xcrun simctl create t 'iPhone 17' com.apple.CoreSimulator.SimRuntime.iOS-26-5",
        "xcrun simctl delete unavailable",
        "xcrun simctl clone 0A1B2C3D-0000-4000-8000-00000000000A copy",
        "xcrun simctl --set /tmp/devs boot 0A1B2C3D-0000-4000-8000-00000000000A",
        "simctl boot 0A1B2C3D-0000-4000-8000-00000000000A",
        // behind wrappers, assignments and chains
        "cd /x && nohup emulator -avd a > /dev/null 2>&1 &",
        "ANDROID_SERIAL=emulator-5554 nice -n 19 emulator -avd a",
        "sudo xcrun simctl shutdown 0A1B2C3D-0000-4000-8000-00000000000A",
        "timeout 30 xcrun simctl boot 0A1B2C3D-0000-4000-8000-00000000000A",
        "true; xcrun simctl boot X",
        "echo start\nxcrun simctl boot X",
        "bash -c 'xcrun simctl boot X'",
        "sh -c \"emulator -avd a\"",
        "eval xcrun simctl boot X",
        "U=$(xcrun simctl boot X)",
    ];
    for text in refused {
        assert!(denied(text), "should be refused: {text}");
    }
}

#[test]
fn reading_and_naming_are_not_refused() {
    let allowed = [
        "xcrun simctl list devices",
        "xcrun simctl bootstatus 0A1B2C3D-0000-4000-8000-00000000000A -b",
        "xcrun simctl io 0A1B2C3D-0000-4000-8000-00000000000A screenshot a.png",
        "adb devices",
        "adb -s emulator-5554 shell getprop sys.boot_completed",
        "emulator -list-avds",
        "emulator -version",
        "avdmanager list avd",
        "grep -rn 'simctl boot' crates/",
        "rg \"emulator -avd\" scripts",
        "echo \"xcrun simctl boot X\"",
        "git commit -m 'refuse emulator -avd and simctl boot'",
        "# xcrun simctl boot X",
        "smix sim boot qip-insight-01 --unlock-pin 1234",
        "smix sim exec 0A1B2C3D-0000-4000-8000-00000000000A boot",
        "cat notes.md | grep 'adb emu kill'",
    ];
    for text in allowed {
        assert_eq!(judge(text), Verdict::Allow, "should pass: {text}");
    }
}

#[test]
fn a_named_exception_lets_one_command_through_and_is_reported() {
    let text = "SMIX_FENCE_EXCEPTION=\"recovering a wedged AVD\" emulator -avd a -wipe-data";
    match judge(text) {
        Verdict::Excepted(rows) => {
            assert_eq!(rows.len(), 1);
            assert_eq!(rows[0].0, "recovering a wedged AVD");
            assert!(rows[0].1.contains("emulator -avd a"), "{rows:?}");
        }
        other => panic!("expected an exception, got {other:?}"),
    }
}

#[test]
fn an_exception_covers_its_own_command_only() {
    // the prefix applies to the command it stands in front of, the way a
    // shell assignment does; the next command is judged on its own
    let text = "SMIX_FENCE_EXCEPTION=why emulator -avd a; xcrun simctl boot X";
    assert!(denied(text));
    let exported = "export SMIX_FENCE_EXCEPTION=why\nemulator -avd a";
    assert!(denied(exported), "an export is not a per-command exception");
}

#[test]
fn an_exception_without_a_reason_is_refused() {
    match judge("SMIX_FENCE_EXCEPTION= emulator -avd a") {
        Verdict::Deny(why) => assert!(why.contains("needs a reason"), "{why}"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn a_refusal_names_the_smix_command_to_use() {
    let Verdict::Deny(why) = judge("emulator -avd a") else {
        panic!("refused");
    };
    assert!(why.contains("smix sim boot"), "{why}");
    assert!(why.contains("SMIX_FENCE_EXCEPTION"), "{why}");
    let Verdict::Deny(why) = judge("xcrun simctl shutdown X") else {
        panic!("refused");
    };
    assert!(why.contains("smix sim shutdown"), "{why}");
}

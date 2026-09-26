//! `stop_emulator` returns once the emulator has quit, not once it was asked.
//!
//! It returned as soon as `emu kill` was accepted, and its callers then
//! dropped the ledger's boot row and moved on while the emulator was still
//! saving its snapshot and quitting. Anything that followed — the next
//! boot on that console port, a teardown ending the caller's process
//! group — met an emulator halfway out (2026-09-25).
//!
//! The adb here is a stand-in that keeps listing the serial for a number
//! of `devices` calls after `emu kill`, so both outcomes are exercised
//! without an emulator.

use std::path::Path;
use std::time::Duration;

const SERIAL: &str = "emulator-5980";

/// An adb that lists SERIAL until it has been asked `devices` `lingers`
/// times after `emu kill`.
fn stand_in_adb(dir: &Path, lingers: u32) -> String {
    let state = dir.join("state");
    let script = format!(
        "#!/bin/sh\n\
         S='{state}'\n\
         case \"$*\" in\n\
         *'emu kill'*) echo 0 > \"$S\"; echo 'OK: killing emulator, bye bye'; exit 0;;\n\
         devices*)\n\
           echo 'List of devices attached'\n\
           if [ ! -f \"$S\" ]; then echo '{SERIAL}\tdevice'; exit 0; fi\n\
           n=$(cat \"$S\"); echo $((n+1)) > \"$S\"\n\
           [ \"$n\" -lt {lingers} ] && echo '{SERIAL}\toffline'\n\
           exit 0;;\n\
         esac\n\
         exit 0\n",
        state = state.display()
    );
    let path = dir.join("adb");
    std::fs::write(&path, script).expect("the temp dir is writable");
    let mut perm = std::fs::metadata(&path)
        .expect("just written")
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    std::fs::set_permissions(&path, perm).expect("the temp dir is writable");
    path.display().to_string()
}

fn asked(dir: &Path) -> u32 {
    std::fs::read_to_string(dir.join("state"))
        .expect("emu kill was sent")
        .trim()
        .parse()
        .expect("the stand-in writes a count")
}

#[tokio::test]
async fn stop_waits_until_adb_no_longer_lists_it() {
    let dir = tempfile::tempdir().expect("a temp dir is available in tests");
    let adb = smix_adb::AdbClient::with_binary(stand_in_adb(dir.path(), 3));
    adb.stop_emulator_within(SERIAL, Duration::from_secs(10))
        .await
        .expect("the stand-in stops listing it after three looks");
    assert!(
        asked(dir.path()) > 3,
        "returned while the serial was still listed ({} looks)",
        asked(dir.path())
    );
}

#[tokio::test]
async fn an_emulator_that_does_not_quit_is_said_so_and_left_alone() {
    let dir = tempfile::tempdir().expect("a temp dir is available in tests");
    let adb = smix_adb::AdbClient::with_binary(stand_in_adb(dir.path(), u32::MAX));
    let err = adb
        .stop_emulator_within(SERIAL, Duration::from_millis(1500))
        .await
        .expect_err("an emulator that stays listed has not stopped");
    let text = err.to_string();
    assert!(text.contains(SERIAL), "names the emulator: {text}");
    assert!(text.contains("still listed"), "says what it saw: {text}");
    assert!(
        !text.contains("device"),
        "the CLI reads \"device\" in this error as already gone: {text}"
    );
}

//! `smix sim list` names an attached, unregistered phone and sends it
//! nothing.
//!
//! It used to run `adb -s <serial> shell getprop ro.build.version.release`
//! on every device adb listed, so anyone listing devices opened a shell on
//! a phone nobody had offered to smix (2026-09-25, SL2). An unregistered
//! device is unreachable (§9#1), a read included.
//!
//! The adb here is a stand-in on PATH that records what it was asked and
//! invents a phone: no real device is involved, which is also the only way
//! this can be proven without doing the thing it forbids.

use std::path::Path;
use std::process::Command;

const PHONE: &str = "ZZ00FAKE0001";

fn stand_in_adb(dir: &Path, log: &Path) {
    let script = format!(
        "#!/bin/sh\n\
         echo \"$*\" >> '{log}'\n\
         if [ \"$1\" = devices ]; then\n\
         echo 'List of devices attached'\n\
         echo 'emulator-5554          device product:sdk model:sdk_phone device:emu64a transport_id:1'\n\
         echo '{PHONE}           device usb:1-1 product:x model:Stand_In device:x transport_id:2'\n\
         exit 0\n\
         fi\n\
         echo 14\n",
        log = log.display()
    );
    let path = dir.join("adb");
    std::fs::write(&path, script).expect("the temp dir is writable");
    let mut perm = std::fs::metadata(&path)
        .expect("just written")
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    std::fs::set_permissions(&path, perm).expect("the temp dir is writable");
}

/// An xcrun that knows no simulators, so the listing does not depend on
/// the host having Xcode (it does not on Linux CI) or on what it has.
fn stand_in_xcrun(dir: &Path) {
    let path = dir.join("xcrun");
    std::fs::write(&path, "#!/bin/sh\necho '{\"devices\":{}}'\n")
        .expect("the temp dir is writable");
    let mut perm = std::fs::metadata(&path)
        .expect("just written")
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    std::fs::set_permissions(&path, perm).expect("the temp dir is writable");
}

#[test]
fn an_unregistered_phone_is_listed_and_not_asked() {
    let bin = tempfile::tempdir().expect("a temp dir is available in tests");
    let machine = tempfile::tempdir().expect("a temp dir is available in tests");
    let work = tempfile::tempdir().expect("a temp dir is available in tests");
    let log = bin.path().join("adb.log");
    stand_in_adb(bin.path(), &log);
    stand_in_xcrun(bin.path());
    let path = format!(
        "{}:{}",
        bin.path().display(),
        std::env::var("PATH").expect("PATH is set")
    );

    let out = Command::new(env!("CARGO_BIN_EXE_smix"))
        .args(["sim", "list", "--json"])
        .env("PATH", path)
        .env("SMIX_MACHINE_DIR", machine.path())
        .current_dir(work.path())
        .output()
        .expect("the smix binary this test was built with runs");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let asked = std::fs::read_to_string(&log).expect("smix asked the stand-in adb something");

    // Present first: the stand-in was consulted and the listing carries
    // the phone. Without these the absence below proves nothing.
    assert!(asked.lines().any(|l| l.starts_with("devices")), "{asked}");
    assert!(
        asked.lines().any(|l| l.contains("-s emulator-5554 shell")),
        "an emulator is still asked for its release: {asked}"
    );
    assert!(stdout.contains(PHONE), "the phone is listed: {stdout}");

    assert!(
        !asked.lines().any(|l| l.contains(PHONE)),
        "nothing was sent to the unregistered phone, and it was: {asked}"
    );
}

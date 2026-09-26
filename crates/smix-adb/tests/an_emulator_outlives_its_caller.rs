//! An emulator smix starts is not in the caller's process group.
//!
//! It used to be spawned as a plain child, so it shared the group of
//! whatever ran `smix sim boot` — a terminal, a test tier, a release
//! script, a tool harness. A signal to that group (Ctrl-C, a deadline
//! tearing a script down, a harness ending a command) reached the
//! emulator's launcher, which passed it on; the headless qemu aborted in
//! its signal-time quit path and macOS showed the owner an "Android
//! Emulator quit unexpectedly" dialog (2026-09-25, `skin_winsys_quit_request`
//! called from `_sigtramp`).
//!
//! The stand-in `emulator` below reports its own process group, so this is
//! proven without starting an emulator — and without a crash to prove it.

use std::path::Path;
use std::time::{Duration, Instant};

fn stand_in_emulator(sdk: &Path, report: &Path) {
    let dir = sdk.join("emulator");
    std::fs::create_dir_all(&dir).expect("the temp dir is writable");
    let script = format!(
        "#!/bin/sh\nps -o pgid= -p $$ | tr -d ' ' > '{}'\n",
        report.display()
    );
    let path = dir.join("emulator");
    std::fs::write(&path, script).expect("the temp dir is writable");
    let mut perm = std::fs::metadata(&path)
        .expect("just written")
        .permissions();
    std::os::unix::fs::PermissionsExt::set_mode(&mut perm, 0o755);
    std::fs::set_permissions(&path, perm).expect("the temp dir is writable");
}

fn our_group() -> String {
    let out = std::process::Command::new("ps")
        .args(["-o", "pgid=", "-p", &std::process::id().to_string()])
        .output()
        .expect("ps is on every Unix this runs on");
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn a_started_emulator_has_a_process_group_of_its_own() {
    let sdk = tempfile::tempdir().expect("a temp dir is available in tests");
    let report = sdk.path().join("pgid");
    stand_in_emulator(sdk.path(), &report);

    // SAFETY: this test is the only code in its process that reads
    // ANDROID_HOME, and it sets it before spawning anything.
    unsafe { std::env::set_var("ANDROID_HOME", sdk.path()) };
    smix_adb::AdbClient::new()
        .start_emulator("stand-in")
        .expect("the stand-in launcher starts");

    let deadline = Instant::now() + Duration::from_secs(5);
    let theirs = loop {
        if let Ok(s) = std::fs::read_to_string(&report)
            && !s.trim().is_empty()
        {
            break s.trim().to_string();
        }
        assert!(
            Instant::now() < deadline,
            "the stand-in never reported its group"
        );
        std::thread::sleep(Duration::from_millis(20));
    };
    let ours = our_group();
    assert!(!ours.is_empty(), "ps answered with this process's group");
    assert_ne!(
        theirs, ours,
        "the emulator shares the caller's process group, so a signal to the caller's group reaches it"
    );
}

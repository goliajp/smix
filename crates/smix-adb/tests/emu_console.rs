//! The emulator console is not the device shell.
//!
//! `setLocation` shipped calling `adb shell emu geo fix …`, which asks the
//! device for a program named `emu` — `sh: emu: inaccessible or not found`,
//! exit 127. Loud, but the verb never once worked.
//!
//! The console is the part that needs care. `adb shell` propagates the device
//! command's exit status, but `adb emu` does not: it answers `OK` or
//! `KO: <reason>` and exits 0 regardless. So these pin both halves — which
//! argv is sent, and that a `KO` is not mistaken for success.

use std::os::unix::fs::PermissionsExt;

use smix_adb::AdbClient;

/// A stub `adb` that records its argv and replies with `body`.
fn stub_adb(dir: &std::path::Path, body: &str) -> AdbClient {
    let bin = dir.join("adb-stub");
    std::fs::write(
        &bin,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$@\" > {}/argv.txt\n{body}\n",
            dir.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    AdbClient::with_binary(bin.to_string_lossy().into_owned())
}

fn argv(dir: &std::path::Path) -> Vec<String> {
    std::fs::read_to_string(dir.join("argv.txt"))
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

#[tokio::test]
async fn emu_goes_to_the_console_not_the_device_shell() {
    let dir = tempfile::tempdir().unwrap();
    let c = stub_adb(dir.path(), "echo OK");
    c.emu("emulator-5554", &["geo", "fix", "139.6917", "35.6895"])
        .await
        .unwrap();

    let got = argv(dir.path());
    assert_eq!(
        got,
        vec!["-s", "emulator-5554", "emu", "geo", "fix", "139.6917", "35.6895"],
        "the console command must not be wrapped in `shell`"
    );
    assert!(
        !got.contains(&"shell".to_string()),
        "`adb shell emu …` looks for a program the device does not have; got {got:?}"
    );
}

#[tokio::test]
async fn a_console_refusal_is_not_success() {
    // Measured against a real emulator: the console answers `KO: <reason>`
    // and exits 0. Reading stdout is the only way a caller ever finds out.
    let dir = tempfile::tempdir().unwrap();
    let c = stub_adb(dir.path(), "echo \"KO: argument 'notanumber' is not a number\"");
    let err = c
        .emu("emulator-5554", &["geo", "fix", "notanumber"])
        .await
        .expect_err("KO must surface");
    assert!(
        err.to_string().contains("not a number"),
        "the console's own reason is the useful part; got: {err}"
    );
}

#[tokio::test]
async fn a_console_ok_is_success() {
    let dir = tempfile::tempdir().unwrap();
    let c = stub_adb(dir.path(), "echo OK");
    let out = c.emu("emulator-5554", &["geo", "fix", "1", "2"]).await.unwrap();
    assert!(out.contains("OK"), "got: {out}");
}

#[tokio::test]
async fn shell_still_goes_to_the_device() {
    // The sibling path must keep working — this is what `pm grant` rides on.
    let dir = tempfile::tempdir().unwrap();
    let c = stub_adb(dir.path(), "echo done");
    c.shell("emulator-5554", &["pm", "list", "packages"])
        .await
        .unwrap();
    let got = argv(dir.path());
    assert_eq!(
        got,
        vec!["-s", "emulator-5554", "shell", "pm", "list", "packages"]
    );
}

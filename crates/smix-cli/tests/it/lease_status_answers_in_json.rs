//! `smix lease status <device> --json` prints the device's ledger as it is.
//!
//! Four device e2e scripts read a ledger by building its path: three from
//! the checkout's `.smix/leases/`, which stopped being written when device
//! facts moved to the machine (4.0) and still held a file from August, and
//! one from `$HOME/.local/share/smix`, which is not where the ledger is
//! when `SMIX_MACHINE_DIR` or `XDG_DATA_HOME` says otherwise. The first
//! three read a stale row and failed "no recording row" for a recording
//! that was running (2026-09-25). Where a ledger lives is smix's to know;
//! a script asks.

use std::process::Command;

fn smix() -> &'static str {
    env!("CARGO_BIN_EXE_smix")
}

// Tagged per test: the two run on parallel threads of one process, and a
// directory named by pid and time alone was shared between them.
fn machine(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "smix-lease-json-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("leases")).unwrap();
    dir
}

// A serial with an emulator's shape and nothing behind it: addressable
// without registration, and no device is asked anything.
const DEVICE: &str = "emulator-5998";

#[test]
fn the_ledger_comes_back_as_written() {
    let m = machine("written");
    let ledger = serde_json::json!({
        "deviceId": DEVICE,
        "holder": { "pid": 1, "startedAt": "Thu Jan  1 00:00:00 1970", "cmd": "smix record start" },
        "acquiredAt": "2026-09-25T00:00:00+00:00",
        "heartbeatAt": "2026-09-25T00:00:00+00:00",
        "resources": [{
            "kind": "recording",
            "path": "/tmp/first.mov",
            "proc": { "pid": 2, "startedAt": "Thu Jan  1 00:00:00 1970", "cmd": "simctl io recordVideo" },
        }],
    });
    std::fs::write(
        m.join("leases").join(format!("{DEVICE}.json")),
        serde_json::to_vec_pretty(&ledger).unwrap(),
    )
    .unwrap();

    let out = Command::new(smix())
        .args(["lease", "status", DEVICE, "--json"])
        .env("SMIX_MACHINE_DIR", &m)
        .output()
        .expect("run smix");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    std::fs::remove_dir_all(&m).ok();

    assert!(out.status.success(), "exit {:?}: {stderr}", out.status);
    let got: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert_eq!(got["device"], DEVICE);
    // Where it lives is part of the answer: two of the scripts set a
    // holder's row up by hand, and they write where smix says.
    assert_eq!(
        got["path"].as_str().map(std::path::PathBuf::from),
        Some(m.join("leases").join(format!("{DEVICE}.json")))
    );
    assert_eq!(got["lease"]["resources"][0]["kind"], "recording");
    assert_eq!(got["lease"]["resources"][0]["path"], "/tmp/first.mov");
}

#[test]
fn a_device_with_no_ledger_answers_null_rather_than_nothing() {
    let m = machine("none");
    let out = Command::new(smix())
        .args(["lease", "status", DEVICE, "--json"])
        .env("SMIX_MACHINE_DIR", &m)
        .output()
        .expect("run smix");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    std::fs::remove_dir_all(&m).ok();
    assert!(out.status.success());
    let got: serde_json::Value =
        serde_json::from_str(&stdout).unwrap_or_else(|e| panic!("not JSON ({e}): {stdout}"));
    assert_eq!(got["device"], DEVICE);
    assert!(got["lease"].is_null(), "{stdout}");
}

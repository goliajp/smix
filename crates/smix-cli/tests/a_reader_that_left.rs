//! A reader that stops reading is not a crash.
//!
//! `smix sim list | head -1`, or an `awk '…{exit}'` in a harness, closes
//! the pipe while smix still has lines to print. `println!` panics on
//! that, so smix used to die with exit 101 and a Rust panic message —
//! and under `pipefail` a script read that as smix failing at the one
//! thing it had asked for (2026-09-25, SP1: five of five with
//! `sim list | (sleep 0.2; true)`).
//!
//! The command's work is the verdict, not whether anyone read it to the
//! end: smix stops writing to stdout and finishes, and its exit code
//! says how the work went.

use std::io::{Read, Write};
use std::process::{Command, Stdio};

/// Far more than a pipe holds (64 KiB on macOS and Linux), so the write
/// cannot land in the buffer before the reader leaves.
fn a_long_flow() -> String {
    let mut s = String::from("appId: com.example.app\n---\n");
    for i in 0..20_000 {
        s.push_str(&format!("- tapOn: \"row {i}\"\n"));
    }
    s
}

#[test]
fn a_closed_stdout_leaves_the_exit_code_to_the_work() {
    let machine = tempfile::tempdir().expect("a temp dir is available in tests");
    let mut child = Command::new(env!("CARGO_BIN_EXE_smix"))
        .arg("migrate")
        .env("SMIX_MACHINE_DIR", machine.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the smix binary this test was built with runs");

    let flow = a_long_flow();
    let mut stdin = child.stdin.take().expect("stdin was piped");
    let writer = std::thread::spawn(move || {
        stdin
            .write_all(flow.as_bytes())
            .expect("smix reads all of stdin before it writes");
    });

    let mut stdout = child.stdout.take().expect("stdout was piped");
    let mut first = [0u8; 10];
    stdout
        .read_exact(&mut first)
        .expect("smix printed at least ten bytes before the reader left");
    drop(stdout);

    writer.join().expect("the stdin writer finished");
    let out = child.wait_with_output().expect("smix exits");
    let stderr = String::from_utf8_lossy(&out.stderr);

    assert!(
        !stderr.contains("panicked"),
        "smix panicked when its reader left:\n{stderr}"
    );
    assert_eq!(
        out.status.code(),
        Some(0),
        "the migration succeeded, so the exit code says so whoever stopped reading; stderr:\n{stderr}"
    );
}

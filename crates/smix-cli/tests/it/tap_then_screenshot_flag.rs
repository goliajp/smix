//! `--then-screenshot` is on the command, and a tap that fails leaves
//! nothing behind.
//!
//! The second half is the one worth a test. A frame written after a tap
//! that did not land is the most misleading artifact in this kind of
//! debugging: it is a picture of the screen nothing happened on, and it
//! looks exactly like evidence. Nothing about the file says which it is,
//! so the only place that can be decided is here.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::process::Command;

/// A server that is up and refuses to describe the screen — the shape of
/// a runner whose session is gone, which is what a failed tap looks like
/// from this side.
fn healthy_but_blind() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    std::thread::spawn(move || {
        for conn in listener.incoming() {
            let Ok(mut sock) = conn else { return };
            let mut scratch = [0u8; 2048];
            let n = sock.read(&mut scratch).unwrap_or(0);
            let line = String::from_utf8_lossy(&scratch[..n])
                .lines()
                .next()
                .unwrap_or("")
                .to_string();
            let (status, body) = if line.contains("/health") {
                (200, r#"{"ok":true,"runnerVersion":"4.3.0"}"#.to_string())
            } else {
                (
                    500,
                    r#"{"ok":false,"error":"snapshot_unavailable","reason":"not-running"}"#
                        .to_string(),
                )
            };
            let _ = write!(
                sock,
                "HTTP/1.1 {status} X\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
        }
    });
    port
}

#[test]
fn the_flag_is_on_the_command() {
    let out = Command::new(env!("CARGO_BIN_EXE_smix"))
        .args(["tap", "--help"])
        .output()
        .expect("run smix tap --help");
    assert!(out.status.success(), "`tap --help` exited non-zero");
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("<SELECTOR>") || help.contains("selector"),
        "the help does not look like `tap`'s — this test is reading air"
    );
    assert!(
        help.contains("--then-screenshot"),
        "there is no way to take the frame in the same call: {help}"
    );
}

#[test]
fn a_tap_that_fails_writes_no_frame() {
    let port = healthy_but_blind();
    let dir = tempfile::tempdir().expect("tempdir");
    let out_png = dir.path().join("evidence.png");
    let out = Command::new(env!("CARGO_BIN_EXE_smix"))
        .args([
            "tap",
            "id:nothing-here",
            "--port",
            &port.to_string(),
            "--then-screenshot",
            out_png.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("run smix tap --then-screenshot");
    assert!(
        !out.status.success(),
        "the tap could not have landed — the runner refuses to describe \
         the screen — and the command reported success"
    );
    assert!(
        !out_png.exists(),
        "a file was written for a tap that did not land: {}",
        out_png.display()
    );
}

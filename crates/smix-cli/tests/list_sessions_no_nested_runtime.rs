//! `smix runner list-sessions` must not build a Tokio runtime.
//!
//! `run()` is already `#[tokio::main]`, so a handler that calls
//! `Runtime::new().block_on(...)` panics on every invocation with
//! "Cannot start a runtime from within a runtime" — before it ever
//! reaches the network. This one did, so the command panicked 100% of
//! the time, and the release smoke gate that runs it had been red
//! against a real simulator with nobody reading why.
//!
//! The sibling handlers (`Cycle`, `Supervise`) call synchronous
//! functions and were fine; only this branch span up its own runtime.
//!
//! The probe points at a port nothing serves. A working handler fails
//! to connect and exits non-zero with a transport error; the broken
//! one panics building the runtime, before the connection is even
//! attempted. The two are told apart by the panic's own words, so this
//! never needs a runner or a device.

use std::process::Command;

#[test]
fn list_sessions_does_not_panic_building_a_runtime() {
    let exe = env!("CARGO_BIN_EXE_smix");
    let out = Command::new(exe)
        .args(["runner", "list-sessions"])
        // A port no runner is on: the handler should get as far as a
        // failed connection, which is the point past the panic.
        .env("SMIX_RUNNER_PORT", "1")
        .output()
        .expect("run smix");

    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        !stderr.contains("Cannot start a runtime from within a runtime"),
        "list-sessions built a nested Tokio runtime and panicked:\n{stderr}"
    );
    // 101 is the Rust panic exit code. A clean transport failure is a
    // different, non-panic exit.
    assert_ne!(
        out.status.code(),
        Some(101),
        "list-sessions panicked (exit 101):\n{stderr}"
    );
}

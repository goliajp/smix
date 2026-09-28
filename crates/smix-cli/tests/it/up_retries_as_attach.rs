//! The attach fallback is not a flag.
//!
//! A retry spelled `--fallback-attach` turns a default into something
//! every user has to know about, and defaults that have to be typed stop
//! being defaults.
//!
//! The other half of this — that the retry does not reach past the
//! ownership judgement — is asserted in smix-capsule's
//! `up_asks_the_session`, against `up_on` directly. A first draft put it
//! here and it went red for the wrong reason: a temp directory is not a
//! smix workspace, so the command stopped before reaching the branch the
//! test named. A stop is not the same as the stop you asked about.

use std::process::Command;

#[test]
fn the_retry_is_a_default_rather_than_a_flag() {
    let out = Command::new(env!("CARGO_BIN_EXE_smix"))
        .args(["runner", "up", "--help"])
        .output()
        .expect("run smix runner up --help");
    assert!(out.status.success(), "`runner up --help` exited non-zero");
    let help = String::from_utf8_lossy(&out.stdout);
    assert!(
        help.contains("--bundle"),
        "the help does not look like `runner up`'s — this test is reading air"
    );
    for invented in ["--fallback", "--attach", "--retry-as-attach"] {
        assert!(
            !help.contains(invented),
            "{invented} turns the fallback into something to remember; it is \
             what the command does when it runs out of time"
        );
    }
}

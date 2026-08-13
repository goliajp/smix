//! `--force` on `runner up` is a recovery, and its help has to say which
//! kind.
//!
//! A flag called force, on a command that can end processes, reads as
//! "kill whatever is in the way" unless it says otherwise — and this one
//! does the opposite: it cycles the runner this workspace recorded, in
//! place, and refuses somebody else's exactly as the unforced command
//! does. The wording is the only place a reader learns that.

use std::process::Command;

fn up_help() -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_smix"))
        .args(["runner", "up", "--help"])
        .output()
        .expect("run smix runner up --help");
    assert!(out.status.success(), "`runner up --help` exited non-zero");
    String::from_utf8_lossy(&out.stdout).to_string()
}

#[test]
fn the_flag_is_on_the_command() {
    let help = up_help();
    assert!(
        help.contains("--bundle"),
        "the help does not look like `runner up`'s — this test is reading air"
    );
    assert!(
        help.contains("--force"),
        "there is no way to recover a wedged runner from this command: {help}"
    );
}

#[test]
fn the_help_says_cycle_and_does_not_say_kill() {
    let help = up_help();
    let force = help
        .split("--force")
        .nth(1)
        .expect("--force is on the command")
        .split("\n  -")
        .next()
        .expect("the flag's own paragraph");
    assert!(
        force.contains("cycle"),
        "the help has to name what --force does: {force}"
    );
    assert!(
        !force.contains("kill"),
        "--force does not kill anything, and help that says it does will be \
         believed: {force}"
    );
}

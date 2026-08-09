//! `smix wait-for --absent` — waiting for something to GO is reachable
//! from the CLI.
//!
//! `smix_assert_not_visible` is an MCP tool. The CLI could wait for an
//! element to appear and had no way to wait for one to leave, so the
//! plugin held a capability smix on its own did not — the one thing the
//! v2.13 cold plan says a plugin must never do.
//!
//! `smix find` is not the missing piece: it prints `exists=<bool>` and
//! exits 0 either way, so a shell cannot branch on it without parsing
//! stdout, and it answers about now rather than waiting. An assertion
//! has to be able to fail.

use std::process::Command;

fn smix() -> Command {
    Command::new(env!("CARGO_BIN_EXE_smix"))
}

fn wait_for_help() -> String {
    let out = smix()
        .args(["wait-for", "--help"])
        .output()
        .expect("run smix");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn absent_is_a_flag() {
    let help = wait_for_help();
    assert!(
        help.contains("--absent"),
        "`smix wait-for` has no `--absent`. MCP has smix_assert_not_visible \
         and the CLI has no way to wait for an element to go.\n{help}"
    );
}

/// The flag's help says which way round it is.
///
/// `wait-for --absent` reads either way to someone skimming — wait for
/// the thing that is absent, or wait until it is absent. Only the second
/// is what it does.
#[test]
fn absent_help_says_it_waits_until_the_element_is_gone() {
    let help = wait_for_help();
    assert!(
        help.contains("gone") || help.contains("until it is absent") || help.contains("disappear"),
        "`--absent`'s help does not say it waits UNTIL the element is gone. \
         Read quickly, the flag name alone suggests waiting for something \
         that is already absent, which would be a command that returns \
         immediately.\n{help}"
    );
}

/// Absence is refused when the element is still there, and refused with
/// a non-zero exit — that is the whole difference between an assertion
/// and a print.
///
/// No runner here: with nothing listening the call fails on transport,
/// which still proves the exit code is not hard-wired to 0 the way
/// `smix find`'s is. What it cannot prove is the timeout path; that
/// belongs to a device gate.
#[test]
fn absent_does_not_exit_zero_when_it_cannot_answer() {
    let out = smix()
        .args(["wait-for", "id:nothing-here", "--absent", "--port", "1"])
        .output()
        .expect("run smix");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "`smix wait-for --absent` exited 0 with no runner to ask. An \
         assertion that cannot fail is a print statement."
    );
    // Without this, the test passed before `--absent` existed: an
    // unknown flag also exits non-zero. A test green before its feature
    // is written is measuring the parser's rejection, not the command.
    assert!(
        !said.contains("unexpected argument") && !said.contains("unrecognized"),
        "the failure was the argument parser refusing `--absent`, not the \
         command running and failing.\n{said}"
    );
}

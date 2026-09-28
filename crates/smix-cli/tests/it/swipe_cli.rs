//! `smix swipe` exists, and takes the same four directions the MCP tool does.
//!
//! `smix_swipe` has been in the MCP server for as long as the server has
//! existed. The CLI never grew a counterpart, so the one capability a
//! plugin is not allowed to have — something smix cannot do on its own —
//! was sitting in the tool list. The v2.13 cold plan wrote that rule
//! down; the parity gate only ever checked the other direction, that a
//! skill does not name a command the CLI lacks.
//!
//! Direction words are asserted here rather than trusted to the parser,
//! because "which way does `down` mean" is a decision the two surfaces
//! have to share: it names what you want to SEE, not which way the
//! finger moves. A CLI that inverted it would be a working command that
//! did the opposite thing.

use std::process::Command;

fn smix() -> Command {
    Command::new(env!("CARGO_BIN_EXE_smix"))
}

#[test]
fn swipe_is_a_subcommand() {
    let out = smix().args(["swipe", "--help"]).output().expect("run smix");
    assert!(
        out.status.success(),
        "`smix swipe --help` failed — MCP has smix_swipe and the CLI must \
         too.\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn swipe_help_names_all_four_directions() {
    let out = smix().args(["swipe", "--help"]).output().expect("run smix");
    let help = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    for dir in ["up", "down", "left", "right"] {
        assert!(
            help.contains(dir),
            "`smix swipe --help` does not mention `{dir}`. The MCP tool takes \
             all four; a CLI that takes fewer is a narrower capability wearing \
             the same name.\n{help}"
        );
    }
}

/// The direction is what you want to see, and the help has to say so.
///
/// Both surfaces drive `swipe_once`, so they cannot disagree about the
/// gesture — but they can disagree about what the word means to a
/// reader, and only one of them documents it today.
#[test]
fn swipe_help_says_the_direction_is_what_you_want_to_see() {
    let out = smix().args(["swipe", "--help"]).output().expect("run smix");
    let help = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        help.contains("reveals") || help.contains("want to see"),
        "`smix swipe --help` does not say that the direction names what you \
         want to see rather than which way the finger moves. `smix_swipe`'s \
         description does, and a user reading only one of them would guess \
         wrong half the time.\n{help}"
    );
}

/// An unknown direction is refused, and the refusal is about the
/// direction.
///
/// Asserting only on a non-zero exit passed before `swipe` existed at
/// all — every argument fails when the subcommand does not parse. A test
/// that is green before its feature is written is measuring the wrong
/// thing, so this reads what the refusal says.
#[test]
fn an_unknown_direction_is_refused_as_a_direction() {
    let out = smix()
        .args(["swipe", "sideways"])
        .output()
        .expect("run smix");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        !out.status.success(),
        "`smix swipe sideways` succeeded. A direction the driver cannot \
         express has to fail loudly, not pick something."
    );
    assert!(
        said.contains("sideways") || said.contains("direction"),
        "`smix swipe sideways` failed without saying the direction was the \
         problem. The reader has to learn which word was wrong.\n{said}"
    );
}

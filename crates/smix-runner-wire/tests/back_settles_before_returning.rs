//! `back` decides it has landed with `NavigationSettle`, not by reading
//! an unfindable navigation bar as arrival.
//!
//! The bar's identifier is the screen title, so a change in it is the
//! "we moved" signal — that part was always right. The shortcut beside
//! it was not: `if !bar.exists { return true }` treated a bar that could
//! not be found as arrival, and during a pop animation the bar is
//! momentarily unfindable. So `back` returned before landing, and the
//! assertion after it read the screen being left behind. It surfaced as
//! one flake in ten corpus runs on `nav-accessibility-and-back`, whose
//! own failure diagnostics listed the departing screen's navigation bar
//! and back button as still on screen.
//!
//! The decision now lives in `SmixRunnerCore.NavigationSettle`, where
//! `swift test` drives it with reading sequences. This guards the wiring:
//! the UITest body must take readings and ask, not decide inline. Running
//! it needs a simulator, so a source assertion is the only device-free
//! guard the call site can have — the same reasoning as
//! `tree_root_identity.rs` next door.

const UITESTS: &str =
    include_str!("../../../swift-bridge/SmixRunnerUITests/SmixRunnerUITests.swift");

/// Swift source with `//` lines removed.
///
/// Not cosmetic. The comment explaining this fix names the deleted line
/// verbatim, so a scan of the raw text finds the very string it exists to
/// forbid and fails on a correct file. Three separate gates in this cycle
/// were fooled by prose containing the code they were checking for, every
/// time in the direction that reports coverage that is not there.
fn code_only(src: &str) -> String {
    src.lines()
        .filter(|l| !l.trim_start().starts_with("//"))
        .collect::<Vec<_>>()
        .join("\n")
}

#[test]
fn back_does_not_read_an_absent_bar_as_arrival() {
    let code = code_only(UITESTS);
    assert!(
        !code.contains("if !bar.exists { return true }"),
        "the back handler still treats an unfindable navigation bar as \
         arrival. During a pop animation the bar blinks out for a frame, \
         so this returns before the navigation lands and whatever asserts \
         next reads the screen being left behind."
    );
}

#[test]
fn back_asks_navigation_settle() {
    let code = code_only(UITESTS);
    assert!(
        code.contains("NavigationSettle("),
        "the back handler does not construct a NavigationSettle. Whatever \
         it does instead is a decision made inline, where no test can \
         reach it — which is how the absent-bar shortcut survived."
    );
    assert!(
        code.contains(".observe("),
        "the back handler never feeds a reading to NavigationSettle. \
         Constructing one and deciding by hand anyway would satisfy the \
         assertion above while changing nothing."
    );
}

/// The three readings must all be produced, or the machine is being fed
/// a narrower world than it was written for.
///
/// `.unreadable` is the one at risk: a snapshot that throws is easy to
/// collapse into `.absent`, and then "I could not look" would count
/// toward the run of absences that means the destination has no bar.
#[test]
fn back_distinguishes_absent_from_unreadable() {
    let code = code_only(UITESTS);
    for reading in [".absent", ".title(", ".unreadable"] {
        assert!(
            code.contains(reading),
            "the back handler never produces `{reading}`. NavigationSettle \
             treats absence, a title and an unreadable snapshot as three \
             different things; a call site that produces only two of them \
             has folded one into another."
        );
    }
}

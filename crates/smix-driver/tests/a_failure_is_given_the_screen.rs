//! A failure's screen goes in through one door.
//!
//! Seven call sites built a failure by collecting ten visible elements and
//! handing them over. The ten were cut at collection, so the total was gone
//! before the failure was made, and on Android the ten were the status bar.
//! `FailureInit::with_screen` takes the list, the total and the windows
//! together; this checks that no site goes round it.

const SOURCES: &[(&str, &str)] = &[
    ("smix-driver/src/lib.rs", include_str!("../src/lib.rs")),
    (
        "smix-driver/src/android.rs",
        include_str!("../src/android.rs"),
    ),
    (
        "smix-driver/src/scroll_until.rs",
        include_str!("../src/scroll_until.rs"),
    ),
    (
        "smix-sdk/src/lib.rs",
        include_str!("../../smix-sdk/src/lib.rs"),
    ),
];

#[test]
fn no_failure_is_handed_a_list_without_its_count() {
    let mut offenders = Vec::new();
    for (name, src) in SOURCES {
        for (i, line) in src.lines().enumerate() {
            let t = line.trim_start();
            if t.starts_with("//") {
                continue;
            }
            if t.starts_with("visible_elements:") {
                offenders.push(format!("{name}:{}: {t}", i + 1));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these build a failure's element list by hand, without the total or the windows — use `.with_screen(screen_facts(&tree, 10))`:\n{}",
        offenders.join("\n")
    );
}

/// The check above reads four files for one spelling. If none of them held
/// a failure at all, it would pass on nothing.
#[test]
fn the_files_it_reads_do_build_failures() {
    let with_screen: usize = SOURCES
        .iter()
        .map(|(_, s)| s.matches(".with_screen(").count() + s.matches(".with_screen_from(").count())
        .sum();
    assert!(
        with_screen >= 7,
        "expected the seven sites that build a failure from the screen to go through with_screen / with_screen_from; found {with_screen}"
    );
}

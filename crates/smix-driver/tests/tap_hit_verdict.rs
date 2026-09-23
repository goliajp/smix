//! Did the touch land inside the element it aimed at?
//!
//! `tapOn` reported success ten times in a row against an icon button
//! whose app-side counter never moved (EXT1, 2026-07-22). It was
//! telling the truth about what it did — a touch was synthesised at a
//! coordinate — and that is not what a reader takes "tapped" to mean.
//!
//! The chains below are not invented. They were read off a live
//! iPhone 17 Pro running Settings (iOS 26.5, 2026-07-22), from the tree
//! committed beside this file. That tree is why the rule is
//! containment and not identity: at the centre of the first row, the
//! innermost named element is the row's own label, not the row.

use smix_driver::{ActVerdict, ChainCoverage, HitElement, tap_landed_within};

fn el(id: &str, label: &str, frame: (f64, f64, f64, f64)) -> HitElement {
    HitElement {
        identifier: id.to_string(),
        label: label.to_string(),
        frame,
    }
}

/// The named elements containing the centre of Settings' first row,
/// innermost first, exactly as observed.
fn settings_first_row_chain() -> Vec<HitElement> {
    vec![
        el(
            "",
            "登录以访问iCloud数据、App Store等",
            (24.0, 196.0, 216.0, 33.0),
        ),
        el(
            "com.apple.settings.primaryAppleAccount",
            "Apple账户、登录以访问iCloud数据、App Store等",
            (16.0, 168.0, 370.0, 90.0),
        ),
        el("com.apple.Preferences", "设置", (0.0, 0.0, 402.0, 874.0)),
    ]
}

/// The case that killed the identity rule.
///
/// A flow aims at the row's button and taps its centre. The innermost
/// element there is the button's own label. Identity called this a
/// miss; it is a perfectly good tap.
#[test]
fn a_tap_that_lands_on_the_aimed_elements_own_label_is_confirmed() {
    let aimed = el(
        "com.apple.settings.primaryAppleAccount",
        "Apple账户、登录以访问iCloud数据、App Store等",
        (16.0, 168.0, 370.0, 90.0),
    );
    assert_eq!(
        tap_landed_within(
            &aimed,
            &settings_first_row_chain(),
            ChainCoverage::NamedOnly
        ),
        ActVerdict::Confirmed
    );
}

/// The reported case: the point is inside something unrelated.
#[test]
fn a_point_inside_none_of_the_aimed_element_names_what_was_there() {
    let aimed = el("landing-logo", "", (100.0, 100.0, 80.0, 80.0));
    let chain = vec![
        el("overlay-scrim", "", (0.0, 0.0, 402.0, 874.0)),
        el("com.example.app", "Insight", (0.0, 0.0, 402.0, 874.0)),
    ];
    let ActVerdict::Missed(why) = tap_landed_within(&aimed, &chain, ChainCoverage::NamedOnly)
    else {
        panic!("a point inside neither the element nor its ancestors is a miss");
    };
    assert!(
        why.contains("landing-logo") && why.contains("overlay-scrim"),
        "a miss names what was aimed at and what was there: {why}"
    );
}

/// Aiming at an ancestor is fine too — a flow may target the row.
#[test]
fn aiming_at_an_ancestor_on_the_chain_is_confirmed() {
    let aimed = el("com.apple.Preferences", "设置", (0.0, 0.0, 402.0, 874.0));
    assert_eq!(
        tap_landed_within(
            &aimed,
            &settings_first_row_chain(),
            ChainCoverage::NamedOnly
        ),
        ActVerdict::Confirmed
    );
}

/// Nothing at the point at all — the frame was stale.
#[test]
fn an_empty_chain_is_a_miss() {
    let aimed = el("btn-login", "", (10.0, 20.0, 80.0, 30.0));
    let ActVerdict::Missed(why) = tap_landed_within(&aimed, &[], ChainCoverage::NamedOnly) else {
        panic!("a tap that landed on nothing is a miss");
    };
    assert!(
        why.contains("btn-login") && why.to_lowercase().contains("nothing"),
        "{why}"
    );
}

/// Unnamed elements are matched by geometry, since that is all there is.
#[test]
fn an_unnamed_element_is_found_by_its_frame() {
    let aimed = el("", "", (16.0, 168.0, 370.0, 90.0));
    let chain = vec![el("", "", (16.4, 168.0, 370.0, 90.0))];
    assert_eq!(
        tap_landed_within(&aimed, &chain, ChainCoverage::NamedOnly),
        ActVerdict::Confirmed
    );
}

/// An unnamed target against a named chain cannot be judged.
///
/// The load-bearing case. Calling this `Confirmed` would put the check
/// in the category it exists to remove — an answer asserting more than
/// it checked — and calling it `Missed` would fail correct taps on
/// elements the tree does not name.
#[test]
fn an_unnamed_target_against_a_named_chain_is_unconfirmable() {
    let aimed = el("", "", (0.0, 0.0, 0.0, 0.0));
    let verdict = tap_landed_within(
        &aimed,
        &settings_first_row_chain(),
        ChainCoverage::NamedOnly,
    );
    let ActVerdict::Unconfirmable(why) = verdict else {
        panic!("expected Unconfirmable, got {verdict:?}");
    };
    assert!(
        why.contains("identifier") && why.contains("label"),
        "it has to say what it was missing: {why}"
    );
}

/// The fixture is still the tree these chains were read from.
///
/// A check on the evidence, not on the rule: the chains above are
/// hand-transcribed, and if the tree they came from is gone there is
/// nothing left saying they were ever real.
#[test]
fn the_captured_tree_still_holds_the_element_these_chains_name() {
    let tree = include_str!("fixtures/live-tree-preferences-mini-2026-07-22.json");
    for needle in [
        "com.apple.settings.primaryAppleAccount",
        "com.apple.Preferences",
    ] {
        assert!(
            tree.contains(needle),
            "the captured tree no longer contains {needle} — the chains \
             in this file no longer stand for anything observed"
        );
    }
}

/// The fixture's dialog count label, and what an Android touch at y=2122
/// was delivered to: the activity's root, and nothing else.
///
/// An Android chain lists every element under the point, named or not,
/// so a target that is not in it was not touched. A consumer's confirm
/// was pressed there and reported as a tap.
fn android_chain_below_the_dialog() -> Vec<HitElement> {
    vec![el("", "", (0.0, 0.0, 1080.0, 2340.0))]
}

#[test]
fn an_unnamed_target_missing_from_a_chain_that_lists_everything_is_a_miss() {
    let aimed = el("", "", (44.0, 357.0, 201.0, 45.0));
    assert!(matches!(
        tap_landed_within(
            &aimed,
            &android_chain_below_the_dialog(),
            ChainCoverage::Every
        ),
        ActVerdict::Missed(_)
    ));
}

#[test]
fn an_unnamed_target_found_by_its_frame_in_a_chain_that_lists_everything_is_confirmed() {
    let aimed = el("", "", (44.0, 357.0, 201.0, 45.0));
    let chain = vec![
        el("", "", (44.0, 357.0, 201.0, 45.0)),
        el("", "", (0.0, 0.0, 1080.0, 2340.0)),
    ];
    assert_eq!(
        tap_landed_within(&aimed, &chain, ChainCoverage::Every),
        ActVerdict::Confirmed
    );
}

#[test]
fn an_unnamed_target_missing_from_a_chain_of_named_elements_cannot_be_judged() {
    // The iOS chain leaves unnamed elements out, so absence from it is
    // not evidence — unchanged, and the reason the coverage is carried.
    let aimed = el("", "", (44.0, 357.0, 201.0, 45.0));
    assert!(matches!(
        tap_landed_within(
            &aimed,
            &settings_first_row_chain(),
            ChainCoverage::NamedOnly
        ),
        ActVerdict::Unconfirmable(_)
    ));
}

#[test]
fn a_named_target_missing_from_a_chain_that_lists_everything_is_a_miss() {
    let aimed = el("button1", "", (773.0, 1192.0, 203.0, 149.0));
    assert!(matches!(
        tap_landed_within(
            &aimed,
            &android_chain_below_the_dialog(),
            ChainCoverage::Every
        ),
        ActVerdict::Missed(_)
    ));
}

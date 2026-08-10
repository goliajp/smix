//! The fixture app carries what the portable corpus needs to drive.
//!
//! Twenty of the twenty-one corpus flows drive the system Settings app,
//! with identifiers like `com.apple.settings.actionButton` that differ
//! by iOS version and by device model. This machine runs iOS 26.5; a CI
//! runner will not. Moving those flows to CI unchanged would produce a
//! gate whose red says nothing about smix — worse than a gate a
//! bystander can turn red, because that kind at least points at a real
//! conflict.
//!
//! So the fixture grows a scrollable list and a navigation stack, and
//! the corpus gains portable counterparts. The Settings flows stay: a
//! real system app is a subject the fixture cannot imitate, and it has
//! caught defects the fixture could not reach.
//!
//! Read out of the fixture source rather than restated here. A test that
//! carries its own copy of the identifiers passes when the app and the
//! copy agree with each other and disagree with the corpus — this cycle
//! wrote a probe that asserted on an id belonging to a different
//! fixture entirely, and a gate whose comment named the very string it
//! forbade.

const FIXTURE: &str = include_str!("../../../test-fixtures/demo-app/main.swift");

/// Identifiers the fixture declares, in source order.
fn identifiers() -> Vec<String> {
    FIXTURE
        .lines()
        .filter_map(|l| {
            let at = l.find("accessibilityIdentifier(\"")?;
            let rest = &l[at + "accessibilityIdentifier(\"".len()..];
            let end = rest.find('"')?;
            Some(rest[..end].to_string())
        })
        .collect()
}

/// The three the fixture had before the portable tier, which
/// `fixture-fill-and-submit` still drives.
#[test]
fn the_original_three_are_untouched() {
    let ids = identifiers();
    for id in ["fixture-input", "fixture-submit", "fixture-result"] {
        assert!(
            ids.iter().any(|i| i == id),
            "`{id}` is gone from the fixture. The flow that drives it did not \
             change, so this would fail on the device instead of here.\nfound: \
             {ids:?}"
        );
    }
}

/// A list long enough that the target is off screen at launch.
///
/// A "scroll" flow whose target is already visible asserts nothing about
/// scrolling — it passes on a device that never scrolled, which is
/// exactly the shape of a test that looks like coverage.
#[test]
fn there_is_a_list_deeper_than_one_screen() {
    // Ids are generated in a loop, so the source carries the count as a
    // bound rather than N literals. Read it from there; a fixture that
    // stopped generating them would leave this at zero rather than
    // quietly passing on a hand-written handful.
    let count = FIXTURE
        .lines()
        .find_map(|l| {
            let at = l.find("let fixtureRowCount = ")?;
            l[at + "let fixtureRowCount = ".len()..]
                .trim()
                .parse::<usize>()
                .ok()
        })
        .unwrap_or(0);
    assert!(
        count >= 30,
        "the fixture list has {count} rows. A scroll flow needs its target \
         off screen at launch, or it passes without scrolling."
    );
}

/// Somewhere to navigate into, and a system back button to come out of.
///
/// The Settings flows push a detail screen and use the navigation bar's
/// back button. A portable counterpart that used a bespoke "close"
/// button would exercise a different code path and answer a different
/// question.
#[test]
fn there_is_a_navigation_stack_with_a_detail_screen() {
    assert!(
        FIXTURE.contains("NavigationStack"),
        "the fixture has no NavigationStack, so a ported `nav-and-back` flow \
         would not be driving the same thing the Settings one does."
    );
    let ids = identifiers();
    assert!(
        ids.iter().any(|i| i == "fixture-detail"),
        "the fixture declares no `fixture-detail`. A nav flow needs \
         something on the destination to assert, or arriving and not \
         arriving look the same.\nfound: {ids:?}"
    );
    assert!(
        ids.iter().any(|i| i == "fixture-detail-link"),
        "the fixture declares no `fixture-detail-link` to tap.\nfound: {ids:?}"
    );
}

/// Something to long-press, mirroring `longpress-account`.
#[test]
fn there_is_a_long_press_target() {
    let ids = identifiers();
    assert!(
        ids.iter().any(|i| i == "fixture-longpress"),
        "the fixture declares no `fixture-longpress`.\nfound: {ids:?}"
    );
}

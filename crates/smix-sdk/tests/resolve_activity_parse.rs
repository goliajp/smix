//! What the device actually printed, kept so the parse stays checkable.
//!
//! Every Android launch used to run `am start -n <pkg>/.MainActivity`.
//! On an API 33 emulator that is:
//!
//! ```text
//! Error type 3
//! Error: Activity class {com.android.settings/com.android.settings.MainActivity} does not exist.
//! ```
//!
//! Settings' launcher activity is `.Settings`, and the package manager
//! says so. The fixtures beside this file are `cmd package
//! resolve-activity --brief -c android.intent.category.LAUNCHER`'s
//! output verbatim — one hit, one miss — captured 2026-07-22 from
//! `sim-smix-android-01` (API 33, Android 13).
//!
//! Committing them turns a device observation into something the
//! workspace suite re-checks on every run, which is the point: the
//! failure mode this guards is a shell output format changing shape
//! and the parse quietly falling back to the convention that does not
//! work.

use smix_sdk::parse_resolved_activity;

const HIT: &str = include_str!("fixtures/resolve-activity-settings-2026-07-22.txt");
const MISS: &str = include_str!("fixtures/resolve-activity-absent-2026-07-22.txt");

#[test]
fn the_component_line_yields_a_relative_activity() {
    assert_eq!(
        parse_resolved_activity(HIT, "com.android.settings").as_deref(),
        Some(".Settings"),
        "the captured output is:\n{HIT}"
    );
}

/// The fixture is still the two-line shape a device produced.
///
/// A check on the fixture, not on the parse: reduced to its component
/// line it would still parse, and would no longer stand for what a
/// device prints. That is the way this file could quietly stop meaning
/// anything.
#[test]
fn the_captured_output_still_has_its_header() {
    assert!(
        HIT.lines().count() >= 2,
        "the fixture lost its header line and no longer stands for real output"
    );
    assert!(
        HIT.lines().next().unwrap().starts_with("priority="),
        "the fixture's first line is no longer the header a device printed"
    );
}

/// A package with no launcher activity — and, identically, one that is
/// not installed.
#[test]
fn no_activity_found_yields_nothing() {
    assert_eq!(parse_resolved_activity(MISS, "com.example.nosuchapp"), None);
}

/// A fully-qualified component collapses to the relative form.
///
/// `AdbClient::start_activity` prefixes the package itself, so
/// returning the absolute name would send `<pkg>/<pkg>.Settings`.
#[test]
fn a_fully_qualified_component_is_relativised() {
    assert_eq!(
        parse_resolved_activity(
            "com.example.app/com.example.app.SplashActivity",
            "com.example.app"
        )
        .as_deref(),
        Some(".SplashActivity")
    );
}

/// Output this does not recognise leaves the caller where it was.
#[test]
fn an_unrecognised_shape_yields_nothing() {
    for text in ["", "Exception: something went wrong", "com.other.app/.Main"] {
        assert_eq!(
            parse_resolved_activity(text, "com.android.settings"),
            None,
            "{text:?} was read as an activity"
        );
    }
}

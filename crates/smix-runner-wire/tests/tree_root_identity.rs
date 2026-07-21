//! The tree root reports the bundle THIS request resolved to.
//!
//! `resolveApp()` rebinds per request off the App-Bundle-Id header, but
//! the root identifier was written from the runner's launch-time
//! constant. A client driving a second app therefore got that app's
//! snapshot carrying the wrong id at its root — correct nearly always,
//! wrong exactly when someone switches app, which is the only time
//! anybody reads the field.
//!
//! `describe()` reads front_app from that identifier, so this is the
//! source that field's honesty rests on.
//!
//! Checked against the source because the line lives in the XCUITest
//! target: `swift test` does not compile it, `xcodebuild
//! build-for-testing` compiles but does not run it, and running it
//! needs a simulator. A source assertion is the only device-free guard
//! this line can have.

const UITESTS: &str =
    include_str!("../../../swift-bridge/SmixRunnerUITests/SmixRunnerUITests.swift");

#[test]
fn tree_root_identifier_tracks_per_request_bundle() {
    let marker = "rootIdentifierOverride: resolvedBundle";
    assert!(
        UITESTS.contains(marker),
        "the tree root's identifier is no longer taken from the per-request \
         bundle. If it went back to the launch-time constant, describe()'s \
         front_app silently misreports whenever a client drives a second app."
    );

    let resolved = "let resolvedBundle = SmixRunnerServer.currentContext.bundleId ?? bundleId";
    assert!(
        UITESTS.contains(resolved),
        "resolvedBundle is no longer derived from the request context; it must \
         follow the same priority resolveApp() uses, not a second chain."
    );
}

/// The see-through path builds its own synthetic root and had the same
/// constant. Both paths or neither — a caller cannot tell which one
/// answered.
#[test]
fn see_through_root_identifier_tracks_per_request_bundle_too() {
    let marker = "identifier: SmixRunnerServer.currentContext.bundleId ?? bundleId,";
    assert!(
        UITESTS.contains(marker),
        "the all-windows synthetic root went back to the launch-time bundle, so \
         the two tree paths now disagree about which app answered."
    );
}

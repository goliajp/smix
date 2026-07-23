//! The TS SDK drives, and stays driving.
//!
//! v2.9 retired the TypeScript SDK's `SmixNotImplementedError('napi', ...)`
//! stubs: `App` and `Smix.launchApp` now drive through the napi addon rather
//! than throwing. That closes the napi-axis parity with the Swift / Kotlin /
//! Rust SDKs — but nothing stopped a future edit from reintroducing a stub and
//! quietly reopening the gap. This reads the TS source and refuses to let a
//! `'napi'` stub come back, and refuses to let the driving methods be emptied
//! so the check passes by knowing nothing.
//!
//! The three surfaces still pending (`screenshot` / `openUrl` / `launchFresh`)
//! throw with a `'wire'` / `'host'` stage, not `'napi'` — they are a separate,
//! recorded gap, and the Swift SDK has no such methods either, so they are not
//! a napi-axis regression.

/// How many `'napi'`-staged not-implemented stubs a source still carries.
/// The single source of truth is the TS source itself — no second copy of the
/// method list is written here.
fn count_napi_stubs(src: &str) -> usize {
    src.matches("SmixNotImplementedError('napi'").count()
}

const APP_TS: &str = include_str!("../../../npm/smix-rn/src/App.ts");
const SMIX_TS: &str = include_str!("../../../npm/smix-rn/src/Smix.ts");

#[test]
fn no_napi_stub_remains_in_ts_driving_surface() {
    assert_eq!(
        count_napi_stubs(APP_TS),
        0,
        "App.ts reintroduced a SmixNotImplementedError('napi', ...) stub — the \
         napi-axis retire regressed"
    );
    assert_eq!(
        count_napi_stubs(SMIX_TS),
        0,
        "Smix.ts reintroduced a SmixNotImplementedError('napi', ...) stub"
    );
}

#[test]
fn ts_still_declares_the_wired_driving_verbs() {
    // Not a hand-copied contract: these are the verbs the retire wired, and
    // their absence would mean a method was deleted, not that it stopped
    // throwing — which this parity check would otherwise pass blind to.
    for verb in [
        "tap(",
        "fill(",
        "pressKey(",
        "swipe(",
        "tapAtCoord(",
        "terminate(",
        "relaunch(",
        "snapshotTree(",
        "systemPopups(",
    ] {
        assert!(
            APP_TS.contains(verb),
            "App.ts no longer declares `{verb}` — the driving surface was emptied, \
             not just un-stubbed"
        );
    }
    assert!(
        SMIX_TS.contains("launchApp("),
        "Smix.ts no longer declares the launchApp entry point"
    );
}

#[test]
fn an_injected_napi_stub_is_caught() {
    // The extractor must actually find a stub — a counter that always returns
    // zero would pass `no_napi_stub_remains` while seeing nothing.
    let fixture = "async tap() { throw new SmixNotImplementedError('napi', 'App.tap') }";
    assert!(
        count_napi_stubs(fixture) > 0,
        "the stub extractor failed to catch an injected napi stub — the gate is blind"
    );
}

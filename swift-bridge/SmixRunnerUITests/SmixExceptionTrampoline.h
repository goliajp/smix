#import <Foundation/Foundation.h>

NS_ASSUME_NONNULL_BEGIN

/// v1.4 ③-C1 B1 — NSException → NSError trampoline (UITest target only),
/// executed on the main thread.
///
/// Root cause (v1.md §7 2026-05-19, code-level): the runner's route
/// closures call XCUITest APIs (SmixRunnerUITests.swift:262
/// `element.tap()`, `:252 element.isHittable`, `:255 element.frame`,
/// `:292 app.snapshot()`, `findHandler .exists`). When a resolved element
/// vanishes mid-interaction XCUITest raises an Objective-C exception from
/// inside its private `_XCTFailureHandler` ("Failed to get matching
/// snapshot: No matches found …"). A Swift `do/catch` CANNOT intercept a
/// raw ObjC `@throw` — only an ObjC `@try/@catch` frame can. Without this
/// trampoline the exception unwinds through the FlyingFox route closure →
/// SmixRunnerServer.runForever's withThrowingTaskGroup re-throws it →
/// `test_runForever` terminates → xcodebuild restarts the whole runner.
///
/// Main-thread requirement (v1.md §7 2026-05-19 regression, sc4): the
/// FlyingFox route closures run on the cooperative pool, OFF the main
/// thread. Baseline (pre-B1) unwrapped `element.tap()` worked because
/// XCUITest internally marshals the synthesized touch dispatch. Wrapping
/// the call in an ObjC `@try/@catch` NS_NOESCAPE block frame disrupts that
/// internal marshaling, so `-[XCUIElementEmbeddedEventTarget
/// dispatchEventWithDescription:...]` hits its `XCUIElementBaseEventTarget.m:391`
/// `NSInternalInconsistencyException: Must be called on the main thread`
/// assertion. The fix: run `block` ON the main thread. The runner's
/// `test_runForever` executes on XCTest's background test thread (Maestro
/// long-running pattern), so the main run loop is alive and serviced —
/// `dispatch_sync` onto the main queue from the off-main FlyingFox handler
/// is well-defined and cannot deadlock (the calling thread is never the
/// main thread). When already on the main thread the block runs inline
/// (deadlock guard) — this never happens for FlyingFox handlers but keeps
/// the primitive correct under any caller.
///
/// This is pure Objective-C exception handling (`@try/@catch`) plus a GCD
/// main-queue hop. It is NOT a private symbol, NOT dlsym, NOT Apple-binary
/// patching — CLAUDE.md §9.6 is not engaged. It lives ONLY in the UITest
/// target so SmixRunnerCore keeps its XCTest/XCUI-free invariant.
///
/// On success returns YES and `error` is untouched. On a caught
/// NSException returns NO and `*error` is a Swift-catchable NSError
/// carrying the exception name + reason (AI-readable, CLAUDE.md §9.5).
BOOL SmixRunCatching(void (NS_NOESCAPE ^block)(void),
                      NSError * _Nullable * _Nullable error);

NS_ASSUME_NONNULL_END

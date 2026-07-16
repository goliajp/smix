import XCTest
@testable import SmixRunnerCore

// Verifies the tap mode that makes RN Pressable onPress fire reliably.
//
// Root cause: `TapRoute.resolveAndTap` drives `XCUIElement.tap()`, which
// synthesises pointerDown/pointerUp through Apple's gesture recognizer
// chain. RN Pressable's `RCTTouchHandler` UIGestureRecognizer, registered
// on the native side, gets that synthesised gesture cancelled or routed
// past it — so the JS-thread `onPress` callback never fires reliably.
//
// The fix is a third TapMode — `.daemonProxySynthesize` — alongside
// `.resolve` and `.resolveAndTap`. The UITests-side tapHandler dispatches
// that mode through the EventSynthesizer path
// (`XCSynthesizedEventRecord` + `XCPointerEventPath` +
// `XCTRunnerDaemonSession.daemonProxy._XCT_synthesizeEvent:completion:`
// via ObjC dlsym, the same route tap-at-norm-coord takes): the runner
// resolves the element's frame centre coord, then emits a raw IOKit-level
// touch event with NO XCUIElement-owner metadata, which UIKit's standard
// hit-test routes through RN's `RCTTouchHandler` UIGestureRecognizer →
// Pressable onPress fires.
//
// This test verifies the Core-layer wire: decode accepts the new mode
// string, the request carries it through, and the response builder
// serialises the matched envelope unchanged (no new fields on the
// response — the mode is request-only).

final class TapRouteG8FixTests: XCTestCase {
    // -- decode --

    func test_decode_modeDaemonProxySynthesize_returnsRequest() throws {
        let body = Data(#"{"selector":{"text":"Sign In"},"mode":"daemonProxySynthesize"}"#.utf8)
        let req = try TapRoute.decode(body)
        XCTAssertEqual(req.mode, .daemonProxySynthesize)
        XCTAssertEqual(req.selector.text, "Sign In")
    }

    func test_decode_modeDaemonProxySynthesize_rawValueRoundTrip() {
        // Wire-format invariant: SDK side must be able to construct the
        // request with `"mode":"daemonProxySynthesize"` exactly.
        XCTAssertEqual(TapRoute.TapMode.daemonProxySynthesize.rawValue, "daemonProxySynthesize")
        XCTAssertEqual(
            TapRoute.TapMode(rawValue: "daemonProxySynthesize"),
            .daemonProxySynthesize
        )
    }

    // -- backward compat: existing modes still decode --

    func test_decode_existingModes_unaffected() throws {
        let bodyResolve = Data(#"{"selector":{"text":"General"},"mode":"resolve"}"#.utf8)
        XCTAssertEqual(try TapRoute.decode(bodyResolve).mode, .resolve)
        let bodyResolveAndTap = Data(#"{"selector":{"text":"General"},"mode":"resolveAndTap"}"#.utf8)
        XCTAssertEqual(try TapRoute.decode(bodyResolveAndTap).mode, .resolveAndTap)
        let bodyDefault = Data(#"{"selector":{"text":"General"}}"#.utf8)
        XCTAssertEqual(try TapRoute.decode(bodyDefault).mode, .resolveAndTap)
    }

    // -- response wire-compat --
    //
    // The new mode is request-only; the response wire shape is unchanged
    // (matched.label only, no extra fields). This test re-checks the
    // existing `success` builder still
    // returns the same envelope when called from any mode path.

    func test_success_matchedLabel_envelopeUnchanged() async throws {
        let resp = TapRoute.success(matchedLabel: "Sign In")
        XCTAssertEqual(resp.statusCode, .ok)
        let bodyStr = try await String(decoding: resp.bodyData, as: UTF8.self)
        XCTAssertTrue(bodyStr.contains(#""ok":true"#), bodyStr)
        XCTAssertTrue(bodyStr.contains(#""matched""#), bodyStr)
        XCTAssertTrue(bodyStr.contains(#""label":"Sign In""#), bodyStr)
    }
}

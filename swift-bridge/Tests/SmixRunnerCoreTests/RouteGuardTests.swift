import XCTest
import FlyingFox
@testable import SmixRunnerCore

// Server-side route guard (defense-in-depth layer 1).
//
// Root cause: a route closure that invokes a
// throwing/failing XCUITest API (SmixRunnerUITests.swift:262 element.tap()
// when the element vanishes mid-interaction) lets the ObjC exception /
// Swift error propagate out of the closure → through FlyingFox →
// SmixRunnerServer.runForever's withThrowingTaskGroup (`:288-309`) which
// re-throws every non-shutdown error → test_runForever terminates →
// xcodebuild restarts the whole runner.
//
// The guard is the purely-additive server-side layer: a route response builder
// that can throw is wrapped so any thrown error is converted into the
// route's own error envelope (4xx/5xx) and NEVER escapes the closure. This
// is unit-decidable in SmixRunnerCore (no XCUI/XCTest, no sim): inject a
// throwing builder, assert (a) the wrapper returns the supplied fallback
// envelope, (b) it does not rethrow, (c) a non-throwing builder is
// byte-identical to calling it directly (zero behaviour change on the
// happy path = zero regression for every existing route).
final class RouteGuardTests: XCTestCase {

  struct Boom: Error {}

  func test_guardedResponse_throwingBuilder_returnsFallbackEnvelope() async {
    let fallback = TreeRoute.unavailable()
    let resp = await SmixRunnerServer.guardedResponse(fallback: fallback) {
      throw Boom()
    }
    XCTAssertEqual(resp.statusCode, fallback.statusCode)
  }

  func test_guardedResponse_throwingBuilder_doesNotRethrow() async {
    // The whole point: invoking the guard with a throwing builder must
    // return normally (no error escapes to the task group → server stays
    // alive). Reaching the assertion at all proves no rethrow.
    let resp = await SmixRunnerServer.guardedResponse(
      fallback: TapRoute.badRequest(reason: "guarded")
    ) {
      throw Boom()
    }
    XCTAssertEqual(resp.statusCode, .badRequest)
  }

  func test_guardedResponse_nonThrowingBuilder_passesThroughUnchanged() async throws {
    let payload = Data(#"{"ok":true,"x":1}"#.utf8)
    let direct = TreeRoute.success(payload)
    let guarded = await SmixRunnerServer.guardedResponse(
      fallback: TreeRoute.unavailable()
    ) {
      direct
    }
    XCTAssertEqual(guarded.statusCode, direct.statusCode)
    XCTAssertEqual(guarded.headers[.contentType], direct.headers[.contentType])
    let gBody = try await guarded.bodyData
    XCTAssertEqual(gBody, payload)
  }

  func test_guardedResponse_throwingBuilder_fallbackBodyIsErrorEnvelope() async throws {
    let resp = await SmixRunnerServer.guardedResponse(
      fallback: TreeRoute.unavailable()
    ) {
      throw Boom()
    }
    let body = try await resp.bodyData
    let json = try JSONSerialization.jsonObject(with: body) as? [String: Any]
    XCTAssertEqual(json?["ok"] as? Bool, false)
    XCTAssertEqual(json?["error"] as? String, "snapshot_unavailable")
  }
}

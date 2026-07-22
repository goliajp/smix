import XCTest
import FlyingFox
@testable import SmixRunnerCore

// LongPressRoute POCO unit tests — decode + badRequest envelope.
//
// success(ok:) is deliberately NOT asserted here: the Rust client
// (smix-runner-client long_press) deserializes the /long-press response
// as `TapResult`, whose fields are all optional-with-default — the
// `{"ok":bool}` body this route emits parses to an all-None TapResult,
// so the `ok:false` failure signal is silently dropped on the Rust
// side. Same silently-dropped-shape class as the fixed /tap bug;
// codifying the emission here would enshrine the wrong contract.
final class LongPressRouteTests: XCTestCase {
  // -- decode --

  func test_decode_validBody_returnsRequest() throws {
    let body = Data(#"{"selector":{"text":"General"},"durationMs":750}"#.utf8)
    let req = try LongPressRoute.decode(body)
    XCTAssertEqual(
      req, LongPressRoute.LongPressRequest(selector: .text("General"), durationMs: 750))
  }

  func test_decode_emptyBody_throwsInvalidJSON() {
    XCTAssertThrowsError(try LongPressRoute.decode(Data())) { err in
      XCTAssertEqual(err as? LongPressRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_malformedJSON_throwsInvalidJSON() {
    let body = Data("{not json".utf8)
    XCTAssertThrowsError(try LongPressRoute.decode(body)) { err in
      XCTAssertEqual(err as? LongPressRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_rootNotObject_throwsWrongType() {
    let body = Data(#"[1,2]"#.utf8)
    XCTAssertThrowsError(try LongPressRoute.decode(body)) { err in
      XCTAssertEqual(err as? LongPressRoute.DecodeError, .wrongType("root not object"))
    }
  }

  func test_decode_missingSelector_throwsMissingSelector() {
    let body = Data(#"{"durationMs":500}"#.utf8)
    XCTAssertThrowsError(try LongPressRoute.decode(body)) { err in
      XCTAssertEqual(err as? LongPressRoute.DecodeError, .missingSelector)
    }
  }

  func test_decode_selectorNotObject_throwsWrongType() {
    let body = Data(#"{"selector":42,"durationMs":500}"#.utf8)
    XCTAssertThrowsError(try LongPressRoute.decode(body)) { err in
      XCTAssertEqual(err as? LongPressRoute.DecodeError, .wrongType("selector not object"))
    }
  }

  func test_decode_selectorWithoutText_throwsMissingText() {
    let body = Data(#"{"selector":{},"durationMs":500}"#.utf8)
    XCTAssertThrowsError(try LongPressRoute.decode(body)) { err in
      XCTAssertEqual(err as? LongPressRoute.DecodeError, .unsupportedSelectorForm)
    }
  }

  func test_decode_textIsNumber_throwsWrongType() {
    let body = Data(#"{"selector":{"text":7},"durationMs":500}"#.utf8)
    XCTAssertThrowsError(try LongPressRoute.decode(body)) { err in
      XCTAssertEqual(err as? LongPressRoute.DecodeError, .wrongType("selector.text not string"))
    }
  }

  func test_decode_missingDuration_throwsMissingDuration() {
    let body = Data(#"{"selector":{"text":"General"}}"#.utf8)
    XCTAssertThrowsError(try LongPressRoute.decode(body)) { err in
      XCTAssertEqual(err as? LongPressRoute.DecodeError, .missingDuration)
    }
  }

  func test_decode_durationIsString_throwsWrongType() {
    let body = Data(#"{"selector":{"text":"General"},"durationMs":"long"}"#.utf8)
    XCTAssertThrowsError(try LongPressRoute.decode(body)) { err in
      XCTAssertEqual(err as? LongPressRoute.DecodeError, .wrongType("durationMs not unsigned int"))
    }
  }

  // -- response builders --

  func test_badRequest_400ExactBody() async throws {
    let resp = LongPressRoute.badRequest(reason: "bad json")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false,"error":"bad_request","reason":"bad json"}"#)
  }

  func test_badRequest_escapesQuoteAndNewline() async throws {
    let resp = LongPressRoute.badRequest(reason: "he\"llo\nx")
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false,"error":"bad_request","reason":"he\"llo\nx"}"#)
  }
  func test_decode_idForm() throws {
    let body = Data(#"{"selector":{"id":"btn-login"},"durationMs":800}"#.utf8)
    let req = try LongPressRoute.decode(body)
    XCTAssertEqual(req.selector, .id("btn-login"))
  }

  // `press(forDuration:)` does not report when the touch went down, so
  // the route reports bounds derived from the call's own span. A host
  // that took these for instants would claim a frame was captured
  // during a press it was not — which is the failure that asked for
  // this in the first place.

  func test_timings_boundTheHoldInsideTheCallSpan() {
    // Entered at 1000, called 1200..2500, asked to hold 1000ms.
    let t = LongPressRoute.PressTimings.around(
      callStartMs: 1200, callEndMs: 2500, holdMs: 1000, handlerEntryMs: 1000)
    XCTAssertEqual(t.latestDownOffsetMs, 500)    // (2500-1000) - 1000
    XCTAssertEqual(t.earliestUpOffsetMs, 1200)   // (1200+1000) - 1000
    XCTAssertEqual(t.handlerWallMs, 1500)
  }

  /// A call that returned in less than the hold it was given leaves no
  /// interval bounded by both ends; the down bound must not run past
  /// the call's own start.
  func test_timings_doNotPlaceTouchDownBeforeTheCallBegan() {
    let t = LongPressRoute.PressTimings.around(
      callStartMs: 1200, callEndMs: 1400, holdMs: 1000, handlerEntryMs: 1000)
    XCTAssertEqual(t.latestDownOffsetMs, 200)
    XCTAssertLessThan(t.latestDownOffsetMs, t.earliestUpOffsetMs)
  }

  func test_success_carriesTimingsWhenTheyWereMeasured() async throws {
    let resp = LongPressRoute.success(
      timings: .init(latestDownOffsetMs: 500, earliestUpOffsetMs: 1200, handlerWallMs: 1500))
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(
      body,
      #"{"ok":true,"latestDownOffsetMs":500,"earliestUpOffsetMs":1200,"handlerWallMs":1500}"#)
  }

  func test_success_withoutTimingsStaysTheOldBody() async throws {
    let body = try await String(decoding: LongPressRoute.success().bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":true}"#)
  }

}

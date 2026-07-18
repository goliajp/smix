import XCTest
import FlyingFox
@testable import SmixRunnerCore

#if canImport(CoreGraphics)
import CoreGraphics
#endif

final class TapRouteTests: XCTestCase {
  // -- decode --

  func test_decode_validTextSelector_returnsRequest() throws {
    let body = Data(#"{"selector":{"text":"General"}}"#.utf8)
    let req = try TapRoute.decode(body)
    XCTAssertEqual(req, TapRoute.TapRequest(selector: .init(text: "General"), mode: .resolveAndTap))
  }

  // The mode field controls whether the runner calls element.tap() or
  // just resolves the element and returns its frame (SDK injects via host-HID).

  func test_decode_modeResolve_returnsRequestWithResolveMode() throws {
    let body = Data(#"{"selector":{"text":"General"},"mode":"resolve"}"#.utf8)
    let req = try TapRoute.decode(body)
    XCTAssertEqual(req.mode, .resolve)
    XCTAssertEqual(req.selector.text, "General")
  }

  func test_decode_modeMissing_defaultsToResolveAndTap() throws {
    // Backward-compat invariant: legacy clients that POST without "mode"
    // still get the original behaviour (resolve + element.tap()).
    let body = Data(#"{"selector":{"text":"General"}}"#.utf8)
    let req = try TapRoute.decode(body)
    XCTAssertEqual(req.mode, .resolveAndTap)
  }

  func test_decode_modeBadString_throwsWrongType() {
    let body = Data(#"{"selector":{"text":"General"},"mode":"badxyz"}"#.utf8)
    XCTAssertThrowsError(try TapRoute.decode(body)) { err in
      guard let e = err as? TapRoute.DecodeError, case .wrongType(let msg) = e else {
        return XCTFail("expected wrongType, got \(err)")
      }
      XCTAssertTrue(msg.contains("mode"), msg)
    }
  }

  func test_decode_emptyBody_throwsInvalidJSON() {
    XCTAssertThrowsError(try TapRoute.decode(Data())) { err in
      XCTAssertEqual(err as? TapRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_malformedJSON_throwsInvalidJSON() {
    let body = Data("{not json".utf8)
    XCTAssertThrowsError(try TapRoute.decode(body)) { err in
      XCTAssertEqual(err as? TapRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_missingSelector_throwsMissingSelector() {
    let body = Data("{}".utf8)
    XCTAssertThrowsError(try TapRoute.decode(body)) { err in
      XCTAssertEqual(err as? TapRoute.DecodeError, .missingSelector)
    }
  }

  func test_decode_selectorWithoutText_throwsMissingText() {
    let body = Data(#"{"selector":{}}"#.utf8)
    XCTAssertThrowsError(try TapRoute.decode(body)) { err in
      XCTAssertEqual(err as? TapRoute.DecodeError, .missingText)
    }
  }

  func test_decode_textIsNumber_throwsWrongType() {
    let body = Data(#"{"selector":{"text":7}}"#.utf8)
    XCTAssertThrowsError(try TapRoute.decode(body)) { err in
      guard let e = err as? TapRoute.DecodeError else { return XCTFail("wrong error type") }
      if case .wrongType = e { /* ok */ } else { XCTFail("expected wrongType, got \(e)") }
    }
  }

  // -- response builders --
  //
  // The success body is the Rust `TapResult` contract: top-level
  // camelCase keys ("matchedLabel", "frame", "appFrame", "stages"). The
  // old emission nested frame/appFrame under "matched" and wrote stages
  // in snake_case — the Rust side deserialized that to all-None/zero
  // without erroring, so these tests assert the exact keys, not just
  // "contains a stages object".

  func test_success_matchedLabel_200TopLevelCamelCase() async throws {
    let resp = TapRoute.success(matchedLabel: "General")
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":true,"matchedLabel":"General"}"#)
  }

  // stages field is opt-in; default nil leaves the body shape unchanged.
  func test_success_includesStagesCamelCase() async throws {
    let resp = TapRoute.success(
      matchedLabel: "About",
      stages: TapRoute.TapStages(resolveMs: 12.5, tapCallMs: 870.0, totalMs: 882.5)
    )
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertTrue(body.contains(#""ok":true"#), body)
    XCTAssertTrue(body.contains(#""matchedLabel":"About""#), body)
    XCTAssertTrue(
      body.contains(#""stages":{"resolveMs":12.5,"tapCallMs":870.0,"totalMs":882.5}"#),
      body
    )
    XCTAssertFalse(body.contains("resolve_ms"), body)
  }

  func test_success_withoutStages_omitsStages() async throws {
    let resp = TapRoute.success(matchedLabel: "About")
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertFalse(body.contains(#""stages""#), body)
  }

  // frame + appFrame are optional top-level fields on the success body,
  // each a full x/y/w/h rect (Rust `Rect` has no field defaults — a
  // w/h-only rect would fail the whole-body parse).

  func test_success_includesFrameAndAppFrameWhenProvided() async throws {
    let resp = TapRoute.success(
      matchedLabel: "General",
      stages: TapRoute.TapStages(resolveMs: 1050.0, tapCallMs: 0.0, totalMs: 1050.0),
      frame: CGRect(x: 20, y: 118.5, width: 353, height: 44),
      appFrame: CGRect(x: 0, y: 0, width: 393, height: 852)
    )
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertTrue(body.contains(#""matchedLabel":"General""#), body)
    XCTAssertTrue(body.contains(#""frame":{"x":20.00,"y":118.50,"w":353.00,"h":44.00}"#), body)
    XCTAssertTrue(body.contains(#""appFrame":{"x":0.00,"y":0.00,"w":393.00,"h":852.00}"#), body)
    XCTAssertTrue(
      body.contains(#""stages":{"resolveMs":1050.0,"tapCallMs":0.0,"totalMs":1050.0}"#),
      body
    )
    XCTAssertFalse(body.contains(#""matched":{"#), body)
  }

  func test_success_omitsFrameAndAppFrameWhenNil() async throws {
    let resp = TapRoute.success(
      matchedLabel: "About",
      stages: TapRoute.TapStages(resolveMs: 12.5, tapCallMs: 870.0, totalMs: 882.5),
      frame: nil,
      appFrame: nil
    )
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertFalse(body.contains(#""frame""#), body)
    XCTAssertFalse(body.contains(#""appFrame""#), body)
    XCTAssertTrue(body.contains(#""matchedLabel":"About""#), body)
  }

  func test_notFound_includesEchoSelectorAndVisibleEmptyArray() async throws {
    let resp = TapRoute.notFound(selector: .init(text: "ZZZ"))
    XCTAssertEqual(resp.statusCode, .notFound)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertTrue(body.contains(#""ok":false"#), body)
    XCTAssertTrue(body.contains(#""error":"not_found""#), body)
    XCTAssertTrue(body.contains(#""selector":{"text":"ZZZ"}"#), body)
    XCTAssertTrue(body.contains(#""visible":[]"#), body)
  }

  func test_badRequest_reasonEchoed() async throws {
    let resp = TapRoute.badRequest(reason: "bad json")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertTrue(body.contains(#""ok":false"#), body)
    XCTAssertTrue(body.contains(#""error":"bad_request""#), body)
    XCTAssertTrue(body.contains(#""reason":"bad json""#), body)
  }
}

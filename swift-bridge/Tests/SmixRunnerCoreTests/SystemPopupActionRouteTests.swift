import XCTest
import FlyingFox
@testable import SmixRunnerCore

// POST /system-popup-action {popupId, buttonId} — the act side of system
// popup handling.
//
// SystemPopupsRoute holds the enumerate side (sense). This route closes the
// act side — dismiss / button-tap by popupId + buttonId. id derivation
// mirrors SystemPopupsRoute enumerate (popup.id ← container.identifier
// fallback "popup-N"; button.id ← b.identifier fallback "b-N") so
// enumerate→action round-trips. UITests dispatch walks the same scan order
// and taps via the EventSynthesizer + daemonProxySynthesize chain, whose
// private symbols are always resolved with dlsym at runtime rather than
// hard-linked.
//
// This file covers Core-layer wire only: request decode and 200/404
// envelope encode.
final class SystemPopupActionRouteTests: XCTestCase {

  // -- decode --

  func test_decode_validBody_returnsRequest() throws {
    let body = Data(#"{"popupId":"p-1","buttonId":"b-open"}"#.utf8)
    let req = try SystemPopupActionRoute.decode(body)
    XCTAssertEqual(req.popupId, "p-1")
    XCTAssertEqual(req.buttonId, "b-open")
  }

  func test_decode_missingPopupId_throwsMissingPopupId() {
    let body = Data(#"{"buttonId":"b-open"}"#.utf8)
    XCTAssertThrowsError(try SystemPopupActionRoute.decode(body)) { err in
      XCTAssertEqual(err as? SystemPopupActionRoute.DecodeError, .missingPopupId)
    }
  }

  func test_decode_missingButtonId_throwsMissingButtonId() {
    let body = Data(#"{"popupId":"p-1"}"#.utf8)
    XCTAssertThrowsError(try SystemPopupActionRoute.decode(body)) { err in
      XCTAssertEqual(err as? SystemPopupActionRoute.DecodeError, .missingButtonId)
    }
  }

  // -- envelope encode --

  func test_success_emitsOkTrueEnvelope() async throws {
    let resp = SystemPopupActionRoute.success()
    XCTAssertEqual(resp.statusCode, .ok)
    let bodyStr = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertTrue(bodyStr.contains(#""ok":true"#), bodyStr)
  }

  func test_notFound_emitsOkFalseNotFoundEnvelopeWithIds() async throws {
    let resp = SystemPopupActionRoute.notFound(popupId: "p-9", buttonId: "b-9")
    XCTAssertEqual(resp.statusCode, .notFound)
    let bodyStr = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertTrue(bodyStr.contains(#""ok":false"#), bodyStr)
    XCTAssertTrue(bodyStr.contains(#""error":"not_found""#), bodyStr)
    XCTAssertTrue(bodyStr.contains(#""popupId":"p-9""#), bodyStr)
    XCTAssertTrue(bodyStr.contains(#""buttonId":"b-9""#), bodyStr)
  }
}

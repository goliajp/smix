import XCTest
import FlyingFox
@testable import SmixRunnerCore

// SwipeAtCoordRoute POCO unit tests — decode + envelope.
//
// The Rust client (smix-runner-client swipe_at_norm_coord) sends
// fromNx/fromNy/toNx/toNy in camelCase and discards the response body
// (serde_json::Value), so the `{ok}` envelope here follows the
// BackRoute / SwipeOnceRoute precedent rather than a typed Rust shape.
final class SwipeAtCoordRouteTests: XCTestCase {
  // -- decode --

  func test_decode_validBody_returnsRequest() throws {
    let body = Data(#"{"fromNx":0.5,"fromNy":0.8,"toNx":0.5,"toNy":0.2}"#.utf8)
    let req = try SwipeAtCoordRoute.decode(body)
    XCTAssertEqual(
      req,
      SwipeAtCoordRoute.SwipeAtCoordRequest(fromNx: 0.5, fromNy: 0.8, toNx: 0.5, toNy: 0.2))
  }

  func test_decode_boundaryZeroAndOne_accepted() throws {
    let body = Data(#"{"fromNx":0,"fromNy":1,"toNx":0,"toNy":1}"#.utf8)
    let req = try SwipeAtCoordRoute.decode(body)
    XCTAssertEqual(
      req, SwipeAtCoordRoute.SwipeAtCoordRequest(fromNx: 0, fromNy: 1, toNx: 0, toNy: 1))
  }

  func test_decode_emptyBody_throwsInvalidJSON() {
    XCTAssertThrowsError(try SwipeAtCoordRoute.decode(Data())) { err in
      XCTAssertEqual(err as? SwipeAtCoordRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_rootNotObject_throwsInvalidJSON() {
    let body = Data(#"[0.5,0.8]"#.utf8)
    XCTAssertThrowsError(try SwipeAtCoordRoute.decode(body)) { err in
      XCTAssertEqual(err as? SwipeAtCoordRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_missingFromNx_throwsMissingField() {
    let body = Data(#"{"fromNy":0.8,"toNx":0.5,"toNy":0.2}"#.utf8)
    XCTAssertThrowsError(try SwipeAtCoordRoute.decode(body)) { err in
      XCTAssertEqual(err as? SwipeAtCoordRoute.DecodeError, .missingField("fromNx"))
    }
  }

  func test_decode_missingToNy_throwsMissingField() {
    let body = Data(#"{"fromNx":0.5,"fromNy":0.8,"toNx":0.5}"#.utf8)
    XCTAssertThrowsError(try SwipeAtCoordRoute.decode(body)) { err in
      XCTAssertEqual(err as? SwipeAtCoordRoute.DecodeError, .missingField("toNy"))
    }
  }

  func test_decode_fromNxIsString_throwsInvalidField() {
    let body = Data(#"{"fromNx":"mid","fromNy":0.8,"toNx":0.5,"toNy":0.2}"#.utf8)
    XCTAssertThrowsError(try SwipeAtCoordRoute.decode(body)) { err in
      XCTAssertEqual(err as? SwipeAtCoordRoute.DecodeError, .invalidField("fromNx", "mid"))
    }
  }

  func test_decode_fromNxAboveOne_throwsOutOfRange() {
    let body = Data(#"{"fromNx":1.5,"fromNy":0.8,"toNx":0.5,"toNy":0.2}"#.utf8)
    XCTAssertThrowsError(try SwipeAtCoordRoute.decode(body)) { err in
      XCTAssertEqual(err as? SwipeAtCoordRoute.DecodeError, .outOfRange("fromNx", 1.5))
    }
  }

  func test_decode_toNyNegative_throwsOutOfRange() {
    let body = Data(#"{"fromNx":0.5,"fromNy":0.8,"toNx":0.5,"toNy":-0.25}"#.utf8)
    XCTAssertThrowsError(try SwipeAtCoordRoute.decode(body)) { err in
      XCTAssertEqual(err as? SwipeAtCoordRoute.DecodeError, .outOfRange("toNy", -0.25))
    }
  }

  // -- response builders --

  func test_success_okTrue_200ExactBody() async throws {
    let resp = SwipeAtCoordRoute.success(ok: true)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":true}"#)
  }

  func test_success_okFalse_200ExactBody() async throws {
    let resp = SwipeAtCoordRoute.success(ok: false)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false}"#)
  }

  func test_badRequest_400ExactBody() async throws {
    let resp = SwipeAtCoordRoute.badRequest(reason: "bad json")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false,"error":"bad_request","reason":"bad json"}"#)
  }
}

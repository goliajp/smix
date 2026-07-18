import XCTest
import FlyingFox
@testable import SmixRunnerCore

// SetOrientationRoute POCO unit tests — decode + envelope.
//
// The four orientation literals are the wire contract with the Rust
// client (smix-runner-client set_orientation / Orientation::as_wire);
// the response body is discarded on the Rust side, so the `{ok}`
// envelope follows the BackRoute precedent.
final class SetOrientationRouteTests: XCTestCase {
  // -- decode --

  func test_decode_allFourValidOrientations() throws {
    for o in ["portrait", "portraitUpsideDown", "landscapeLeft", "landscapeRight"] {
      let body = Data(#"{"orientation":"\#(o)"}"#.utf8)
      let req = try SetOrientationRoute.decode(body)
      XCTAssertEqual(req, SetOrientationRoute.SetOrientationRequest(orientation: o))
    }
  }

  func test_validOrientations_tableMatchesWireLiterals() {
    XCTAssertEqual(
      SetOrientationRoute.validOrientations,
      ["portrait", "portraitUpsideDown", "landscapeLeft", "landscapeRight"])
  }

  func test_decode_emptyBody_throwsInvalidJSON() {
    XCTAssertThrowsError(try SetOrientationRoute.decode(Data())) { err in
      XCTAssertEqual(err as? SetOrientationRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_malformedJSON_throwsInvalidJSON() {
    let body = Data("{not json".utf8)
    XCTAssertThrowsError(try SetOrientationRoute.decode(body)) { err in
      XCTAssertEqual(err as? SetOrientationRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_rootNotObject_throwsWrongType() {
    let body = Data(#"["portrait"]"#.utf8)
    XCTAssertThrowsError(try SetOrientationRoute.decode(body)) { err in
      XCTAssertEqual(err as? SetOrientationRoute.DecodeError, .wrongType("root not object"))
    }
  }

  func test_decode_missingOrientation_throwsMissingOrientation() {
    let body = Data("{}".utf8)
    XCTAssertThrowsError(try SetOrientationRoute.decode(body)) { err in
      XCTAssertEqual(err as? SetOrientationRoute.DecodeError, .missingOrientation)
    }
  }

  func test_decode_orientationIsNumber_throwsWrongType() {
    let body = Data(#"{"orientation":90}"#.utf8)
    XCTAssertThrowsError(try SetOrientationRoute.decode(body)) { err in
      XCTAssertEqual(err as? SetOrientationRoute.DecodeError, .wrongType("orientation not string"))
    }
  }

  func test_decode_unknownOrientation_throwsUnknownOrientation() {
    let body = Data(#"{"orientation":"upsideDown"}"#.utf8)
    XCTAssertThrowsError(try SetOrientationRoute.decode(body)) { err in
      XCTAssertEqual(
        err as? SetOrientationRoute.DecodeError, .unknownOrientation("upsideDown"))
    }
  }

  // -- response builders --

  func test_success_okTrue_200ExactBody() async throws {
    let resp = SetOrientationRoute.success(ok: true)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":true}"#)
  }

  func test_success_okFalse_200ExactBody() async throws {
    let resp = SetOrientationRoute.success(ok: false)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false}"#)
  }

  func test_badRequest_400ExactBody() async throws {
    let resp = SetOrientationRoute.badRequest(reason: "unknownOrientation(\"diag\")")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(
      body, #"{"ok":false,"error":"bad_request","reason":"unknownOrientation(\"diag\")"}"#)
  }
}

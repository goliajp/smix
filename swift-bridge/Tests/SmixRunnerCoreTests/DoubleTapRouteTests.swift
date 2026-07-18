import XCTest
import FlyingFox
@testable import SmixRunnerCore

// DoubleTapRoute POCO unit tests — decode + badRequest envelope.
//
// success(ok:) is deliberately NOT asserted here: the Rust client
// (smix-runner-client double_tap) deserializes the /double-tap response
// as `TapResult`, whose fields are all optional-with-default — the
// `{"ok":bool}` body this route emits parses to an all-None TapResult,
// so the `ok:false` failure signal is silently dropped on the Rust
// side. Same silently-dropped-shape class as the fixed /tap bug;
// codifying the emission here would enshrine the wrong contract.
final class DoubleTapRouteTests: XCTestCase {
  // -- decode --

  func test_decode_validBody_returnsRequest() throws {
    let body = Data(#"{"selector":{"text":"General"}}"#.utf8)
    let req = try DoubleTapRoute.decode(body)
    XCTAssertEqual(req, DoubleTapRoute.DoubleTapRequest(selectorText: "General"))
  }

  func test_decode_emptyBody_throwsInvalidJSON() {
    XCTAssertThrowsError(try DoubleTapRoute.decode(Data())) { err in
      XCTAssertEqual(err as? DoubleTapRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_malformedJSON_throwsInvalidJSON() {
    let body = Data("{not json".utf8)
    XCTAssertThrowsError(try DoubleTapRoute.decode(body)) { err in
      XCTAssertEqual(err as? DoubleTapRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_rootNotObject_throwsWrongType() {
    let body = Data(#"[1,2]"#.utf8)
    XCTAssertThrowsError(try DoubleTapRoute.decode(body)) { err in
      XCTAssertEqual(err as? DoubleTapRoute.DecodeError, .wrongType("root not object"))
    }
  }

  func test_decode_missingSelector_throwsMissingSelector() {
    let body = Data("{}".utf8)
    XCTAssertThrowsError(try DoubleTapRoute.decode(body)) { err in
      XCTAssertEqual(err as? DoubleTapRoute.DecodeError, .missingSelector)
    }
  }

  func test_decode_selectorNotObject_throwsWrongType() {
    let body = Data(#"{"selector":"General"}"#.utf8)
    XCTAssertThrowsError(try DoubleTapRoute.decode(body)) { err in
      XCTAssertEqual(err as? DoubleTapRoute.DecodeError, .wrongType("selector not object"))
    }
  }

  func test_decode_selectorWithoutText_throwsMissingText() {
    let body = Data(#"{"selector":{}}"#.utf8)
    XCTAssertThrowsError(try DoubleTapRoute.decode(body)) { err in
      XCTAssertEqual(err as? DoubleTapRoute.DecodeError, .missingText)
    }
  }

  func test_decode_textIsNumber_throwsWrongType() {
    let body = Data(#"{"selector":{"text":7}}"#.utf8)
    XCTAssertThrowsError(try DoubleTapRoute.decode(body)) { err in
      XCTAssertEqual(err as? DoubleTapRoute.DecodeError, .wrongType("selector.text not string"))
    }
  }

  // -- response builders --

  func test_badRequest_400ExactBody() async throws {
    let resp = DoubleTapRoute.badRequest(reason: "bad json")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false,"error":"bad_request","reason":"bad json"}"#)
  }

  func test_badRequest_escapesQuoteAndNewline() async throws {
    let resp = DoubleTapRoute.badRequest(reason: "he\"llo\nx")
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false,"error":"bad_request","reason":"he\"llo\nx"}"#)
  }
}

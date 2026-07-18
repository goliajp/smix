import XCTest
import FlyingFox
@testable import SmixRunnerCore

// FindTextByOcrRoute POCO unit tests — decode + envelope. No Vision /
// XCUIScreen here; the OCR pass itself lives in the handler.
//
// Wire contract (matches smix-runner-client find_text_by_ocr): request
// carries snake_case `recognition_level`; response is
// `{"found":bool,"frame":[nx,ny,w,h]|null}`.
final class FindTextByOcrRouteTests: XCTestCase {
  // -- decode --

  func test_decode_minimalBody_defaultsLocalesAndLevel() throws {
    let body = Data(#"{"text":"Submit"}"#.utf8)
    let req = try FindTextByOcrRoute.decode(body)
    XCTAssertEqual(
      req,
      FindTextByOcrRoute.FindTextByOcrRequest(
        text: "Submit", locales: ["en"], recognitionLevel: "accurate"))
  }

  func test_decode_fullBody_returnsRequest() throws {
    let body = Data(#"{"text":"送信","locales":["ja","en"],"recognition_level":"fast"}"#.utf8)
    let req = try FindTextByOcrRoute.decode(body)
    XCTAssertEqual(
      req,
      FindTextByOcrRoute.FindTextByOcrRequest(
        text: "送信", locales: ["ja", "en"], recognitionLevel: "fast"))
  }

  func test_decode_emptyLocalesArray_fallsBackToEn() throws {
    let body = Data(#"{"text":"Submit","locales":[]}"#.utf8)
    let req = try FindTextByOcrRoute.decode(body)
    XCTAssertEqual(req.locales, ["en"])
  }

  func test_decode_emptyBody_throwsInvalidJSON() {
    XCTAssertThrowsError(try FindTextByOcrRoute.decode(Data())) { err in
      XCTAssertEqual(err as? FindTextByOcrRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_rootNotObject_throwsInvalidJSON() {
    let body = Data(#"["Submit"]"#.utf8)
    XCTAssertThrowsError(try FindTextByOcrRoute.decode(body)) { err in
      XCTAssertEqual(err as? FindTextByOcrRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_missingText_throwsMissingText() {
    let body = Data(#"{"locales":["en"]}"#.utf8)
    XCTAssertThrowsError(try FindTextByOcrRoute.decode(body)) { err in
      XCTAssertEqual(err as? FindTextByOcrRoute.DecodeError, .missingText)
    }
  }

  func test_decode_textIsNumber_throwsInvalidField() {
    let body = Data(#"{"text":7}"#.utf8)
    XCTAssertThrowsError(try FindTextByOcrRoute.decode(body)) { err in
      XCTAssertEqual(err as? FindTextByOcrRoute.DecodeError, .invalidField("text", "7"))
    }
  }

  func test_decode_emptyText_throwsInvalidField() {
    let body = Data(#"{"text":""}"#.utf8)
    XCTAssertThrowsError(try FindTextByOcrRoute.decode(body)) { err in
      XCTAssertEqual(err as? FindTextByOcrRoute.DecodeError, .invalidField("text", "empty"))
    }
  }

  func test_decode_localesNotArray_throwsInvalidField() {
    let body = Data(#"{"text":"Submit","locales":"en"}"#.utf8)
    XCTAssertThrowsError(try FindTextByOcrRoute.decode(body)) { err in
      XCTAssertEqual(err as? FindTextByOcrRoute.DecodeError, .invalidField("locales", "en"))
    }
  }

  func test_decode_recognitionLevelNotString_throwsInvalidField() {
    let body = Data(#"{"text":"Submit","recognition_level":2}"#.utf8)
    XCTAssertThrowsError(try FindTextByOcrRoute.decode(body)) { err in
      XCTAssertEqual(
        err as? FindTextByOcrRoute.DecodeError, .invalidField("recognition_level", "2"))
    }
  }

  func test_decode_recognitionLevelUnknownLiteral_throwsInvalidField() {
    let body = Data(#"{"text":"Submit","recognition_level":"medium"}"#.utf8)
    XCTAssertThrowsError(try FindTextByOcrRoute.decode(body)) { err in
      XCTAssertEqual(
        err as? FindTextByOcrRoute.DecodeError, .invalidField("recognition_level", "medium"))
    }
  }

  // -- response builders --

  func test_success_foundWithFrame_200ExactBody() async throws {
    // Exactly-representable doubles so string interpolation is stable.
    let resp = FindTextByOcrRoute.success(found: true, frame: (0.25, 0.5, 0.125, 0.0625))
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"found":true,"frame":[0.25,0.5,0.125,0.0625]}"#)
  }

  func test_success_notFound_frameNull_200ExactBody() async throws {
    let resp = FindTextByOcrRoute.success(found: false, frame: nil)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"found":false,"frame":null}"#)
  }

  func test_badRequest_400ExactBody() async throws {
    let resp = FindTextByOcrRoute.badRequest(reason: #"invalidField("text", "empty")"#)
    XCTAssertEqual(resp.statusCode, .badRequest)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(
      body,
      #"{"found":false,"error":"bad_request","reason":"invalidField(\"text\", \"empty\")"}"#)
  }
}

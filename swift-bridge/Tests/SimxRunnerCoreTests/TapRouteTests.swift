import XCTest
import FlyingFox
@testable import SimxRunnerCore

final class TapRouteTests: XCTestCase {
  // -- decode --

  func test_decode_validTextSelector_returnsRequest() throws {
    let body = Data(#"{"selector":{"text":"General"}}"#.utf8)
    let req = try TapRoute.decode(body)
    XCTAssertEqual(req, TapRoute.TapRequest(selector: .init(text: "General")))
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

  func test_success_matchedLabel_200WithMatchedField() async throws {
    let resp = TapRoute.success(matchedLabel: "General")
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertTrue(body.contains(#""ok":true"#), body)
    XCTAssertTrue(body.contains(#""matched":{"label":"General"}"#), body)
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

import XCTest
import FlyingFox
@testable import SmixRunnerCore

// InputTextRoute POCO unit tests. Route owns decode + envelope
// serialization only (no XCUITest).
//
// Wire contract cross-checked against both existing consumers/producers:
// - Rust client (crates/smix-runner-client HttpRunnerClient::input_text)
//   POSTs `{"text": <string>}` and parses a 200 OkEnvelope where absent
//   `ok` passes and `ok:false` is a refusal.
// - Kotlin runner (RunnerWire.decodeInputText) does
//   `JSONObject(payload).getString("text")` — "text" key required, must
//   be a string; extra fields ignored.
// The iOS decode must accept exactly the same body shape.
final class InputTextRouteTests: XCTestCase {

  private func parse(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  // -- decode --

  func test_decode_validBody_returnsText() throws {
    let body = Data(#"{"text":"hello world"}"#.utf8)
    let req = try InputTextRoute.decode(body)
    XCTAssertEqual(req, InputTextRoute.InputTextRequest(text: "hello world"))
  }

  func test_decode_emptyStringText_ok() throws {
    // Kotlin getString("text") accepts "" — so do we.
    let body = Data(#"{"text":""}"#.utf8)
    let req = try InputTextRoute.decode(body)
    XCTAssertEqual(req.text, "")
  }

  func test_decode_unicodeText_ok() throws {
    let body = Data(#"{"text":"héllo — 世界 🚀"}"#.utf8)
    let req = try InputTextRoute.decode(body)
    XCTAssertEqual(req.text, "héllo — 世界 🚀")
  }

  func test_decode_escapedCharactersInText_ok() throws {
    let body = Data(#"{"text":"line1\nline2\t\"quoted\""}"#.utf8)
    let req = try InputTextRoute.decode(body)
    XCTAssertEqual(req.text, "line1\nline2\t\"quoted\"")
  }

  func test_decode_extraFields_ignored() throws {
    // Additive payload fields are forward-compatible (Kotlin
    // getString ignores siblings too).
    let body = Data(#"{"text":"abc","future":123}"#.utf8)
    let req = try InputTextRoute.decode(body)
    XCTAssertEqual(req.text, "abc")
  }

  func test_decode_emptyBody_throwsInvalidJSON() {
    XCTAssertThrowsError(try InputTextRoute.decode(Data())) { err in
      XCTAssertEqual(err as? InputTextRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_malformedJSON_throwsInvalidJSON() {
    let body = Data("{not json".utf8)
    XCTAssertThrowsError(try InputTextRoute.decode(body)) { err in
      XCTAssertEqual(err as? InputTextRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_rootNotObject_throwsWrongType() {
    let body = Data(#"["text"]"#.utf8)
    XCTAssertThrowsError(try InputTextRoute.decode(body)) { err in
      guard let e = err as? InputTextRoute.DecodeError, case .wrongType = e else {
        return XCTFail("expected wrongType, got \(err)")
      }
    }
  }

  func test_decode_missingText_throwsMissingText() {
    let body = Data("{}".utf8)
    XCTAssertThrowsError(try InputTextRoute.decode(body)) { err in
      XCTAssertEqual(err as? InputTextRoute.DecodeError, .missingText)
    }
  }

  func test_decode_textIsNumber_throwsWrongType() {
    let body = Data(#"{"text":7}"#.utf8)
    XCTAssertThrowsError(try InputTextRoute.decode(body)) { err in
      guard let e = err as? InputTextRoute.DecodeError, case .wrongType(let msg) = e else {
        return XCTFail("expected wrongType, got \(err)")
      }
      XCTAssertTrue(msg.contains("text"), msg)
    }
  }

  // -- response envelopes --

  func test_success_okTrue_serializes() async throws {
    let resp = InputTextRoute.success(ok: true)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
    XCTAssertEqual(json.count, 1)
  }

  func test_success_okFalse_serializes() async throws {
    // ok:false on a 200 is what the Rust OkEnvelope.require_ok treats
    // as a refusal — daemon send failure surfaces here.
    let resp = InputTextRoute.success(ok: false)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, false)
  }

  func test_badRequest_serializes() async throws {
    let resp = InputTextRoute.badRequest(reason: "missingText")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, false)
    XCTAssertEqual(json["error"] as? String, "bad_request")
    XCTAssertEqual(json["reason"] as? String, "missingText")
  }

  func test_badRequest_reasonEscaped() async throws {
    let resp = InputTextRoute.badRequest(reason: "bad \"quote\"\nline")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let json = try await parse(resp)
    XCTAssertEqual(json["reason"] as? String, "bad \"quote\"\nline")
  }
}

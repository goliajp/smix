import XCTest
import FlyingFox
@testable import SmixRunnerCore

// BackRoute POCO unit tests. Mirrors ForegroundRouteTests / ScrollRouteTests.
// No XCUITest here — BackRoute only owns decode + envelope.
//
// case I: decode happy path — empty body allowed (back takes no fields, so no body is required)
// case J: decode happy path — a `{}` JSON object is also allowed
// case K: decode a non-JSON string → DecodeError.invalidJSON
// case L: success(ok:true) → 200 + body {"ok":true}
// case M: success(ok:false) → 200 + body {"ok":false}
final class BackRouteTests: XCTestCase {

  private func parse(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  // case I: empty body — back is parameterless
  func test_decode_empty_body_ok() throws {
    let body = Data()
    let req = try BackRoute.decode(body)
    XCTAssertEqual(req, BackRoute.BackRequest())
  }

  // case J: empty JSON object `{}`
  func test_decode_empty_object_ok() throws {
    let body = Data(#"{}"#.utf8)
    let req = try BackRoute.decode(body)
    XCTAssertEqual(req, BackRoute.BackRequest())
  }

  // case K: not-JSON → DecodeError.invalidJSON
  func test_decode_non_json_throws() {
    let body = Data("not-json".utf8)
    XCTAssertThrowsError(try BackRoute.decode(body)) { error in
      XCTAssertEqual(
        error as? BackRoute.DecodeError,
        BackRoute.DecodeError.invalidJSON
      )
    }
  }

  // case L: success(ok:true) → 200 body {"ok":true}
  func test_success_ok_true_serializes() async throws {
    let resp = BackRoute.success(ok: true)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
  }

  // case M: success(ok:false) → 200 body {"ok":false}
  func test_success_ok_false_serializes() async throws {
    let resp = BackRoute.success(ok: false)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, false)
  }

  // A refusal has to carry what it saw.
  //
  // `gaveUp` says the budget ran out and nothing was observed, and that
  // is the whole of it — the same word covers "the back button was not
  // there to tap", "it was tapped and the title never moved" and "the
  // edge gesture did nothing". On CI this refusal has come back on
  // `portable-nav-detail-and-back` since 2026-08-19, four runs, and no
  // change could be aimed at it because the word names no branch. The
  // route's own doc says the next change attacks the branch the data
  // names; this is that field.
  func test_a_refusal_carries_what_it_saw() async throws {
    let resp = BackRoute.success(
      ok: false,
      settledBy: .gaveUp,
      saw: "button=yes before=Detail last=Detail absences=0"
    )
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, false)
    XCTAssertEqual(json["settledBy"] as? String, "gaveUp")
    XCTAssertEqual(
      json["saw"] as? String,
      "button=yes before=Detail last=Detail absences=0"
    )
  }

  // And an answer with nothing to report must not invent the field —
  // an empty `saw` on every success would train the reader to skip it.
  func test_an_answer_with_nothing_to_report_omits_the_field() async throws {
    let resp = BackRoute.success(ok: true, settledBy: .titleChanged)
    let json = try await parse(resp)
    XCTAssertNil(json["saw"])
  }
}

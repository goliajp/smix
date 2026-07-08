import XCTest
import FlyingFox
@testable import SmixRunnerCore

// v1.5 c5g'' S2 — HideKeyboardRoute POCO unit tests. Mirrors BackRouteTests
// (parameterless app-level capability). Does not exercise XCUITest — route
// owns only decode + envelope serialization.
//
// case I: decode empty body → request OK (hide-keyboard parameterless)
// case J: decode `{}` empty JSON object body → request OK
// case K: decode non-JSON body → DecodeError.invalidJSON
final class HideKeyboardRouteTests: XCTestCase {

  private func parse(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  // case I: empty body — hide-keyboard is parameterless
  func test_decode_empty_body_ok() throws {
    let body = Data()
    let req = try HideKeyboardRoute.decode(body)
    XCTAssertEqual(req, HideKeyboardRoute.HideKeyboardRequest())
  }

  // case J: empty JSON object `{}` body
  func test_decode_empty_object_ok() throws {
    let body = Data(#"{}"#.utf8)
    let req = try HideKeyboardRoute.decode(body)
    XCTAssertEqual(req, HideKeyboardRoute.HideKeyboardRequest())
  }

  // case K: not-JSON → DecodeError.invalidJSON
  func test_decode_non_json_throws() {
    let body = Data("not-json".utf8)
    XCTAssertThrowsError(try HideKeyboardRoute.decode(body)) { error in
      XCTAssertEqual(
        error as? HideKeyboardRoute.DecodeError,
        HideKeyboardRoute.DecodeError.invalidJSON
      )
    }
  }

  // bonus: success(ok:true) → 200 {"ok":true}
  func test_success_ok_true_serializes() async throws {
    let resp = HideKeyboardRoute.success(ok: true)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
  }
}

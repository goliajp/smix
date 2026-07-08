import XCTest
import FlyingFox
@testable import SmixRunnerCore

// v1.5 C5d' S1 — BackRoute POCO 单测. 形态镜像 ForegroundRouteTests / ScrollRouteTests.
// 不跑 XCUITest (BackRoute 只持 decode + envelope 职责).
//
// case I: decode happy path — empty body 允许 (back 0 字段, 不需 body)
// case J: decode happy path — `{}` JSON object 也允许
// case K: decode 非 JSON 字符串 → DecodeError.invalidJSON
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
}

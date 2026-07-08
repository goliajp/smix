// v7.8 c1 — SelectResolveRoute Request/Response wire shape tests.
//
// Mirror npm/smix-rn/src/HttpRunner.ts contract:
//   POST /select/resolve         { treeJson, selectorJson } → { ids: [String] }
//   POST /select/resolve-count   { treeJson, selectorJson } → { count: u32 }
//   POST /select/resolve-labels  { treeJson, selectorJson } → { labels: [String] }

import FlyingFox
import Foundation
import XCTest
@testable import SmixRunnerCore

final class SelectResolveRouteTests: XCTestCase {

  // MARK: - Request decode

  func test_decode_validRequest_succeeds() throws {
    let body = #"{"treeJson":"{\"rawType\":\"other\"}","selectorJson":"{\"id\":\"btn-login\"}"}"#.data(using: .utf8)!
    let req = try SelectResolveRoute.decode(body)
    XCTAssertEqual(req.treeJson, #"{"rawType":"other"}"#)
    XCTAssertEqual(req.selectorJson, #"{"id":"btn-login"}"#)
  }

  func test_decode_invalidJson_throwsDecodeError() {
    let body = "not json".data(using: .utf8)!
    XCTAssertThrowsError(try SelectResolveRoute.decode(body)) { error in
      guard let e = error as? SelectResolveRoute.DecodeError else {
        XCTFail("expected DecodeError, got \(error)")
        return
      }
      XCTAssertTrue(e.reason.contains("invalid JSON"))
    }
  }

  func test_decode_missingTreeJson_throwsDecodeError() {
    let body = #"{"selectorJson":"{}"}"#.data(using: .utf8)!
    XCTAssertThrowsError(try SelectResolveRoute.decode(body)) { error in
      let e = error as? SelectResolveRoute.DecodeError
      XCTAssertNotNil(e)
      XCTAssertTrue(e?.reason.contains("treeJson") ?? false)
    }
  }

  func test_decode_missingSelectorJson_throwsDecodeError() {
    let body = #"{"treeJson":"{}"}"#.data(using: .utf8)!
    XCTAssertThrowsError(try SelectResolveRoute.decode(body)) { error in
      let e = error as? SelectResolveRoute.DecodeError
      XCTAssertNotNil(e)
      XCTAssertTrue(e?.reason.contains("selectorJson") ?? false)
    }
  }

  func test_decode_rootArrayNotObject_throwsDecodeError() {
    let body = "[]".data(using: .utf8)!
    XCTAssertThrowsError(try SelectResolveRoute.decode(body)) { error in
      let e = error as? SelectResolveRoute.DecodeError
      XCTAssertNotNil(e)
    }
  }

  // MARK: - Response builders

  func test_successIds_emitsExpectedShape() async throws {
    let resp = SelectResolveRoute.successIds(["btn-a", "btn-b"])
    XCTAssertEqual(resp.statusCode, .ok)
    XCTAssertEqual(resp.headers[.contentType], "application/json")
    let body = String(data: try await bodyData(resp), encoding: .utf8) ?? ""
    XCTAssertEqual(body, #"{"ids":["btn-a","btn-b"]}"#)
  }

  func test_successIds_emptyArrayShape() async throws {
    let resp = SelectResolveRoute.successIds([])
    let body = String(data: try await bodyData(resp), encoding: .utf8) ?? ""
    XCTAssertEqual(body, #"{"ids":[]}"#)
  }

  func test_successCount_emitsCountField() async throws {
    let resp = SelectResolveRoute.successCount(42)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = String(data: try await bodyData(resp), encoding: .utf8) ?? ""
    XCTAssertEqual(body, #"{"count":42}"#)
  }

  func test_successCount_zero() async throws {
    let resp = SelectResolveRoute.successCount(0)
    let body = String(data: try await bodyData(resp), encoding: .utf8) ?? ""
    XCTAssertEqual(body, #"{"count":0}"#)
  }

  func test_successLabels_emitsLabelsField() async throws {
    let resp = SelectResolveRoute.successLabels(["First", "Second"])
    XCTAssertEqual(resp.statusCode, .ok)
    let body = String(data: try await bodyData(resp), encoding: .utf8) ?? ""
    XCTAssertEqual(body, #"{"labels":["First","Second"]}"#)
  }

  func test_badRequest_emitsErrorEnvelope() async throws {
    let resp = SelectResolveRoute.badRequest(reason: "missing field 'treeJson'")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let body = String(data: try await bodyData(resp), encoding: .utf8) ?? ""
    XCTAssertTrue(body.contains(#""error""#))
    XCTAssertTrue(body.contains("missing field"))
  }

  func test_ffiError_emits500() async throws {
    let resp = SelectResolveRoute.ffiError(reason: "invalid tree JSON")
    XCTAssertEqual(resp.statusCode, .internalServerError)
    let body = String(data: try await bodyData(resp), encoding: .utf8) ?? ""
    XCTAssertTrue(body.contains(#""error""#))
  }

  // MARK: - Helpers

  /// FlyingFox HTTPResponse exposes body via async/throws property; bridge
  /// to sync test code via a helper. (Tests are sync; we use try await
  /// because XCTestCase allows it via `async throws`.)
  private func bodyData(_ resp: HTTPResponse) async throws -> Data {
    return try await resp.bodyData
  }
}

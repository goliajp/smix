import XCTest
import FlyingFox
@testable import SmixRunnerCore

final class ForegroundRouteTests: XCTestCase {

  private func parse(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  // case I: decode happy path — {bundleId:"com.example.app"}
  func test_decode_happy_bundleId() throws {
    let body = Data(#"{"bundleId":"com.example.app"}"#.utf8)
    let req = try ForegroundRoute.decode(body)
    XCTAssertEqual(req.bundleId, "com.example.app")
  }

  // case J: decode missing bundleId → DecodeError.missingBundleId
  func test_decode_missingBundleId_throws() {
    let body = Data(#"{}"#.utf8)
    XCTAssertThrowsError(try ForegroundRoute.decode(body)) { error in
      XCTAssertEqual(
        error as? ForegroundRoute.DecodeError,
        ForegroundRoute.DecodeError.missingBundleId
      )
    }
  }

  // case K: decode empty bundleId string "" → DecodeError.emptyBundleId
  func test_decode_emptyBundleId_throws() {
    let body = Data(#"{"bundleId":""}"#.utf8)
    XCTAssertThrowsError(try ForegroundRoute.decode(body)) { error in
      XCTAssertEqual(
        error as? ForegroundRoute.DecodeError,
        ForegroundRoute.DecodeError.emptyBundleId
      )
    }
  }

  // case L: decode bundleId 非 String → DecodeError.wrongType
  func test_decode_wrongTypeBundleId_throws() {
    let body = Data(#"{"bundleId":123}"#.utf8)
    XCTAssertThrowsError(try ForegroundRoute.decode(body)) { error in
      guard let e = error as? ForegroundRoute.DecodeError else {
        XCTFail("expected DecodeError, got \(error)")
        return
      }
      switch e {
      case .wrongType: break
      default: XCTFail("expected .wrongType, got \(e)")
      }
    }
  }

  // case M: success(ok:true) → HTTPResponse 200 body {"ok":true}
  func test_success_ok_true_serializes() async throws {
    let resp = ForegroundRoute.success(ok: true)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
  }
}

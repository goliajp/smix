import XCTest
import FlyingFox
@testable import SmixRunnerCore

final class FindRouteTests: XCTestCase {

  private func parse(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  func test_decode_validRequest_returnsFindRequest() throws {
    let body = Data(#"{"selector":{"text":"General"}}"#.utf8)
    let req = try FindRoute.decode(body)
    XCTAssertEqual(req.selector, FindRoute.Selector(text: "General"))
  }

  func test_decode_missingSelector_throws() {
    let body = Data("{}".utf8)
    XCTAssertThrowsError(try FindRoute.decode(body)) { error in
      XCTAssertEqual(error as? FindRoute.DecodeError, FindRoute.DecodeError.missingSelector)
    }
  }

  func test_decode_missingText_throws() {
    let body = Data(#"{"selector":{}}"#.utf8)
    XCTAssertThrowsError(try FindRoute.decode(body)) { error in
      XCTAssertEqual(error as? FindRoute.DecodeError, FindRoute.DecodeError.missingText)
    }
  }

  func test_decode_invalidJSON_throws() {
    let body = Data("not json".utf8)
    XCTAssertThrowsError(try FindRoute.decode(body)) { error in
      XCTAssertEqual(error as? FindRoute.DecodeError, FindRoute.DecodeError.invalidJSON)
    }
  }

  func test_success_found_true_serializes() async throws {
    let resp = FindRoute.success(found: true)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
    XCTAssertEqual(json["found"] as? Bool, true)
  }

  func test_success_found_false_serializes() async throws {
    let resp = FindRoute.success(found: false)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
    XCTAssertEqual(json["found"] as? Bool, false)
  }
}

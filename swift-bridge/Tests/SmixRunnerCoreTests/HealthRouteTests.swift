import XCTest
import FlyingFox
@testable import SmixRunnerCore

final class HealthRouteTests: XCTestCase {
  func test_body_isExactJSONOK() throws {
    let data = HealthRoute.body()
    XCTAssertEqual(String(data: data, encoding: .utf8), #"{"ok":true}"#)
  }

  func test_body_parsesAsJSONWithOkTrue() throws {
    let data = HealthRoute.body()
    let obj = try JSONSerialization.jsonObject(with: data, options: []) as? [String: Any]
    XCTAssertNotNil(obj)
    XCTAssertEqual(obj?["ok"] as? Bool, true)
  }

  func test_response_status200_contentTypeJSON() {
    let resp = HealthRoute.response()
    XCTAssertEqual(resp.statusCode, .ok)
    XCTAssertEqual(resp.headers[.contentType], "application/json")
  }
}

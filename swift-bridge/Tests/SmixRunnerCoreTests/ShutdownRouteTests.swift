import XCTest
import FlyingFox
@testable import SmixRunnerCore

final class ShutdownRouteTests: XCTestCase {
  func test_body_isExactJSONOK() throws {
    let data = ShutdownRoute.body()
    XCTAssertEqual(String(data: data, encoding: .utf8), #"{"ok":true,"op":"shutdown"}"#)
  }

  func test_body_parsesAsJSONWithOkTrue() throws {
    let data = ShutdownRoute.body()
    let obj = try JSONSerialization.jsonObject(with: data, options: []) as? [String: Any]
    XCTAssertNotNil(obj)
    XCTAssertEqual(obj?["ok"] as? Bool, true)
    XCTAssertEqual(obj?["op"] as? String, "shutdown")
  }

  func test_response_status200_contentTypeJSON() {
    let resp = ShutdownRoute.response()
    XCTAssertEqual(resp.statusCode, .ok)
    XCTAssertEqual(resp.headers[.contentType], "application/json")
  }
}

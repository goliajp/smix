import XCTest
import FlyingFox
@testable import SmixRunnerCore

/// fill/clear/pressKey timing emitted to SDK side via response
/// body `stages` field (mirrors TapRoute.TapStages wire convention).
final class KeyboardRouteStagesTest: XCTestCase {

  private func parseBody(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  func test_successWithStages_serializesOkAndStages() async throws {
    let resp = KeyboardRoute.successWithStages(focusMs: 12, daemonSendMs: 34)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parseBody(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
    let stages = json["stages"] as? [String: Any]
    XCTAssertNotNil(stages)
    XCTAssertEqual(stages?["focus_ms"] as? Int, 12)
    XCTAssertEqual(stages?["daemon_send_ms"] as? Int, 34)
  }

  func test_success_backwardCompatible_noStages() async throws {
    let resp = KeyboardRoute.success()
    let json = try await parseBody(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
    XCTAssertNil(json["stages"], "legacy success() must not emit stages field")
  }
}

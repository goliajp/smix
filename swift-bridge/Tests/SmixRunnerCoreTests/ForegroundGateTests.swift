import XCTest
@testable import SmixRunnerCore

final class ForegroundGateTests: XCTestCase {

  func test_queries_on_the_app_s_elements_are_held() {
    for route in ["GET /tree", "GET /coordinate-space", "POST /find", "POST /tap", "POST /tap-by-id", "POST /fill",
                  "POST /clear", "POST /hittable"] {
      XCTAssertTrue(ForegroundGate.holds(route: route, include: nil, activate: false), route)
    }
  }

  func test_the_system_windows_an_activation_and_screen_gestures_pass() {
    XCTAssertFalse(ForegroundGate.holds(route: "POST /tap", include: "all-windows", activate: false))
    XCTAssertFalse(ForegroundGate.holds(route: "GET /tree", include: "all-windows", activate: false))
    XCTAssertFalse(ForegroundGate.holds(route: "POST /tap", include: nil, activate: true))
    for route in ["GET /system-popups", "POST /tap-at-norm-coord", "POST /input-text",
                  "POST /find-text-by-ocr", "POST /back", "POST /session/open"] {
      XCTAssertFalse(ForegroundGate.holds(route: route, include: nil, activate: false), route)
    }
  }

  func test_the_refusal_is_the_unavailable_envelope_naming_the_app_in_front() async throws {
    let seen = AppNotInFront(
      bundleId: "com.example.shop", state: "running-background",
      inFront: ["com.apple.Preferences"])
    let resp = ForegroundGate.refusal(seen)
    XCTAssertEqual(resp.statusCode, .internalServerError)
    let body = String(decoding: try await resp.bodyData, as: UTF8.self)
    XCTAssertTrue(body.contains(#""error":"snapshot_unavailable""#), body)
    XCTAssertTrue(body.contains(#""reason":"in-background""#), body)
    XCTAssertTrue(body.contains("com.example.shop is in the background (running-background)"), body)
    XCTAssertTrue(body.contains("com.apple.Preferences is in front"), body)
  }

  func test_the_home_screen_is_said_as_no_app_in_front() {
    let hint = ForegroundGate.hint(
      AppNotInFront(bundleId: "com.example.shop", state: "running-background-suspended", inFront: []))
    XCTAssertTrue(hint.contains("no app in front"), hint)
  }
}

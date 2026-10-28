import XCTest
import FlyingFox
@testable import SmixRunnerCore

final class PinchRouteTests: XCTestCase {
  private func req(_ json: String) throws -> PinchRoute.PinchRequest {
    try PinchRoute.decode(Data(json.utf8))
  }

  func test_decode_withoutPan_defaultsPanToNone() throws {
    let r = try req(#"{"cx":0.5,"cy":0.4,"fromSpan":0.2,"toSpan":0.6,"durationMs":500}"#)
    XCTAssertEqual(r, PinchRoute.PinchRequest(cx: 0.5, cy: 0.4, fromSpan: 0.2, toSpan: 0.6, durationMs: 500))
  }

  func test_decode_withPan() throws {
    let r = try req(#"{"cx":0.5,"cy":0.5,"fromSpan":0.6,"toSpan":0.2,"durationMs":300,"panDx":0.1,"panDy":-0.2,"panMs":250}"#)
    XCTAssertEqual(r.panDx, 0.1)
    XCTAssertEqual(r.panDy, -0.2)
    XCTAssertEqual(r.panMs, 250)
  }

  func test_decode_missingSpan_namesIt() {
    XCTAssertThrowsError(try req(#"{"cx":0.5,"cy":0.5,"fromSpan":0.2,"durationMs":500}"#)) {
      XCTAssertEqual($0 as? PinchRoute.DecodeError, .missingField("toSpan"))
    }
  }

  func test_decode_zeroSpan_refused() {
    XCTAssertThrowsError(try req(#"{"cx":0.5,"cy":0.5,"fromSpan":0,"toSpan":0.6,"durationMs":500}"#)) {
      XCTAssertEqual($0 as? PinchRoute.DecodeError, .outOfRange("fromSpan", 0))
    }
  }

  func test_decode_tooShortDuration_refused() {
    XCTAssertThrowsError(try req(#"{"cx":0.5,"cy":0.5,"fromSpan":0.2,"toSpan":0.6,"durationMs":10}"#)) {
      XCTAssertEqual($0 as? PinchRoute.DecodeError, .outOfRange("durationMs", 10))
    }
  }

  func test_decode_boolIsNotANumber() {
    XCTAssertThrowsError(try req(#"{"cx":true,"cy":0.5,"fromSpan":0.2,"toSpan":0.6,"durationMs":500}"#)) {
      XCTAssertEqual($0 as? PinchRoute.DecodeError, .invalidField("cx", "1"))
    }
  }

  func test_plan_spansAreSharesOfTheShorterSide() throws {
    let r = PinchRoute.PinchRequest(cx: 0.5, cy: 0.5, fromSpan: 0.2, toSpan: 0.6, durationMs: 500)
    let p = try PinchRoute.plan(r, size: CGSize(width: 400, height: 800))
    XCTAssertEqual(p.start.first, CGPoint(x: 160, y: 400))
    XCTAssertEqual(p.start.second, CGPoint(x: 240, y: 400))
    XCTAssertEqual(p.pinched.first, CGPoint(x: 80, y: 400))
    XCTAssertEqual(p.pinched.second, CGPoint(x: 320, y: 400))
    XCTAssertEqual(p.end, p.pinched)
    let landscape = try PinchRoute.plan(r, size: CGSize(width: 800, height: 400))
    XCTAssertEqual(landscape.pinched.second.x - landscape.pinched.first.x, 240)
  }

  func test_plan_fingersComeDownShortOfTheStart() throws {
    let r = PinchRoute.PinchRequest(cx: 0.5, cy: 0.5, fromSpan: 0.2, toSpan: 0.6, durationMs: 500)
    let p = try PinchRoute.plan(r, size: CGSize(width: 400, height: 800))
    // zooming in: each finger starts 5 points nearer the middle
    XCTAssertEqual(p.down.first, CGPoint(x: 165, y: 400))
    XCTAssertEqual(p.down.second, CGPoint(x: 235, y: 400))
    let out = PinchRoute.PinchRequest(cx: 0.5, cy: 0.5, fromSpan: 0.6, toSpan: 0.2, durationMs: 500)
    let q = try PinchRoute.plan(out, size: CGSize(width: 400, height: 800))
    XCTAssertEqual(q.down.first, CGPoint(x: 75, y: 400))
  }

  func test_plan_comingDownOffTheEdge_refused() {
    // zooming out with the fingers at the edges: the start is on screen, the
    // point 5 points outside it is not
    let r = PinchRoute.PinchRequest(cx: 0.5, cy: 0.5, fromSpan: 1, toSpan: 0.2, durationMs: 500)
    XCTAssertThrowsError(try PinchRoute.plan(r, size: CGSize(width: 400, height: 800))) {
      XCTAssertEqual($0 as? PinchRoute.DecodeError, .offScreen(finger: 1, moment: "as it comes down", x: -5, y: 400))
    }
  }

  func test_plan_panMovesBothFingersTogether() throws {
    let r = PinchRoute.PinchRequest(
      cx: 0.5, cy: 0.5, fromSpan: 0.2, toSpan: 0.6, durationMs: 500, panDx: 0, panDy: -0.25, panMs: 300)
    let p = try PinchRoute.plan(r, size: CGSize(width: 400, height: 800))
    XCTAssertEqual(p.end.first, CGPoint(x: 80, y: 200))
    XCTAssertEqual(p.end.second, CGPoint(x: 320, y: 200))
  }

  func test_plan_panIgnoredWhenItHasNoDuration() throws {
    let r = PinchRoute.PinchRequest(
      cx: 0.5, cy: 0.5, fromSpan: 0.2, toSpan: 0.6, durationMs: 500, panDx: 0.3, panDy: 0, panMs: 0)
    let p = try PinchRoute.plan(r, size: CGSize(width: 400, height: 800))
    XCTAssertEqual(p.end, p.pinched)
  }

  func test_plan_fingerOffScreen_namesFingerAndMoment() {
    let r = PinchRoute.PinchRequest(cx: 0.1, cy: 0.5, fromSpan: 0.1, toSpan: 0.6, durationMs: 500)
    XCTAssertThrowsError(try PinchRoute.plan(r, size: CGSize(width: 400, height: 800))) {
      XCTAssertEqual($0 as? PinchRoute.DecodeError, .offScreen(finger: 1, moment: "after the pinch", x: -80, y: 400))
    }
  }

  func test_plan_panOffScreen_refused() {
    let r = PinchRoute.PinchRequest(
      cx: 0.5, cy: 0.5, fromSpan: 0.2, toSpan: 0.6, durationMs: 500, panDx: 0.3, panDy: 0, panMs: 300)
    XCTAssertThrowsError(try PinchRoute.plan(r, size: CGSize(width: 400, height: 800))) {
      XCTAssertEqual($0 as? PinchRoute.DecodeError, .offScreen(finger: 2, moment: "after the pan", x: 440, y: 400))
    }
  }

  func test_success_putsOkAndFingersOnTheWire() async throws {
    let r = PinchRoute.PinchRequest(cx: 0.5, cy: 0.5, fromSpan: 0.2, toSpan: 0.6, durationMs: 500)
    let p = try PinchRoute.plan(r, size: CGSize(width: 400, height: 800))
    let resp = PinchRoute.success(ok: true, plan: p)
    XCTAssertEqual(resp.statusCode, .ok)
    let obj = try JSONSerialization.jsonObject(with: try await resp.bodyData) as? [String: Any]
    XCTAssertEqual(obj?["ok"] as? Bool, true)
    let fingers = obj?["fingers"] as? [String: Any]
    let to = fingers?["to"] as? [[String: Int]]
    XCTAssertEqual(to?[0]["x"], 80)
    XCTAssertEqual(to?[1]["x"], 320)
  }

  func test_badRequest_is400WithReason() async throws {
    let resp = PinchRoute.badRequest(reason: "finger 1 \"x\"")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let obj = try JSONSerialization.jsonObject(with: try await resp.bodyData) as? [String: Any]
    XCTAssertEqual(obj?["reason"] as? String, "finger 1 \"x\"")
  }
}

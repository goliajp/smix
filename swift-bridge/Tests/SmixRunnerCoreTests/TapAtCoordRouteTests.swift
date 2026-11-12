import FlyingFox
import Foundation
import XCTest

@testable import SmixRunnerCore

final class TapAtCoordRouteTests: XCTestCase {
  // The synthesised gesture does not report when the touch went down,
  // so the route reports bounds derived from the call's own span. A host
  // that took these for instants would claim a frame was captured
  // during a press it was not — which is the failure that asked for
  // this in the first place.

  func test_timings_boundTheHoldInsideTheCallSpan() {
    // Entered at 1000, called 1200..2500, asked to hold 1000ms.
    let t = TapAtCoordRoute.PressTimings.around(
      callStartMs: 1200, callEndMs: 2500, holdMs: 1000, handlerEntryMs: 1000)
    XCTAssertEqual(t.latestDownOffsetMs, 500)    // (2500-1000) - 1000
    XCTAssertEqual(t.earliestUpOffsetMs, 1200)   // (1200+1000) - 1000
    XCTAssertEqual(t.handlerWallMs, 1500)
  }

  /// A call that returned in less than the hold it was given leaves no
  /// interval bounded by both ends; the down bound must not run past
  /// the call's own start.
  func test_timings_doNotPlaceTouchDownBeforeTheCallBegan() {
    let t = TapAtCoordRoute.PressTimings.around(
      callStartMs: 1200, callEndMs: 1400, holdMs: 1000, handlerEntryMs: 1000)
    XCTAssertEqual(t.latestDownOffsetMs, 200)
    XCTAssertLessThan(t.latestDownOffsetMs, t.earliestUpOffsetMs)
  }

  func test_success_carriesTheBoundsOfAHeldTouch() async throws {
    let resp = TapAtCoordRoute.success(
      ok: true, chain: [],
      press: .init(latestDownOffsetMs: 500, earliestUpOffsetMs: 1200, handlerWallMs: 1500))
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(
      body,
      #"{"ok":true,"chain":[],"latestDownOffsetMs":500,"earliestUpOffsetMs":1200,"handlerWallMs":1500}"#)
  }

  func test_success_withoutBoundsIsTheTapBody() async throws {
    let body = try await String(
      decoding: TapAtCoordRoute.success(ok: true, chain: [], press: nil).bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":true,"chain":[]}"#)
  }

  func test_success_saysWhichEntriesAreBehind() async throws {
    let f = CGRect(x: 0, y: 0, width: 10, height: 20)
    let chain = [
      HitChainEntry(identifier: "a", label: "", frame: f),
      HitChainEntry(identifier: "b", label: "", frame: f, behind: true),
    ]
    let body = try await String(
      decoding: TapAtCoordRoute.success(ok: true, chain: chain, press: nil).bodyData, as: UTF8.self)
    XCTAssertEqual(
      body,
      #"{"ok":true,"chain":[{"identifier":"a","label":"","frame":{"x":0.0,"y":0.0,"w":10.0,"h":20.0}},"#
        + #"{"identifier":"b","label":"","frame":{"x":0.0,"y":0.0,"w":10.0,"h":20.0},"behind":true}]}"#)
  }

  func test_decode_takesTimesAndHold() throws {
    let req = try TapAtCoordRoute.decode(
      Data(#"{"nx":0.5,"ny":0.25,"times":2,"holdMs":800}"#.utf8))
    XCTAssertEqual(req.times, 2)
    XCTAssertEqual(req.holdMs, 800)
    XCTAssertEqual(req.intervalMs, TouchTimeline.defaultIntervalMs)
    XCTAssertFalse(req.doubleTap)
  }

  func test_decode_takesDoubleTap() throws {
    let req = try TapAtCoordRoute.decode(
      Data(#"{"nx":0.5,"ny":0.25,"times":2,"doubleTap":true}"#.utf8))
    XCTAssertTrue(req.doubleTap)
    XCTAssertEqual(req.times, 2)
  }
}

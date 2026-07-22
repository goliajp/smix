// The spacing of a burst, which is the whole point of having one.
//
// The value of sending N touches in one synthesise is not that it is
// faster — though at ~400 ms per synthesise it is — but that the
// interval becomes a stated number instead of whatever the round trip
// happened to cost.

import XCTest

@testable import SmixRunnerCore

final class TouchTimelineTests: XCTestCase {

  func testOffsetsAreEvenlySpaced() {
    let offsets = TouchTimeline.downOffsets(times: 4, intervalMs: 100)
    XCTAssertEqual(offsets.count, 4)
    for (i, o) in offsets.enumerated() {
      XCTAssertEqual(o, Double(i) * 0.1, accuracy: 1e-9)
    }
  }

  /// A burst of one is a tap, not an error.
  func testTimesBelowOneIsASingleTouch() {
    XCTAssertEqual(TouchTimeline.downOffsets(times: 0, intervalMs: 50).count, 1)
    XCTAssertEqual(TouchTimeline.downOffsets(times: -3, intervalMs: 50).count, 1)
  }

  /// Two touches at the same offset are one event to a recogniser.
  ///
  /// Clamping means a caller asking for ten taps gets ten, rather than
  /// silently getting one and a puzzle.
  func testZeroIntervalStillSeparatesTheTouches() {
    let offsets = TouchTimeline.downOffsets(times: 3, intervalMs: 0)
    XCTAssertEqual(Set(offsets).count, 3, "offsets collapsed onto each other")
  }

  func testHoldExtendsPastTheDownOffset() {
    XCTAssertEqual(
      TouchTimeline.upOffset(downOffset: 0.2, holdMs: 50), 0.25, accuracy: 1e-9)
  }

  /// What the caller is waiting for: last touch down, plus its hold.
  func testDurationCoversTheWholeBurst() {
    XCTAssertEqual(
      TouchTimeline.duration(times: 10, intervalMs: 100, holdMs: 50),
      0.9 + 0.05,
      accuracy: 1e-9)
  }

  /// The documented default cadence stays fast enough to be worth
  /// having — ten taps well inside the 4.28 s that ten round trips cost.
  func testDefaultCadenceBeatsTenRoundTrips() {
    let d = TouchTimeline.duration(
      times: 10, intervalMs: TouchTimeline.defaultIntervalMs,
      holdMs: TouchTimeline.defaultHoldMs)
    XCTAssertLessThan(d, 1.0, "ten taps at the default cadence should be under a second")
  }
}

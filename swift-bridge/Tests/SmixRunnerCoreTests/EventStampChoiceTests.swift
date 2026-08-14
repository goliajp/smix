import Foundation
import XCTest

@testable import SmixRunnerCore

/// Which orientation a synthesised touch is stamped with.
///
/// The stamp decides which space the coordinates in an event path are
/// read against. Every one of them was written out as a literal
/// `.portrait`, and a point computed against a landscape `app.frame`
/// then arrives read against a portrait screen — measured, on a
/// fixture, with the counter never moving and the pixels never
/// changing.
///
/// Two repairs are possible and only one can be right, so both are
/// buildable here and a device picks between them. This file pins the
/// arithmetic; it cannot say which strategy the system actually honours
/// — that is what the experiment on a simulator is for, and stating a
/// winner here would be the analysis choosing instead of the
/// measurement.
final class EventStampChoiceTests: XCTestCase {
  func testDerivingFromTheAppFrameFollowsTheLayout() {
    XCTAssertEqual(
      eventStamp(forAppFrame: CGSize(width: 874, height: 402), strategy: .deriveFromAppFrame),
      .landscapeRight)
    XCTAssertEqual(
      eventStamp(forAppFrame: CGSize(width: 402, height: 874), strategy: .deriveFromAppFrame),
      .portrait)
  }

  /// Today's behaviour, kept buildable so the experiment has a control
  /// row. Without it, "the fix works" is a claim with nothing beside
  /// it — the same reason the landscape repro carries a portrait run.
  func testTheLegacyStrategyIsAlwaysPortrait() {
    for size in [CGSize(width: 874, height: 402), CGSize(width: 402, height: 874)] {
      XCTAssertEqual(
        eventStamp(forAppFrame: size, strategy: .legacyAlwaysPortrait),
        .portrait,
        "the control row has to reproduce the defect, or it is not a control")
    }
  }

  /// The third strategy leaves the stamp alone and moves the point
  /// instead, so its stamp answer is portrait by construction. Pinned
  /// so that a later edit cannot quietly turn it into the first
  /// strategy and make the experiment compare two copies of one idea.
  func testConvertingThePointKeepsThePortraitStamp() {
    XCTAssertEqual(
      eventStamp(
        forAppFrame: CGSize(width: 874, height: 402), strategy: .convertPointToDeviceSpace),
      .portrait)
  }

  /// A square frame has no handedness to read. It cannot happen on any
  /// device this drives, and a rule that silently picks one of two
  /// answers at the boundary is a rule nobody can predict — so it is
  /// stated rather than left to whichever comparison operator was
  /// typed.
  func testASquareFrameStaysPortrait() {
    XCTAssertEqual(
      eventStamp(forAppFrame: CGSize(width: 500, height: 500), strategy: .deriveFromAppFrame),
      .portrait)
  }

  /// Converting a landscape-space point to the device's portrait space.
  ///
  /// The device frame is the app frame turned on its side. A point at
  /// the app's top-left corner is at the device's bottom-left when the
  /// interface is `landscapeRight`, so the arithmetic is a rotation
  /// rather than a swap of x and y — a swap alone would mirror the
  /// screen and land taps in the wrong corner, which is the kind of
  /// nearly-right that survives a smoke test.
  func testPointConversionRotatesRatherThanSwaps() {
    let appFrame = CGSize(width: 874, height: 402)

    // Anchored on a measurement, not on arithmetic reasoned out here.
    // The fixture's increment button sits at (437, 260.3) in the app's
    // landscape space, and a screenshot puts it at roughly (141, 436)
    // in the device's portrait space. `(height - y, x)` gives
    // (141.7, 437). The first draft of this test asserted (0, 0) maps
    // to (0, 874), reasoned from a mental picture, and the numbers off
    // the device disagreed — which is the whole reason the strategy is
    // chosen by experiment rather than by analysis.
    let measured = pointInDeviceSpace(
      CGPoint(x: 437, y: 260.3), appFrame: appFrame, interface: .landscapeRight)
    XCTAssertEqual(measured.x, 141.7, accuracy: 0.05)
    XCTAssertEqual(measured.y, 437, accuracy: 0.05)

    // The app's origin is at the device's top-right corner, not its
    // top-left: a swap of x and y would put it at the origin and mirror
    // everything about the diagonal.
    let origin = pointInDeviceSpace(
      CGPoint(x: 0, y: 0), appFrame: appFrame, interface: .landscapeRight)
    XCTAssertEqual(origin.x, 402, accuracy: 0.001)
    XCTAssertEqual(origin.y, 0, accuracy: 0.001)
  }

  func testPointConversionIsIdentityInPortrait() {
    let p = CGPoint(x: 201, y: 437)
    let converted = pointInDeviceSpace(
      p, appFrame: CGSize(width: 402, height: 874), interface: .portrait)
    XCTAssertEqual(converted.x, p.x, accuracy: 0.001)
    XCTAssertEqual(converted.y, p.y, accuracy: 0.001)
  }
}

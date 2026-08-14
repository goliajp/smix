import Foundation
import XCTest

@testable import SmixRunnerCore

/// The runner can describe the screen and it can touch the screen, and
/// until now nothing made it say whether those two are the same space.
///
/// A consumer's landscape taps were reported as landing inside the
/// button they aimed at while the button never fired. Both halves of
/// that sentence were true: `HitChain` had the point inside the element
/// as the snapshot describes it, and the snapshot describes a landscape
/// screen — but what the touch is measured against is
/// `XCUIApplication.frame`, and the synthesised event carries an
/// interface orientation of its own. Three numbers, no route that
/// returns them together, so the mismatch could only be inferred.
///
/// This route exists to make it measurable rather than arguable. It
/// takes no action and moves nothing.
final class CoordinateSpaceRouteTests: XCTestCase {
  func testBodyCarriesBothSpacesAndTheResolvedPoint() throws {
    let body = CoordinateSpaceRoute.body(
      appFrame: CGRect(x: 0, y: 0, width: 402, height: 874),
      snapshotRootFrame: CGRect(x: 0, y: 0, width: 874, height: 402),
      deviceOrientation: "landscapeRight",
      eventRecordOrientation: "portrait",
      stampStrategy: "legacyAlwaysPortrait",
      nx: 0.5,
      ny: 0.5,
      resolvedPoint: CGPoint(x: 201, y: 437)
    )
    let json = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: body) as? [String: Any])

    let app = try XCTUnwrap(json["appFrame"] as? [String: Any])
    XCTAssertEqual(app["w"] as? Double, 402)
    XCTAssertEqual(app["h"] as? Double, 874)

    let root = try XCTUnwrap(json["snapshotRootFrame"] as? [String: Any])
    XCTAssertEqual(root["w"] as? Double, 874)
    XCTAssertEqual(root["h"] as? Double, 402)

    XCTAssertEqual(json["deviceOrientation"] as? String, "landscapeRight")

    // The orientation stamped on the synthesised event, which is what
    // decides how the point below is read once it leaves here. It is
    // reported rather than assumed: this is the field the whole
    // investigation turns on.
    XCTAssertEqual(json["eventRecordOrientation"] as? String, "portrait")

    let point = try XCTUnwrap(json["resolvedPoint"] as? [String: Any])
    XCTAssertEqual(point["x"] as? Double, 201)
    XCTAssertEqual(point["y"] as? Double, 437)
  }

  /// Two spaces disagreeing is the whole subject, so the body says so
  /// itself rather than leaving every reader to compare four numbers.
  func testAgreementIsStatedNotLeftToTheReader() throws {
    let disagreeing = CoordinateSpaceRoute.body(
      appFrame: CGRect(x: 0, y: 0, width: 402, height: 874),
      snapshotRootFrame: CGRect(x: 0, y: 0, width: 874, height: 402),
      deviceOrientation: "landscapeRight",
      eventRecordOrientation: "portrait",
      stampStrategy: "legacyAlwaysPortrait",
      nx: 0.5, ny: 0.5,
      resolvedPoint: CGPoint(x: 201, y: 437))
    let a = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: disagreeing) as? [String: Any])
    XCTAssertEqual(a["spacesAgree"] as? Bool, false)

    let agreeing = CoordinateSpaceRoute.body(
      appFrame: CGRect(x: 0, y: 0, width: 402, height: 874),
      snapshotRootFrame: CGRect(x: 0, y: 0, width: 402, height: 874),
      deviceOrientation: "portrait",
      eventRecordOrientation: "portrait",
      stampStrategy: "legacyAlwaysPortrait",
      nx: 0.5, ny: 0.5,
      resolvedPoint: CGPoint(x: 201, y: 437))
    let b = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: agreeing) as? [String: Any])
    XCTAssertEqual(b["spacesAgree"] as? Bool, true)
  }

  /// A simulator that has never been rotated reports its orientation as
  /// `unknown`, and portrait is where every tap in this repository's
  /// own control group lands. The first version of the predicate
  /// compared the event's stamp against that string and called the
  /// working case a mismatch — measured on a real device before it was
  /// written down here.
  func testAnUnknownDeviceOrientationIsNotAMismatch() throws {
    let body = CoordinateSpaceRoute.body(
      appFrame: CGRect(x: 0, y: 0, width: 402, height: 874),
      snapshotRootFrame: CGRect(x: 0, y: 0, width: 402, height: 874),
      deviceOrientation: "unknown",
      eventRecordOrientation: "portrait",
      stampStrategy: "legacyAlwaysPortrait",
      nx: 0.5, ny: 0.5,
      resolvedPoint: CGPoint(x: 201, y: 437))
    let json = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: body) as? [String: Any])
    XCTAssertEqual(json["spacesAgree"] as? Bool, true)
  }

  /// The landscape case as measured: both frames landscape, both
  /// agreeing with each other, and the event still stamped portrait.
  /// The disagreement is the stamp alone — neither "the tree is wrong"
  /// nor "the aim is wrong", which were the two hypotheses this
  /// measurement was built to choose between and both of which it
  /// refuted.
  func testTheMeasuredLandscapeCaseIsAStampMismatchAlone() throws {
    let body = CoordinateSpaceRoute.body(
      appFrame: CGRect(x: 0, y: 0, width: 874, height: 402),
      snapshotRootFrame: CGRect(x: 0, y: 0, width: 874, height: 402),
      deviceOrientation: "unknown",
      eventRecordOrientation: "portrait",
      stampStrategy: "legacyAlwaysPortrait",
      nx: 0.5, ny: 0.5,
      resolvedPoint: CGPoint(x: 437, y: 201))
    let json = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: body) as? [String: Any])
    XCTAssertEqual(json["spacesAgree"] as? Bool, false)

    let app = try XCTUnwrap(json["appFrame"] as? [String: Any])
    let root = try XCTUnwrap(json["snapshotRootFrame"] as? [String: Any])
    XCTAssertEqual(app["w"] as? Double, root["w"] as? Double)
    XCTAssertEqual(app["h"] as? Double, root["h"] as? Double)
  }
}

extension CoordinateSpaceRouteTests {
  /// A portrait stamp with the point already rotated into the device's
  /// space is not a mismatch. Judged by the stamp alone it looks like
  /// one, and the refusal built on that verdict blocked every touch in
  /// the experiment that was supposed to choose between the two
  /// repairs — three rows, identical, none of them a measurement.
  func testACompensatedDeliveryIsNotAMismatch() throws {
    let body = CoordinateSpaceRoute.body(
      appFrame: CGRect(x: 0, y: 0, width: 874, height: 402),
      snapshotRootFrame: CGRect(x: 0, y: 0, width: 874, height: 402),
      deviceOrientation: "portrait",
      eventRecordOrientation: "portrait",
      stampStrategy: "convertPointToDeviceSpace",
      nx: 0.5, ny: 0.5,
      resolvedPoint: CGPoint(x: 437, y: 201))
    let json = try XCTUnwrap(
      try JSONSerialization.jsonObject(with: body) as? [String: Any])
    XCTAssertEqual(json["spacesAgree"] as? Bool, true)
  }
}

import CoreGraphics
import XCTest

@testable import SmixRunnerCore

// Semantic lock for the frameFromDictValue pure function. The act-side
// findAndTapSystemPopupButton works off snapshots, so it has to derive a
// button's tap centre from the frame value inside
// XCUIElementSnapshot.dictionaryRepresentation. That value comes in two
// shapes: [String: Double] (X/Y/Width/Height) or a CGRect. This file locks
// all three outcomes (dict / CGRect / unparseable → .zero), equivalent to
// the frame parsing convertSnapshotDict already does on the UITests side.
// The Core layer has no XCUI dependency, hence the Any? parameter.
final class FrameFromDictValueTests: XCTestCase {
  func test_dictForm_parsesXYWH() {
    XCTAssertEqual(
      frameFromDictValue(["X": 10.0, "Y": 20.0, "Width": 100.0, "Height": 44.0]),
      CGRect(x: 10, y: 20, width: 100, height: 44))
  }

  func test_dictForm_missingKeysDefaultZero() {
    XCTAssertEqual(
      frameFromDictValue(["X": 10.0, "Width": 100.0]),
      CGRect(x: 10, y: 0, width: 100, height: 0))
  }

  func test_cgRectForm_returnsAsIs() {
    let r = CGRect(x: 5, y: 6, width: 7, height: 8)
    XCTAssertEqual(frameFromDictValue(r), r)
  }

  func test_unparseable_returnsZero() {
    XCTAssertEqual(frameFromDictValue(["bad": 1.0]), .zero)
    XCTAssertEqual(frameFromDictValue("str"), .zero)
    XCTAssertEqual(frameFromDictValue(nil), .zero)
  }
}

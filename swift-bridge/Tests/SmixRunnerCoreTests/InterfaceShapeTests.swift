import CoreGraphics
import XCTest

@testable import SmixRunnerCore

final class InterfaceShapeTests: XCTestCase {
  private func app(_ w: CGFloat, _ h: CGFloat, child: CGRect) -> TreeRoute.A11ySnapshotData {
    let button = TreeRoute.A11ySnapshotData(
      elementTypeRawValue: 9, identifier: "close", label: "Close", value: nil,
      frame: child, isEnabled: true, isSelected: false, children: [])
    return TreeRoute.A11ySnapshotData(
      elementTypeRawValue: 2, identifier: "app", label: "", value: nil,
      frame: CGRect(x: 0, y: 0, width: w, height: h), isEnabled: true, isSelected: false,
      children: [button])
  }

  /// Safari unfolded on a foldable: its close button sat at x = 742 in a
  /// 951-wide layout under an application frame reported 669 wide.
  func testALandscapeAppOnAPortraitShapedFrameKeepsItsRightHalfOnScreen() {
    let closeFrame = CGRect(x: 742, y: 110, width: 40, height: 40)
    let reported = app(669, 951, child: closeFrame)
    XCTAssertFalse(TreeRoute.isVisible(closeFrame, reported.frame))
    let shaped = reported.inInterfaceShape(landscape: true)
    XCTAssertEqual(shaped.frame, CGRect(x: 0, y: 0, width: 951, height: 669))
    XCTAssertTrue(TreeRoute.isVisible(closeFrame, shaped.frame))
    XCTAssertEqual(shaped.children.first?.frame, closeFrame)
  }

  func testAFrameAlreadyInTheAppsShapeIsLeftAlone() {
    let child = CGRect(x: 10, y: 10, width: 5, height: 5)
    XCTAssertEqual(app(951, 669, child: child).inInterfaceShape(landscape: true).frame.width, 951)
    XCTAssertEqual(app(402, 874, child: child).inInterfaceShape(landscape: false).frame.width, 402)
  }
}

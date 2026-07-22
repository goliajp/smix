// The chain reported back after a tap.
//
// Shapes taken from a live iPhone 17 Pro running Settings (iOS 26.5,
// 2026-07-22). The interesting one is the first row: a button whose own
// label sits inside it, so the innermost element at the row's centre is
// the label and not the row. Reporting one element would make the host
// call that tap a miss.

import CoreGraphics
import XCTest

@testable import SmixRunnerCore

final class HitChainTests: XCTestCase {

  private func node(
    _ identifier: String,
    _ label: String,
    _ frame: CGRect,
    _ children: [TreeRoute.A11ySnapshotData] = []
  ) -> TreeRoute.A11ySnapshotData {
    TreeRoute.A11ySnapshotData(
      elementTypeRawValue: 1, identifier: identifier, label: label, value: nil,
      frame: frame, isEnabled: true, isSelected: false, children: children)
  }

  /// Settings' first row, as observed: an anonymous container holding a
  /// named button, holding the button's own label.
  private func settingsFirstRow() -> TreeRoute.A11ySnapshotData {
    node(
      "com.apple.Preferences", "设置", CGRect(x: 0, y: 0, width: 402, height: 874),
      [
        node(
          "", "", CGRect(x: 0, y: 100, width: 402, height: 700),
          [
            node(
              "com.apple.settings.primaryAppleAccount", "Apple账户",
              CGRect(x: 16, y: 168, width: 370, height: 90),
              [
                node(
                  "", "登录以访问iCloud数据、App Store等",
                  CGRect(x: 24, y: 196, width: 216, height: 33))
              ])
          ])
      ])
  }

  func testInnermostFirstByArea() {
    let chain = HitChain.at(
      point: CGPoint(x: 201, y: 213), in: settingsFirstRow())
    XCTAssertEqual(chain.count, 3, "expected label, button, application")
    XCTAssertEqual(chain[0].label, "登录以访问iCloud数据、App Store等")
    XCTAssertEqual(chain[1].identifier, "com.apple.settings.primaryAppleAccount")
    XCTAssertEqual(chain[2].identifier, "com.apple.Preferences")
  }

  /// The anonymous container between them is not reported.
  ///
  /// It contains the point, and the host cannot do anything with it:
  /// selectors match on identifier and label, and there is nothing to
  /// match. Including it would make the chain longer without making it
  /// more useful.
  func testUnnamedElementsAreLeftOut() {
    let chain = HitChain.at(
      point: CGPoint(x: 201, y: 213), in: settingsFirstRow())
    XCTAssertFalse(
      chain.contains { $0.identifier.isEmpty && $0.label.isEmpty },
      "an unnamed element reached the chain")
  }

  /// A point outside everything yields nothing, which the host reads as
  /// a stale frame rather than as a pass.
  func testPointOutsideTheTreeYieldsNothing() {
    XCTAssertTrue(
      HitChain.at(point: CGPoint(x: 5000, y: 5000), in: settingsFirstRow()).isEmpty)
  }

  /// Depth does not order the chain; area does.
  ///
  /// A point can sit in several branches at once, and a deeper node in
  /// one branch is not inside a shallower node in another. The live tree
  /// had 29 elements containing a single point across multiple branches.
  func testASiblingBranchDoesNotOutrankBySize() {
    let tree = node(
      "root", "", CGRect(x: 0, y: 0, width: 400, height: 800),
      [
        node("wide-sibling", "", CGRect(x: 0, y: 0, width: 400, height: 800)),
        node(
          "mid", "", CGRect(x: 0, y: 0, width: 200, height: 200),
          [node("small", "", CGRect(x: 0, y: 0, width: 50, height: 50))]),
      ])
    let chain = HitChain.at(point: CGPoint(x: 10, y: 10), in: tree)
    XCTAssertEqual(
      chain.map(\.identifier), ["small", "mid", "root", "wide-sibling"],
      "the chain must be ordered by area, not by traversal or depth")
  }

  /// Zero-sized elements contain nothing.
  func testEmptyFramesAreSkipped() {
    let tree = node(
      "root", "", CGRect(x: 0, y: 0, width: 400, height: 800),
      [node("degenerate", "", CGRect(x: 10, y: 10, width: 0, height: 0))])
    let chain = HitChain.at(point: CGPoint(x: 10, y: 10), in: tree)
    XCTAssertEqual(chain.map(\.identifier), ["root"])
  }
}

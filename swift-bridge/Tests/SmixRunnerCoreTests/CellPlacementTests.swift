// The controls inside an off-screen list cell.
//
// Shapes taken from a UIKit table of 30 rows, 66 pt each, under a 44 pt
// header, on an iPhone simulator running iOS 27.0 (24A434), 2026-10-12.
// Rows 0-11 were on screen with their buttons where the rows are; from
// row 12 on, each cell kept its own frame and its button was placed at
// the table's origin, y=106, and called visible.

import CoreGraphics
import XCTest

@testable import SmixRunnerCore

final class CellPlacementTests: XCTestCase {

  private let screen = CGRect(x: 0, y: 0, width: 402, height: 874)

  private func node(
    _ type: UInt,
    _ identifier: String,
    _ frame: CGRect,
    _ children: [TreeRoute.A11ySnapshotData] = []
  ) -> TreeRoute.A11ySnapshotData {
    TreeRoute.A11ySnapshotData(
      elementTypeRawValue: type, identifier: identifier, label: "", value: nil,
      frame: frame, isEnabled: true, isSelected: false, children: children)
  }

  private func row(_ i: Int, button: CGRect, extra: [TreeRoute.A11ySnapshotData] = [])
    -> TreeRoute.A11ySnapshotData
  {
    node(
      CellPlacement.cellType, "",
      CGRect(x: 0, y: 106 + 66 * CGFloat(i), width: 402, height: 66),
      [node(9, "uk_row_\(i)", button)] + extra)
  }

  private func rowFrame(_ i: Int) -> CGRect {
    CGRect(x: 16, y: 106 + 66 * CGFloat(i), width: 370, height: 66)
  }

  private let tableOrigin = CGRect(x: 16, y: 106, width: 370, height: 66)

  /// Rows 0, 11 and 12 on screen, and 20 below it with its button at the
  /// table's origin.
  private func table() -> TreeRoute.A11ySnapshotData {
    node(
      1, "app", screen,
      [
        node(
          1, "table", CGRect(x: 0, y: 106, width: 402, height: 768),
          [
            row(0, button: rowFrame(0)),
            row(11, button: rowFrame(11)),
            row(12, button: tableOrigin),
            row(20, button: tableOrigin),
          ])
      ])
  }

  private func frames(_ d: TreeRoute.A11ySnapshotData, into out: inout [String: CGRect]) {
    if !d.identifier.isEmpty { out[d.identifier] = d.frame }
    for c in d.children { frames(c, into: &out) }
  }

  private func placedFrames(_ root: TreeRoute.A11ySnapshotData) -> [String: CGRect] {
    var out: [String: CGRect] = [:]
    frames(CellPlacement.placed(root), into: &out)
    return out
  }

  func test_a_button_placed_at_the_table_origin_takes_its_off_screen_cell_frame() {
    let f = placedFrames(table())
    XCTAssertEqual(f["uk_row_20"], CGRect(x: 0, y: 106 + 66 * 20, width: 402, height: 66))
    XCTAssertEqual(f["uk_row_12"], CGRect(x: 0, y: 106 + 66 * 12, width: 402, height: 66))
  }

  func test_buttons_in_rows_on_screen_keep_their_own_frames() {
    let f = placedFrames(table())
    XCTAssertEqual(f["uk_row_0"], rowFrame(0))
    XCTAssertEqual(f["uk_row_11"], rowFrame(11))
  }

  /// A row on screen holds its swipe actions past the screen's right edge
  /// until it is swiped; they must not be moved onto the row.
  func test_a_swipe_action_past_the_edge_of_a_row_on_screen_stays_there() {
    let delete = CGRect(x: 456.7, y: 106, width: 56, height: 65)
    let root = node(1, "app", screen, [row(0, button: rowFrame(0), extra: [node(9, "swipe-delete", delete)])])
    XCTAssertEqual(placedFrames(root)["swipe-delete"], delete)
  }

  /// A control in an off-screen cell that does touch the cell is placed
  /// where it says, and an empty one stays empty.
  func test_a_control_that_touches_its_off_screen_cell_or_is_empty_is_left_alone() {
    let own = CGRect(x: 16, y: 106 + 66 * 20 + 10, width: 100, height: 40)
    let root = node(
      1, "app", screen,
      [row(20, button: own, extra: [node(9, "zero", .zero)])])
    let f = placedFrames(root)
    XCTAssertEqual(f["uk_row_20"], own)
    XCTAssertEqual(f["zero"], .zero)
  }

  func test_the_tree_calls_a_button_in_an_off_screen_row_not_visible() throws {
    let obj = try XCTUnwrap(
      JSONSerialization.jsonObject(with: TreeRoute.serialize(table(), logSink: nil)) as? [String: Any])
    var visible: [String: Bool] = [:]
    func walk(_ n: [String: Any]) {
      if let id = n["identifier"] as? String { visible[id] = n["visible"] as? Bool }
      for c in n["children"] as? [[String: Any]] ?? [] { walk(c) }
    }
    walk(obj)
    XCTAssertEqual(visible["uk_row_0"], true)
    XCTAssertEqual(visible["uk_row_11"], true)
    XCTAssertEqual(visible["uk_row_12"], false)
    XCTAssertEqual(visible["uk_row_20"], false)
  }

  /// A tap at row 0 was reported "inside" every off-screen row's button.
  func test_a_tap_at_row_zero_is_inside_row_zero_alone() {
    let chain = HitChain.at(point: CGPoint(x: 200, y: 139), in: table())
    XCTAssertEqual(chain.map(\.identifier), ["uk_row_0", "table", "app"])
  }

  func test_misplaced_reads_the_cell_and_the_screen() {
    let offCell = CGRect(x: 0, y: 1426, width: 402, height: 66)
    XCTAssertTrue(CellPlacement.misplaced(control: tableOrigin, cell: offCell, screen: screen))
    XCTAssertFalse(
      CellPlacement.misplaced(control: tableOrigin, cell: CGRect(x: 0, y: 700, width: 402, height: 66), screen: screen),
      "a cell on screen")
    XCTAssertFalse(
      CellPlacement.misplaced(control: CGRect(x: 16, y: 1430, width: 50, height: 20), cell: offCell, screen: screen),
      "a control inside its cell")
    XCTAssertFalse(
      CellPlacement.misplaced(control: tableOrigin, cell: offCell, screen: .zero),
      "a screen of unknown size judges nothing")
  }
}

#if canImport(CoreGraphics)
import CoreGraphics
#endif

/// Where the controls inside an off-screen list cell are.
///
/// UIKit lists every row of a table or collection view to accessibility,
/// on screen or not. On iOS 27 the cell of an off-screen row keeps its own
/// frame, but the controls inside it are given a frame that is not theirs:
/// the list's origin, a copy of the first visible cell's frame, or zero.
/// Taken as true, that frame put row 20's button over row 0 — "visible",
/// so a scroll to it never scrolled, and a tap at it opened row 0.
///
/// The cell's own frame is right, and a control is drawn inside its cell,
/// so a control that does not touch its off-screen cell is given the
/// cell's frame. Off-screen cells only: a row on screen holds swipe
/// actions placed past the screen's edge until the row is swiped, and
/// moving those onto the row would make them tappable there. An empty
/// frame is left as it is; it already reads as not on screen.
public enum CellPlacement {
  public static func placed(_ root: TreeRoute.A11ySnapshotData) -> TreeRoute.A11ySnapshotData {
    place(root, screen: root.frame, offScreenCell: nil)
  }

  /// Whether a control's own frame is to be set aside for its cell's:
  /// the cell is off screen and the control does not touch it.
  public static func misplaced(control: CGRect, cell: CGRect, screen: CGRect) -> Bool {
    !control.isEmpty && !TreeRoute.isVisible(cell, screen) && !touches(control, cell)
  }

  static let cellType: UInt = 75

  private static func place(
    _ d: TreeRoute.A11ySnapshotData,
    screen: CGRect,
    offScreenCell: CGRect?
  ) -> TreeRoute.A11ySnapshotData {
    var frame = d.frame
    if let cell = offScreenCell, misplaced(control: frame, cell: cell, screen: screen) {
      frame = cell
    }
    var inside = offScreenCell
    if d.elementTypeRawValue == cellType {
      inside = TreeRoute.isVisible(frame, screen) ? nil : frame
    }
    return TreeRoute.A11ySnapshotData(
      elementTypeRawValue: d.elementTypeRawValue,
      identifier: d.identifier,
      label: d.label,
      value: d.value,
      frame: frame,
      isEnabled: d.isEnabled,
      isSelected: d.isSelected,
      hasFocus: d.hasFocus,
      children: d.children.map { place($0, screen: screen, offScreenCell: inside) },
      title: d.title,
      placeholderValue: d.placeholderValue
    )
  }

  static func touches(_ a: CGRect, _ b: CGRect) -> Bool {
    let i = a.intersection(b)
    return !i.isNull && !i.isEmpty
  }
}

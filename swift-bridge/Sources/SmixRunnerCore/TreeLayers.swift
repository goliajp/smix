import Foundation

#if canImport(CoreGraphics)
import CoreGraphics
#endif

extension TreeRoute {
  /// What a window shows over child `i`: the parts of the children drawn
  /// after it that something occupies. Empty and off-screen parts cover
  /// nothing.
  static func drawnAfter(_ children: [A11ySnapshotData], _ i: Int, _ rootFrame: CGRect) -> [CGRect] {
    children[(i + 1)...].flatMap { occupied($0, rootFrame) }.filter { f in
      let shown = rootFrame.isNull || rootFrame.isEmpty ? f : f.intersection(rootFrame)
      return !shown.isNull && shown.width > 0 && shown.height > 0
    }
  }

  /// Where a layer actually draws.
  ///
  /// A container spanning the whole screen that holds other elements is a
  /// host, not a surface: apps keep a full-window overlay for a badge or a
  /// floating button, and UIKit passes every touch outside those through
  /// to the window below. Taking the host's frame called every element of
  /// the app covered. Its children say where it draws. A smaller container
  /// (a sheet), or a full-screen node with nothing inside (a dimming
  /// view), is a surface and covers its whole frame.
  static func occupied(_ d: A11ySnapshotData, _ screen: CGRect) -> [CGRect] {
    guard !d.children.isEmpty, spans(d.frame, screen) else { return [d.frame] }
    return d.children.flatMap { occupied($0, screen) }
  }

  /// Whether `frame` reaches every edge of `screen`.
  static func spans(_ frame: CGRect, _ screen: CGRect) -> Bool {
    guard !screen.isNull, !screen.isEmpty else { return false }
    return frame.insetBy(dx: -0.5, dy: -0.5).contains(screen)
  }

  /// Whether a node's centre lies under any of `layers`.
  static func covers(_ layers: [CGRect], _ frame: CGRect) -> Bool {
    guard frame.width > 0, frame.height > 0 else { return false }
    let centre = CGPoint(x: frame.midX, y: frame.midY)
    return layers.contains { $0.contains(centre) }
  }
}

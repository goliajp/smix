import CoreGraphics

// Where hideKeyboard drags to take the keyboard down on a scroll view that
// dismisses it by drag (SwiftUI's `.scrollDismissesKeyboard(.interactively)`,
// UIKit's `keyboardDismissMode = .interactive`). On such a screen a tap
// outside the keyboard does nothing, and the only way out a person has is to
// drag the content down into the keyboard.
//
// The drag starts in the scroll view, never on the keyboard: a drag that
// starts on the keyboard is slide typing, and on a sim measured 2026-09-28 it
// put "gu" into an empty field and left the keyboard up. It ends in the
// keyboard's upper half. Measured the same day on a refreshable form, iOS 27:
// from just above the keyboard to 0.80 of the screen the keyboard went down
// and nothing refreshed; to 0.95 it refreshed; a drag ending 60pt into the
// keyboard let it spring back and lifted on a key, typing a character.
public enum KeyboardDismissDrag {
  public struct Plan: Equatable, Sendable {
    /// Normalised to the app frame, 0...1.
    public var start: CGPoint
    public var end: CGPoint
  }

  // Clear of the suggestion bar drawn above the keys: the keyboard frame
  // XCUITest reports leaves it out (keys at 590, bar from 544 on iOS 27), and
  // a drag started 40pt above the frame pressed a suggestion and typed it.
  static let aboveKeyboard: CGFloat = 90
  static let intoKeyboard: CGFloat = 0.45  // of the keyboard's height
  static let leastVisibleBand: CGFloat = 20

  /// nil when there is nothing to drag: no scroll view holds the focused
  /// field, or the one that does has no part showing above the keyboard.
  public static func plan(
    focused: CGRect, scrollers: [CGRect], keyboard: CGRect, app: CGRect
  ) -> Plan? {
    guard app.width > 0, app.height > 0, keyboard.height > 0,
          keyboard.minY > app.minY else { return nil }
    let centre = CGPoint(x: focused.midX, y: focused.midY)
    guard let scroller = scrollers
      .filter({ $0.contains(centre) })
      .min(by: { $0.width * $0.height < $1.width * $1.height })
    else { return nil }
    let top = max(scroller.minY, app.minY)
    let bottom = min(scroller.maxY, keyboard.minY)
    guard bottom - top >= leastVisibleBand else { return nil }
    let startY = max(top + (bottom - top) / 2, bottom - aboveKeyboard)
    let endY = min(keyboard.minY + keyboard.height * intoKeyboard, app.maxY - 1)
    let x = min(max(scroller.midX, app.minX + 1), app.maxX - 1)
    func normalised(_ y: CGFloat) -> CGPoint {
      CGPoint(x: (x - app.minX) / app.width, y: (y - app.minY) / app.height)
    }
    return Plan(start: normalised(startY), end: normalised(endY))
  }
}

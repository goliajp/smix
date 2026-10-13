import CoreGraphics

// Where hideKeyboard touches the app to let the keyboard go: above the
// keyboard, on nothing that acts on a touch. A touch there reaches whatever
// is under it, and right above the keyboard is where forms pin their submit
// button — measured on a sign-in form pinned to the keyboard layout guide, a
// touch 24pt above the keyboard pressed Continue and submitted the form
// before the flow's own step did.
public enum KeyboardOutsideTouch {
  static let aboveKeyboard: CGFloat = 24
  static let step: CGFloat = 8
  // the old fixed point, and above it the navigation bar and status bar,
  // where a touch goes back or scrolls the screen to its top
  static let highest: CGFloat = 0.15

  /// Normalised to the app frame, 0...1. nil when every point between the
  /// keyboard and the top band sits on something that acts on a touch.
  public static func point(keyboardTop: CGFloat, app: CGRect, actionable: [CGRect]) -> CGPoint? {
    guard app.width > 0, app.height > 0 else { return nil }
    let ceiling = app.minY + app.height * highest
    let xs = [0.5, 0.25, 0.75].map { app.minX + app.width * $0 }
    var y = keyboardTop - aboveKeyboard
    while y >= ceiling {
      for x in xs where !actionable.contains(where: { $0.contains(CGPoint(x: x, y: y)) }) {
        return CGPoint(x: (x - app.minX) / app.width, y: (y - app.minY) / app.height)
      }
      y -= step
    }
    return nil
  }
}

import CoreGraphics

// How far up the screen the keyboard reaches. XCUITest's keyboard element is
// the keys alone; the bar drawn above them (suggestions, the Passwords key
// of a sign-in field) is a sibling inside the same container, and a touch on
// it is a touch on the keyboard. Measured on iOS 27 against a sign-in field:
// keys from y=590, the bar and its Passwords button from y=546, both inside
// one container from 546 to the bottom of the screen. A touch aimed 24pt
// above the keys pressed Passwords; the keyboard left XCUITest's tree for
// about a second and came back, and the app's next tap was swallowed.
public enum KeyboardExtent {
  /// The top of the keyboard and everything drawn on it: the highest frame,
  /// among those of the keyboard's window, that holds the whole keyboard
  /// and is not the window itself.
  public static func top(keyboard: CGRect, window: CGRect, frames: [CGRect]) -> CGFloat {
    frames
      .filter { f in
        f.minX <= keyboard.minX && f.maxX >= keyboard.maxX
          && f.minY <= keyboard.minY && f.maxY >= keyboard.maxY
          && f.height < window.height
      }
      .map(\.minY)
      .reduce(keyboard.minY, min)
  }
}

import CoreGraphics

// The keyboard's return key, named when it is one hideKeyboard does not
// press. A field whose return key is Go or Search, and on whose screen
// nothing else ends editing, gives the keyboard up only through that key —
// a consumer's device search was one. hideKeyboard refuses there, and the
// refusal says which key it is, because that key is the field's submit.
//
// Found by place, not by a list of names: the return key is the keyboard's
// bottom-right button, and its identifier varies with its type ("Go",
// "Search", "Next:"). Measured on iOS 27 with a Go field: the keyboard's
// buttons are "shift" (y 705) and "Go" (x 300, y 759, the bottom row's
// right end); the other keys are keys, not buttons.
public enum KeyboardReturnKey {
  /// The identifier of the bottom-right button, or nil when there is none
  /// or it is one already pressed (`tried`).
  public static func untried(
    buttons: [(identifier: String, frame: CGRect)], tried: Set<String>
  ) -> String? {
    guard let key = buttons.max(by: { a, b in
      a.frame.maxY != b.frame.maxY ? a.frame.maxY < b.frame.maxY : a.frame.maxX < b.frame.maxX
    }), !key.identifier.isEmpty, !tried.contains(key.identifier) else { return nil }
    return key.identifier
  }
}

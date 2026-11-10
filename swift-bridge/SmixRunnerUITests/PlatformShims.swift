import Foundation

// tvOS and watchOS have no interface or device orientation: a watch is
// always upright and a television always landscape. The runner's touch
// and layout code is written against the iOS types, so on these two the
// types are given here with the cases the code reads, and the touch
// routes themselves are refused on them (see `PlatformSupport`).
#if os(tvOS) || os(watchOS)
@objc enum UIInterfaceOrientation: Int {
  case unknown = 0
  case portrait = 1
  case portraitUpsideDown = 2
  case landscapeRight = 3
  case landscapeLeft = 4
}

@objc enum UIDeviceOrientation: Int {
  case unknown = 0
  case portrait = 1
  case portraitUpsideDown = 2
  case landscapeLeft = 3
  case landscapeRight = 4
  case faceUp = 5
  case faceDown = 6
}
#endif

import XCTest

/// Act on `element` the way this platform acts on things: a touch where
/// there is one, and on a television — which takes no touch — the remote:
/// focus moved onto it, then select.
@discardableResult
func smixActivate(_ element: XCUIElement) -> Bool {
  #if os(tvOS)
  return SmixRemote.focusThenSelect(element)
  #else
  element.tap()
  return true
  #endif
}

#if os(tvOS)
/// Moving the focus with the remote.
enum SmixRemote {
  /// Presses before giving up on reaching an element. A screen of a few
  /// dozen focusable things is crossed in fewer; a target the focus engine
  /// never lands on is reported rather than chased.
  static let maxPresses = 40

  /// Move the focus onto `target`, a press at a time toward it, then press
  /// select. False when the focus stops moving or never arrives.
  static func focusThenSelect(_ target: XCUIElement) -> Bool {
    guard target.exists else { return false }
    let remote = XCUIRemote.shared
    var last: CGRect?
    for _ in 0..<maxPresses {
      if target.hasFocus {
        remote.press(.select)
        return true
      }
      guard let focused = focusedElement(), focused.exists else { return false }
      let from = focused.frame
      if let last, last == from { return false }
      last = from
      remote.press(direction(from: from, to: target.frame))
    }
    return false
  }

  /// The element holding the focus, anywhere in the front app.
  static func focusedElement() -> XCUIElement? {
    let app = XCUIApplication()
    let hit = app.descendants(matching: .any)
      .matching(NSPredicate(format: "hasFocus == true")).firstMatch
    return hit.exists ? hit : nil
  }

  /// The remote button that moves the focus from `from` toward `to`: the
  /// axis on which the two centres are further apart.
  static func direction(from: CGRect, to: CGRect) -> XCUIRemote.Button {
    let dx = to.midX - from.midX
    let dy = to.midY - from.midY
    if abs(dx) >= abs(dy) { return dx > 0 ? .right : .left }
    return dy > 0 ? .down : .up
  }
}
#endif

/// Whether this platform scrolls by swiping. A television scrolls by
/// moving the focus, and has no swipe to make.
#if os(tvOS)
let smixCanSwipe = false
#else
let smixCanSwipe = true
#endif

enum SmixSwipeDirection { case left, right, up, down }

/// A finger swipe on `element`; nothing on a platform that has none
/// (callers check `smixCanSwipe` first).
func smixSwipe(_ element: XCUIElement, _ direction: SmixSwipeDirection) {
  #if !os(tvOS)
  switch direction {
  case .left: element.swipeLeft()
  case .right: element.swipeRight()
  case .up: element.swipeUp()
  case .down: element.swipeDown()
  }
  #endif
}

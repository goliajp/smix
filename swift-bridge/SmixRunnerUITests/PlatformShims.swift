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

/// Remember the app the current request drives: on a television the
/// focus is read from it. Nothing on the other platforms.
func smixRemember(_ app: XCUIApplication) {
  #if os(tvOS)
  SmixRemote.lock.lock()
  SmixRemote.current = app
  SmixRemote.lock.unlock()
  #endif
}

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

  /// The app the latest request drove (see `smixRemember`).
  nonisolated(unsafe) static var current: XCUIApplication?
  static let lock = NSLock()

  /// Move the focus onto `target`, a press at a time toward it, then press
  /// select. False when the focus stops moving or never arrives.
  static func focusThenSelect(_ target: XCUIElement) -> Bool {
    guard target.exists else { return say("the target is gone") }
    let remote = XCUIRemote.shared
    var last: CGRect?
    for _ in 0..<maxPresses {
      if target.hasFocus {
        remote.press(.select)
        return true
      }
      guard let focused = focusedElement(), focused.exists else {
        return say("nothing holds the focus")
      }
      let from = focused.frame
      if let last, last == from {
        return say("the focus stayed at \(from) on the way to \(target.frame)")
      }
      last = from
      remote.press(direction(from: from, to: target.frame))
    }
    return say("the focus did not reach \(target.frame) in \(maxPresses) presses")
  }

  @discardableResult
  private static func say(_ why: String) -> Bool {
    FileHandle.standardError.write(Data("smix-runner: remote: \(why)\n".utf8))
    return false
  }

  /// The element a host aimed at by a normalised point: the smallest
  /// focusable element whose frame holds it. Focused and selected.
  static func activate(at norm: CGVector, in app: XCUIApplication) -> Bool {
    let frame = app.frame
    let point = CGPoint(
      x: frame.minX + norm.dx * frame.width, y: frame.minY + norm.dy * frame.height)
    let focusable: [XCUIElement.ElementType] = [
      .cell, .button, .link, .tab, .textField, .secureTextField, .searchField, .switch,
      .segmentedControl, .slider, .image, .other,
    ]
    var best: (XCUIElement, CGFloat)?
    for type in focusable {
      for e in app.descendants(matching: type).allElementsBoundByIndex {
        let f = e.frame
        // a container the size of the screen is not what was pointed at
        guard f.contains(point), f.width > 0, f.height > 0,
          f.width * f.height < frame.width * frame.height / 2
        else { continue }
        let area = f.width * f.height
        if best == nil || area < best!.1 { best = (e, area) }
      }
      if best != nil, type == .cell || type == .button { break }
    }
    guard let target = best?.0 else { return say("nothing focusable under \(point)") }
    let done = focusThenSelect(target)
    say("activate at \(point): \(target.elementType.rawValue) \(target.frame) → \(done)")
    return done
  }

  /// The element holding the focus, anywhere in the app being driven.
  static func focusedElement() -> XCUIElement? {
    lock.lock()
    let driven = current
    lock.unlock()
    guard let app = driven else { return nil }
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

/// Whether a node of the tree holds the focus.
///
/// On a television the focus is the focus engine's, the one thing the
/// remote moves, and each element reports whether it has it. Elsewhere it
/// is the keyboard's: the live first responder's identifier (one KVC walk
/// per tree) matched against this node's. A nil hint means nothing is
/// focused, and the identifier must be non-empty, so that a node without
/// one cannot match an empty string.
func smixHasFocus(_ reported: Any?, identifier: String, hint: String?) -> Bool {
  #if os(tvOS)
  return (reported as? Bool) ?? false
  #else
  return hint.map { !identifier.isEmpty && $0 == identifier } ?? false
  #endif
}

#if os(watchOS)
/// Where a swipe that brings `direction` into view starts and ends on a
/// watch, as shares of the screen. XCUI's own swipe on the app did nothing
/// toward the earlier rows of a list (watchOS 27.2: a swipe down left it
/// where it was, a drag from 30% to 80% of the height moved it back five
/// rows), so the finger is put down and lifted well inside the screen, as a
/// coordinate swipe is.
func smixWatchSwipeEnds(_ direction: String) -> (from: (Double, Double), to: (Double, Double))? {
  switch direction {
  case "down": return ((0.5, 0.8), (0.5, 0.3))
  case "up": return ((0.5, 0.3), (0.5, 0.8))
  case "left": return ((0.2, 0.5), (0.8, 0.5))
  case "right": return ((0.8, 0.5), (0.2, 0.5))
  default: return nil
  }
}
#endif

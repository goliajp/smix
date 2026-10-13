import CoreGraphics
import XCTest
@testable import SmixRunnerCore

// Where hideKeyboard touches outside the keyboard. A sign-in step on an
// iPhone 17 Pro, iOS 27, in points: the keyboard and its Passwords bar from
// 546, Continue pinned right above it, the email field and a Remember me
// checkbox higher up.
final class KeyboardOutsideTouchTests: XCTestCase {
  let app = CGRect(x: 0, y: 0, width: 402, height: 874)
  let keyboardTop: CGFloat = 546
  let continueButton = CGRect(x: 18, y: 502, width: 366, height: 44)
  let email = CGRect(x: 18, y: 220, width: 366, height: 44)
  let rememberMe = CGRect(x: 18, y: 276, width: 160, height: 30)

  func y(_ p: CGPoint) -> CGFloat { p.y * app.height }

  func test_with_nothing_there_it_touches_just_above_the_keyboard() throws {
    let p = try XCTUnwrap(KeyboardOutsideTouch.point(keyboardTop: keyboardTop, app: app, actionable: []))
    XCTAssertEqual(y(p), keyboardTop - 24, accuracy: 0.001)
    XCTAssertEqual(p.x, 0.5, accuracy: 0.0001)
  }

  func test_it_does_not_press_the_button_pinned_above_the_keyboard() throws {
    let p = try XCTUnwrap(KeyboardOutsideTouch.point(
      keyboardTop: keyboardTop, app: app, actionable: [continueButton, email, rememberMe]))
    let at = CGPoint(x: p.x * app.width, y: y(p))
    XCTAssertFalse(continueButton.contains(at), "a touch there submits the form")
    XCTAssertLessThan(y(p), continueButton.minY)
    XCTAssertGreaterThan(y(p), rememberMe.maxY, "the nearest free spot, not one far up the screen")
  }

  func test_a_control_across_the_middle_moves_the_touch_to_the_side() throws {
    let middle = CGRect(x: 150, y: 400, width: 100, height: 200)
    let p = try XCTUnwrap(KeyboardOutsideTouch.point(keyboardTop: keyboardTop, app: app, actionable: [middle]))
    XCTAssertEqual(y(p), keyboardTop - 24, accuracy: 0.001)
    XCTAssertEqual(p.x, 0.25, accuracy: 0.0001)
  }

  func test_with_no_free_spot_below_the_top_band_there_is_no_touch() {
    let wall = CGRect(x: 0, y: app.height * 0.15, width: 402, height: keyboardTop - app.height * 0.15)
    XCTAssertNil(KeyboardOutsideTouch.point(keyboardTop: keyboardTop, app: app, actionable: [wall]),
      "above the band are the back button and the status bar")
  }
}

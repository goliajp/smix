import CoreGraphics
import XCTest
@testable import SmixRunnerCore

// How far up the keyboard reaches. The frames are the keyboard window of a
// sign-in field read on an iPhone 17 Pro, iOS 27, in points: the window
// fills the screen, one container holds everything from 546 down, the
// Passwords bar is its top 44pt, the keys start at 590.
final class KeyboardExtentTests: XCTestCase {
  let window = CGRect(x: 0, y: 0, width: 402, height: 874)
  let keys = CGRect(x: 0, y: 590, width: 402, height: 226)
  let container = CGRect(x: 0, y: 546, width: 402, height: 328)
  let bar = CGRect(x: 0, y: 546, width: 402, height: 44)
  let passwords = CGRect(x: 30, y: 546, width: 342, height: 44)
  let key = CGRect(x: 4.67, y: 597, width: 39.3, height: 54)

  func test_the_bar_above_the_keys_is_part_of_the_keyboard() {
    let top = KeyboardExtent.top(
      keyboard: keys, window: window,
      frames: [window, window, container, bar, passwords, keys, key])
    XCTAssertEqual(top, 546, "a touch on the Passwords bar is a touch on the keyboard")
  }

  func test_the_window_itself_does_not_count() {
    // it holds the keyboard too, and would put the top at the status bar
    let top = KeyboardExtent.top(keyboard: keys, window: window, frames: [window, keys])
    XCTAssertEqual(top, 590)
  }

  func test_frames_that_do_not_hold_the_whole_keyboard_do_not_count() {
    // the bar sits above the keys and holds none of them; a key is inside
    let top = KeyboardExtent.top(keyboard: keys, window: window, frames: [bar, passwords, key])
    XCTAssertEqual(top, 590)
  }

  func test_with_nothing_drawn_above_the_keys_the_top_is_theirs() {
    let bare = CGRect(x: 0, y: 590, width: 402, height: 284)
    let top = KeyboardExtent.top(keyboard: keys, window: window, frames: [window, bare, keys])
    XCTAssertEqual(top, 590)
  }
}

import CoreGraphics
import XCTest
@testable import SmixRunnerCore

// Where hideKeyboard drags from and to, when the focused field sits in a
// scroll view that lets a drag take the keyboard down. A sign-in form on
// an iPhone 17 Pro in points: the app is 402x874, the keyboard's top is at
// 540, the form's scroll view fills the screen under a 100pt bar.
final class KeyboardDismissDragTests: XCTestCase {
  let app = CGRect(x: 0, y: 0, width: 402, height: 874)
  let keyboard = CGRect(x: 0, y: 540, width: 402, height: 334)
  let form = CGRect(x: 0, y: 100, width: 402, height: 774)
  let email = CGRect(x: 16, y: 200, width: 370, height: 34)

  func test_the_drag_starts_in_the_form_above_the_keyboard_and_ends_in_it() throws {
    let plan = try XCTUnwrap(KeyboardDismissDrag.plan(
      focused: email, scrollers: [form], keyboard: keyboard, app: app))
    let startY = plan.start.y * app.height
    let endY = plan.end.y * app.height
    XCTAssertGreaterThan(startY, form.minY, "the drag has to start inside the scroll view")
    XCTAssertLessThan(startY, keyboard.minY,
      "a drag that starts on the keyboard types: slide typing reads it as letters")
    XCTAssertGreaterThan(endY, keyboard.minY, "it has to end inside the keyboard")
    XCTAssertLessThan(endY, app.maxY)
    XCTAssertEqual(plan.start.x, plan.end.x, accuracy: 0.0001, "a straight drag down")
  }

  // Measured 2026-09-28 on an iPhone 17 Pro, iOS 27, on a refreshable form:
  // a fast drag from just above the keyboard to 0.80 of the screen took the
  // keyboard down and did not refresh; the same drag to 0.95 refreshed, and
  // so did the one from 0.40 to 0.95. So the drag ends in the keyboard's
  // upper half: far enough in for the keyboard to follow, short of the pull
  // that refreshes.
  func test_the_drag_ends_in_the_upper_half_of_the_keyboard() throws {
    let plan = try XCTUnwrap(KeyboardDismissDrag.plan(
      focused: email, scrollers: [form], keyboard: keyboard, app: app))
    let endY = plan.end.y * app.height
    XCTAssertGreaterThan(endY, keyboard.minY + keyboard.height * 0.3,
      "a shallow drag lets the keyboard spring back, and the touch lifts on a key")
    XCTAssertLessThanOrEqual(endY, keyboard.midY, "a deeper pull refreshes a list at its top")
  }

  // The keyboard frame XCUITest reports is the keys alone: on iOS 27 with
  // the pinyin keyboard it read 590..816 while the suggestion bar drawn
  // above the keys started at 544. A drag started 40pt above the frame
  // pressed a suggestion and typed it. So the drag starts clear of that bar.
  func test_the_drag_starts_clear_of_the_bar_above_the_keys() throws {
    let plan = try XCTUnwrap(KeyboardDismissDrag.plan(
      focused: email, scrollers: [form], keyboard: keyboard, app: app))
    let startY = plan.start.y * app.height
    XCTAssertLessThanOrEqual(startY, keyboard.minY - 60, "started at \(startY)")
    XCTAssertGreaterThanOrEqual(startY, keyboard.minY - 120, "started at \(startY)")
  }

  func test_the_innermost_scroll_view_around_the_field_is_the_one_dragged() throws {
    let inner = CGRect(x: 0, y: 150, width: 402, height: 300)
    let plan = try XCTUnwrap(KeyboardDismissDrag.plan(
      focused: email, scrollers: [form, inner], keyboard: keyboard, app: app))
    let startY = plan.start.y * app.height
    XCTAssertGreaterThan(startY, inner.minY)
    XCTAssertLessThan(startY, inner.maxY)
  }

  func test_no_scroll_view_around_the_field_means_no_drag() {
    // Showing above the keyboard, so only "does it hold the field" rules it out.
    let elsewhere = CGRect(x: 0, y: 300, width: 402, height: 200)
    XCTAssertNil(KeyboardDismissDrag.plan(
      focused: email, scrollers: [elsewhere], keyboard: keyboard, app: app))
    XCTAssertNil(KeyboardDismissDrag.plan(
      focused: email, scrollers: [], keyboard: keyboard, app: app))
  }

  func test_a_scroll_view_hidden_under_the_keyboard_is_not_dragged() {
    let under = CGRect(x: 0, y: 545, width: 402, height: 300)
    let field = CGRect(x: 16, y: 560, width: 370, height: 34)
    XCTAssertNil(KeyboardDismissDrag.plan(
      focused: field, scrollers: [under], keyboard: keyboard, app: app))
  }

  func test_an_empty_keyboard_frame_gives_nothing_to_aim_at() {
    XCTAssertNil(KeyboardDismissDrag.plan(
      focused: email, scrollers: [form], keyboard: .zero, app: app))
  }
}

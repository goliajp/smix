import XCTest
import FlyingFox
@testable import SmixRunnerCore

// HideKeyboardRoute POCO unit tests. Mirrors BackRouteTests
// (parameterless app-level capability). Does not exercise XCUITest — route
// owns only decode + envelope serialization.
//
// case I: decode empty body → request OK (hide-keyboard parameterless)
// case J: decode `{}` empty JSON object body → request OK
// case K: decode non-JSON body → DecodeError.invalidJSON
final class HideKeyboardRouteTests: XCTestCase {

  private func parse(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  // case I: empty body — hide-keyboard is parameterless
  func test_decode_empty_body_ok() throws {
    let body = Data()
    let req = try HideKeyboardRoute.decode(body)
    XCTAssertEqual(req, HideKeyboardRoute.HideKeyboardRequest())
  }

  // case J: empty JSON object `{}` body
  func test_decode_empty_object_ok() throws {
    let body = Data(#"{}"#.utf8)
    let req = try HideKeyboardRoute.decode(body)
    XCTAssertEqual(req, HideKeyboardRoute.HideKeyboardRequest())
  }

  // the host's budget is read, so the handler can stop before the host does
  func test_decode_reads_the_budget() throws {
    let req = try HideKeyboardRoute.decode(Data(#"{"budgetMs":20000}"#.utf8))
    XCTAssertEqual(req.budgetMs, 20000)
  }

  // a budget that is not a positive number is no budget, not a zero one
  func test_decode_ignores_a_budget_that_is_not_one() throws {
    XCTAssertNil(try HideKeyboardRoute.decode(Data(#"{"budgetMs":0}"#.utf8)).budgetMs)
    XCTAssertNil(try HideKeyboardRoute.decode(Data(#"{"budgetMs":"soon"}"#.utf8)).budgetMs)
  }

  // case K: not-JSON → DecodeError.invalidJSON
  func test_decode_non_json_throws() {
    let body = Data("not-json".utf8)
    XCTAssertThrowsError(try HideKeyboardRoute.decode(body)) { error in
      XCTAssertEqual(
        error as? HideKeyboardRoute.DecodeError,
        HideKeyboardRoute.DecodeError.invalidJSON
      )
    }
  }

  // bonus: success(ok:true) → 200 {"ok":true}
  func test_success_ok_true_serializes() async throws {
    let resp = HideKeyboardRoute.success(ok: true)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["ok"] as? Bool, true)
  }
}

// A failure that cannot say which failure it was.
//
// A consumer met `ok:false — the action did not happen` with the keyboard
// unmistakably on screen, and could not tell it from the answer they would
// have got if there had been no keyboard at all. Three different situations
// reached them as the same sentence: the strategies ran and the keyboard
// stayed, an XCUITest exception was caught, and the request context was
// lost. What a caller should do next differs in each.
//
// The typealias even documented one of them ("ok:false when smixGuarded
// caught an NSException") while the handler had a second path to false.
final class HideKeyboardOutcomeTests: XCTestCase {

  private func parse(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  func test_absent_keyboard_is_success() async throws {
    let json = try await parse(HideKeyboardRoute.outcome(.alreadyGone))
    XCTAssertEqual(json["ok"] as? Bool, true)
  }

  func test_dismissed_is_success() async throws {
    let json = try await parse(HideKeyboardRoute.outcome(.dismissed))
    XCTAssertEqual(json["ok"] as? Bool, true)
  }

  func test_still_present_says_what_was_tried() async throws {
    let json = try await parse(
      HideKeyboardRoute.outcome(.stillPresent(tried: "Return, tap-above, swipe-down")))
    XCTAssertEqual(json["ok"] as? Bool, false)
    XCTAssertEqual(json["error"] as? String, "keyboard_did_not_close")
    let saw = json["saw"] as? String ?? ""
    XCTAssertTrue(saw.contains("swipe-down"), "the caller needs to know what was attempted: \(saw)")
  }

  func test_could_not_tell_is_not_the_same_as_did_not_close() async throws {
    let json = try await parse(
      HideKeyboardRoute.outcome(.couldNotTell(why: "XCUITest raised mid-interaction")))
    XCTAssertEqual(json["ok"] as? Bool, false)
    XCTAssertEqual(json["error"] as? String, "keyboard_state_unknown",
                   "an exception is not evidence the keyboard is still up")
    XCTAssertNotEqual(json["error"] as? String, "keyboard_did_not_close")
  }

  func test_a_keyboard_below_the_screen_is_success_that_names_the_focus() async throws {
    let json = try await parse(HideKeyboardRoute.outcome(
      .offScreen(focus: "input-camera-name", seen: "at y=918, unmoved for 0.5 s")))
    XCTAssertEqual(json["ok"] as? Bool, true)
    let saw = json["saw"] as? String ?? ""
    XCTAssertTrue(saw.contains("below the screen") && saw.contains("input-camera-name"), saw)
    XCTAssertTrue(saw.contains("y=918"), "where it was seen travels with the answer: \(saw)")
  }

  func test_on_screen_is_any_overlap_with_the_app() {
    let app = CGRect(x: 0, y: 0, width: 402, height: 874)
    // the consumer's minimized keyboard
    XCTAssertFalse(HideKeyboardRoute.keyboardOnScreen(
      keyboard: CGRect(x: 0, y: 918, width: 402, height: 226), app: app))
    // flush with the bottom edge: touching is not showing
    XCTAssertFalse(HideKeyboardRoute.keyboardOnScreen(
      keyboard: CGRect(x: 0, y: 874, width: 402, height: 226), app: app))
    XCTAssertTrue(HideKeyboardRoute.keyboardOnScreen(
      keyboard: CGRect(x: 0, y: 538, width: 402, height: 336), app: app))
    XCTAssertTrue(HideKeyboardRoute.keyboardOnScreen(
      keyboard: CGRect(x: 0, y: 860, width: 402, height: 226), app: app))
    XCTAssertFalse(HideKeyboardRoute.keyboardOnScreen(keyboard: .zero, app: app))
  }

  private let below = CGRect(x: 0, y: 918, width: 402, height: 226)

  func test_a_keyboard_sliding_in_is_not_minimized() {
    // the consumer's case: off the screen at first, then on it
    let looks: [(at: TimeInterval, sighting: HideKeyboardRoute.KeyboardSighting)] = [
      (0.0, .offScreen(CGRect(x: 0, y: 874, width: 402, height: 226))),
      (0.1, .offScreen(CGRect(x: 0, y: 760, width: 402, height: 226))),
    ]
    XCTAssertNil(HideKeyboardRoute.offScreenVerdict(looks, hold: 0.5),
                 "a keyboard that moved has not settled anywhere")
    XCTAssertEqual(
      HideKeyboardRoute.offScreenVerdict(looks + [(0.2, .onScreen)], hold: 0.5), .onScreen)
  }

  func test_a_keyboard_that_stays_below_the_screen_is_minimized() {
    let looks: [(at: TimeInterval, sighting: HideKeyboardRoute.KeyboardSighting)] = [
      (0.0, .offScreen(below)), (0.3, .offScreen(below)),
    ]
    XCTAssertNil(HideKeyboardRoute.offScreenVerdict(looks, hold: 0.5), "not held long enough yet")
    XCTAssertEqual(
      HideKeyboardRoute.offScreenVerdict(looks + [(0.6, .offScreen(below))], hold: 0.5),
      .minimized(below))
  }

  func test_the_hold_counts_from_the_last_move() {
    let looks: [(at: TimeInterval, sighting: HideKeyboardRoute.KeyboardSighting)] = [
      (0.0, .offScreen(CGRect(x: 0, y: 1000, width: 402, height: 226))),
      (0.4, .offScreen(below)), (0.7, .offScreen(below)),
    ]
    XCTAssertNil(HideKeyboardRoute.offScreenVerdict(looks, hold: 0.5))
  }

  func test_a_keyboard_that_left_is_gone() {
    XCTAssertEqual(
      HideKeyboardRoute.offScreenVerdict([(0.0, .offScreen(below)), (0.1, .gone)], hold: 0.5),
      .gone)
  }
}

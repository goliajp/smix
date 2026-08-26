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
}

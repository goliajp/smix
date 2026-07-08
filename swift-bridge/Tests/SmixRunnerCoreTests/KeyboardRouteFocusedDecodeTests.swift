import XCTest
@testable import SmixRunnerCore

/// R4.e (audit Low-19) — KeyboardRoute.decodeFill / decodeClear must
/// accept BOTH the new explicit `{selector: {focused: true}}` wire
/// shape and the legacy `{selector: {text: "_focused_"}}` magic. The
/// new shape is what SDK `app.focused.fill / .clear` posts under R4.e;
/// the legacy form is what existing v1.2 driver Path B fill/clear
/// still emits (backward-compat anchor). Both resolve to the same
/// internal `Selector(text: "_focused_")` so handler hot paths
/// (KeyboardCache + focus-tap skip) are byte-identical.
final class KeyboardRouteFocusedDecodeTests: XCTestCase {
  func test_fillDecodesFocusedTrueBaseForm_asUnderscoreFocusedUnderscore() throws {
    let body = Data(
      #"{"selector":{"focused":true},"text":"hello"}"#.utf8
    )
    let req = try KeyboardRoute.decodeFill(body)
    XCTAssertEqual(req.selector.text, "_focused_")
    XCTAssertEqual(req.text, "hello")
  }

  func test_fillDecodesLegacyMagicText_unchanged() throws {
    // Backward-compat anchor: existing driver Path B still posts
    // `{text:"_focused_"}` (R4.e kept wire byte-identical to avoid
    // regression). Must decode to the same internal selector.
    let body = Data(
      #"{"selector":{"text":"_focused_"},"text":"hello"}"#.utf8
    )
    let req = try KeyboardRoute.decodeFill(body)
    XCTAssertEqual(req.selector.text, "_focused_")
  }

  func test_clearDecodesFocusedTrueBaseForm() throws {
    let body = Data(
      #"{"selector":{"focused":true}}"#.utf8
    )
    let req = try KeyboardRoute.decodeClear(body)
    XCTAssertEqual(req.selector.text, "_focused_")
  }

  func test_focusedFalseIgnored_fallsThroughToText() throws {
    // focused: false is NOT a valid R4.e form. Decoder falls through
    // to the legacy text path; missing text → DecodeError.missingText.
    let body = Data(
      #"{"selector":{"focused":false},"text":"x"}"#.utf8
    )
    XCTAssertThrowsError(try KeyboardRoute.decodeFill(body)) { err in
      XCTAssertEqual(err as? KeyboardRoute.DecodeError, .missingText)
    }
  }
}

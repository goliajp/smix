import XCTest
@testable import SmixRunnerCore

// A fill must not report success it has no evidence for.
//
// `inputText: "..."` (the scalar form) targets `_focused_`. The runner
// skipped focus resolution for that selector *whatever the dispatch mode
// said* — the condition read
//
//     let resolveFocus = dispatch != .keyEvents
//     if resolveFocus && !selectorText.isEmpty && selectorText != "_focused_"
//
// so the documented default mode (`a11y` — "resolve focus via a11y tree")
// never applied to it, and `--force-key-events` was a no-op for it. The
// daemon send then happened unconditionally and answered ok.
//
// Measured on 2026-08-25, iOS fixture, nothing focused: ok, 18 chars, 0
// warnings — and the tree said `value=None` while the screenshot still
// showed the placeholder. Android answers ELEMENT_NOT_FOUND for the same
// flow, which is the right answer and the wrong half of a pair.
//
// The escape hatch is not being removed; it is being made the explicit
// one it was already documented as. `--force-key-events` says "skip a11y
// focus resolution", and that is now the only way to get that.
final class FillFocusPolicyTests: XCTestCase {
  func testNothingFocusedAndNoKeyboardIsRefused() {
    XCTAssertEqual(
      FillFocusPolicy.decide(dispatch: .a11y, isFocusedSelector: true,
                             somethingFocused: false, keyboardUp: false),
      .refuse
    )
  }

  func testAKeyboardIsEnoughEvidenceToProceed() {
    // The RN hidden-input case: the a11y tree cannot address the field,
    // so nothing reads as focused — but the keyboard being up means
    // something took it, and the characters have somewhere to go.
    XCTAssertEqual(
      FillFocusPolicy.decide(dispatch: .a11y, isFocusedSelector: true,
                             somethingFocused: false, keyboardUp: true),
      .proceed
    )
  }

  func testFocusIsEnoughEvidenceToProceed() {
    XCTAssertEqual(
      FillFocusPolicy.decide(dispatch: .a11y, isFocusedSelector: true,
                             somethingFocused: true, keyboardUp: false),
      .proceed
    )
  }

  func testForceKeyEventsProceedsWithNoEvidenceAtAll() {
    // The whole point of the mode, and the reason refusing by default
    // costs nobody the case it was protecting.
    XCTAssertEqual(
      FillFocusPolicy.decide(dispatch: .keyEvents, isFocusedSelector: true,
                             somethingFocused: false, keyboardUp: false),
      .proceed
    )
  }

  func testANamedSelectorIsNotThisPolicysBusiness() {
    // A named field resolves and taps through the existing path; this
    // policy only governs the form that names nothing.
    XCTAssertEqual(
      FillFocusPolicy.decide(dispatch: .a11y, isFocusedSelector: false,
                             somethingFocused: false, keyboardUp: false),
      .proceed
    )
  }
}

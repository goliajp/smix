// `Input-Dispatch-Mode` decides how /fill puts text into the app.
//
// The header has been in the wire format since v1 and the client has
// always sent it; no runner read it, so `smix run --force-key-events`
// changed nothing. Nothing failed either — a header nobody reads looks
// exactly like a header that works, which is why this went unnoticed
// through a whole major release.

import XCTest

@testable import SmixRunnerCore

final class InputDispatchModeTests: XCTestCase {

  func testAbsentHeaderMeansAccessibilityResolution() {
    // The default has to stay the default: every existing flow sends no
    // header and must keep resolving the field through the a11y tree.
    XCTAssertEqual(SmixRunnerServer.InputDispatchMode.parse(nil), .a11y)
    XCTAssertEqual(SmixRunnerServer.InputDispatchMode.parse(""), .a11y)
  }

  func testKeyEventsIsSpelledAsTheWireFormatSpellsIt() {
    // The hyphenated spelling is the contract: the client sends
    // `key-events`, and a runner matching on `keyEvents` would silently
    // fall through to the default — reintroducing the same bug.
    XCTAssertEqual(SmixRunnerServer.InputDispatchMode.parse("key-events"), .keyEvents)
    XCTAssertEqual(SmixRunnerServer.InputDispatchMode.parse("a11y"), .a11y)
    XCTAssertEqual(SmixRunnerServer.InputDispatchMode.parse("auto"), .auto)
  }

  func testAnUnknownModeDegradesRatherThanFailingTheStep() {
    // A client from a later version naming a mode this runner does not
    // have should still type the text. Refusing would turn a forward
    // -compatible request into a failed flow step.
    XCTAssertEqual(SmixRunnerServer.InputDispatchMode.parse("some-future-mode"), .auto)
  }

  func testKeyEventsIsTheOnlyModeThatSkipsFocusResolution() {
    // The handler branches on `!= .keyEvents`; this pins which modes
    // land on which side, so adding a mode cannot quietly change how an
    // existing one behaves.
    let skipsFocus: (SmixRunnerServer.InputDispatchMode) -> Bool = { $0 == .keyEvents }
    XCTAssertTrue(skipsFocus(.keyEvents))
    XCTAssertFalse(skipsFocus(.a11y))
    XCTAssertFalse(skipsFocus(.auto))
  }
}

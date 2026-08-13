import XCTest

@testable import SmixRunnerCore

/// Which reading of the screen an ocrText needle meant.
///
/// The rule used to be "the first observation containing the needle, in
/// the recognizer's order". iOS's notification alert reads `Don't Allow`
/// and `Allow`; an ocrText of "Allow" tapped Don't Allow, reported
/// success, and iOS does not ask a second time — the app had to be
/// reinstalled before that flow could be tried again.
final class OcrPickTests: XCTestCase {
  /// The case that cost a consumer a verification path.
  func testAnExactReadingBeatsALongerOneContainingIt() {
    let screen = ["Don't Allow", "Allow"]
    XCTAssertEqual(FindTextByOcrRoute.pick(needle: "Allow", from: screen), 1)
    XCTAssertEqual(FindTextByOcrRoute.pick(needle: "Don't Allow", from: screen), 0)
  }

  /// Order must not decide it either way round.
  func testOrderDoesNotDecide() {
    XCTAssertEqual(FindTextByOcrRoute.pick(needle: "Allow", from: ["Allow", "Don't Allow"]), 0)
    XCTAssertEqual(FindTextByOcrRoute.pick(needle: "Allow", from: ["Don't Allow", "Allow"]), 1)
  }

  /// Two readings that both merely contain it: a coin flip the caller
  /// never sees, so nothing is chosen.
  func testAmbiguityIsRefusedRatherThanResolved() {
    XCTAssertNil(FindTextByOcrRoute.pick(needle: "Save", from: ["Save Draft", "Save As…"]))
  }

  /// And a single containing reading is still taken — refusing
  /// everything would be as useless as choosing wrongly.
  func testOneContainingReadingIsStillTaken() {
    XCTAssertEqual(FindTextByOcrRoute.pick(needle: "Save", from: ["Save Draft", "Cancel"]), 0)
  }

  /// Case and surrounding space are the recognizer's, not the caller's.
  func testCaseAndSpaceDoNotDecide() {
    XCTAssertEqual(FindTextByOcrRoute.pick(needle: "allow", from: [" Allow "]), 0)
  }

  /// Nothing on screen is nothing found.
  func testNoReadingIsNil() {
    XCTAssertNil(FindTextByOcrRoute.pick(needle: "Allow", from: ["Cancel", "OK"]))
    XCTAssertNil(FindTextByOcrRoute.pick(needle: "Allow", from: []))
  }
}

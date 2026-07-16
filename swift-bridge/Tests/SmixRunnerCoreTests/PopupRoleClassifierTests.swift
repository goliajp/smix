import XCTest

@testable import SmixRunnerCore

// Semantic lock for popup button role classification as a pure function.
// Role used to be resolved with two live predicates per button
// (label == %@ AND userTestingAttributes CONTAINS "cancel-button" /
// "destructive"), which cost two round-trips per button. Instead, the
// consume path now runs the two attribute-only queries once, without the
// label constraint, collecting the cancelLabels / destructiveLabels sets;
// the per-button loop is a plain in-memory comparison. This file locks the
// three-way verdict, which stays equivalent to the per-button predicates.
final class PopupRoleClassifierTests: XCTestCase {
  func test_cancelLabel_returnsCancel() {
    XCTAssertEqual(
      classifyPopupButtonRole(
        label: "Cancel", cancelLabels: ["Cancel"], destructiveLabels: []),
      "cancel")
  }

  func test_destructiveLabel_returnsDestructive() {
    XCTAssertEqual(
      classifyPopupButtonRole(
        label: "Log out", cancelLabels: ["Cancel"], destructiveLabels: ["Log out"]),
      "destructive")
  }

  func test_neitherSet_returnsDefault() {
    XCTAssertEqual(
      classifyPopupButtonRole(
        label: "OK", cancelLabels: ["Cancel"], destructiveLabels: ["Log out"]),
      "default")
  }

  // Priority: a cancel match wins over a destructive one — the cancel
  // predicate was queried first and returned first.
  func test_inBothSets_cancelWins() {
    XCTAssertEqual(
      classifyPopupButtonRole(
        label: "X", cancelLabels: ["X"], destructiveLabels: ["X"]),
      "cancel")
  }

  func test_emptySets_returnsDefault() {
    XCTAssertEqual(
      classifyPopupButtonRole(label: "Anything", cancelLabels: [], destructiveLabels: []),
      "default")
  }
}

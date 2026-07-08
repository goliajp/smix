import XCTest

@testable import SmixRunnerCore

// C-fix — role 分类去 per-button live query 后的纯函数语义锁。旧逻辑：
// 每 button 跑 2 次 live predicate (label==%@ AND userTestingAttributes
// CONTAINS "cancel-button" / "destructive")，cancel 命中优先 destructive，
// 都不命中为 default。新逻辑：consume 开头一次性两个去 label 约束的
// attribute-only query 收 cancelLabels / destructiveLabels Set，button
// 循环改内存比对。本 file 锁纯函数三态判定与旧 per-button 等价。
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

  // Priority: cancel 命中优先于 destructive (旧逻辑 cancelPred 先查先返回)。
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

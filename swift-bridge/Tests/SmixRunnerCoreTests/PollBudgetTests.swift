import Foundation
import SmixRunnerCore
import XCTest

final class PollBudgetTests: XCTestCase {
  func testALookBegunInsideTheBudgetDoesNotEndTheWaitHoweverLateItReturns() {
    let start = Date(timeIntervalSince1970: 1000)
    let budget = PollBudget(limit: 2.0, start: start)
    // Begun at 0.06 s; that it came back at 2.1 s is not its business.
    XCTAssertFalse(budget.spent(by: start.addingTimeInterval(0.06)))
    XCTAssertFalse(budget.spent(by: start.addingTimeInterval(1.999)))
  }

  func testALookBegunAtOrAfterTheLimitEndsIt() {
    let start = Date(timeIntervalSince1970: 1000)
    let budget = PollBudget(limit: 2.0, start: start)
    XCTAssertTrue(budget.spent(by: start.addingTimeInterval(2.0)))
    XCTAssertTrue(budget.spent(by: start.addingTimeInterval(2.11)))
  }
}

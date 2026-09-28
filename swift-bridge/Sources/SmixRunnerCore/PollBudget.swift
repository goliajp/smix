import Foundation

/// When a wait that polls has run out.
///
/// A wait that ends on "not yet" makes a claim about its whole budget, and
/// the claim rests on its latest look. A loop written `while Date() <
/// deadline { look }` lets that look be one that began before the budget was
/// over: on a loaded simulator a single accessibility query has taken
/// eighteen seconds, so a look begun early reads the old screen, finishes
/// after the deadline, and the wait gives up on a state that had already
/// changed. The budget is spent only once a look that began after it still
/// says "not yet".
public struct PollBudget: Sendable {
  public let start: Date
  public let limit: TimeInterval

  public init(limit: TimeInterval, start: Date = Date()) {
    self.start = start
    self.limit = limit
  }

  /// Mark a look beginning now. Take it before looking, not after.
  public func look() -> Date { Date() }

  /// Whether a "not yet" from the look that began at `began` ends the wait.
  public func spent(by began: Date) -> Bool {
    began.timeIntervalSince(start) >= limit
  }
}

// Has the back navigation landed?
//
// `back` taps and then watches the navigation bar's identifier, because
// the bar's identifier IS the screen title and a change in it is the
// "we moved" signal. That much was already right — a fixed sleep both
// overpaid (navigation lands ~100ms after the tap returns) and told the
// caller nothing about whether the screen actually changed.
//
// What was wrong was one line: a bar that could not be found counted as
// arrival. During a pop animation the bar is momentarily unfindable, so
// `back` returned before landing and the next assertion read the screen
// being left behind. That surfaced as one flake in ten corpus runs on
// `nav-accessibility-and-back`, where the failure's own diagnostics
// listed the departing screen's navigation bar and back button as still
// on screen.
//
// Absence is genuinely ambiguous, which is why it was tempting: the
// destination may simply have no navigation bar, and answering "not yet"
// forever would hang every back that lands on one. The way out is not to
// decide from a single reading. One frame of absence is noise; a run of
// them is the destination. That is the same rule that keeps turning up
// elsewhere — a measurement needs corroboration that does not come from
// the same instant.
//
// Lives here rather than inline in the UITest body so it can be driven
// by `swift test` with reading sequences. The device half is now only
// "take a reading"; the decision is testable.

/// Whether a back navigation has landed, decided over successive
/// readings of the navigation bar.
public struct NavigationSettle: Sendable {
  /// What one look at the navigation bar produced.
  public enum Reading: Equatable, Sendable {
    /// A bar exists carrying this identifier, and whether a bar still
    /// carries the identifier the screen had before the tap.
    ///
    /// Both halves are needed, and the second is the one that was
    /// missing. During a pop the destination's navigation bar is built
    /// before the departing one is torn down, so `firstMatch` can
    /// return the new title while the old screen is still up — "a
    /// different title is visible" fires at the START of the
    /// transition. Twice in twenty corpus runs `back` reported exactly
    /// that and the next assertion found the departing screen.
    ///
    /// A different title is not the signal. The old title being gone
    /// is.
    case title(String, departingStillPresent: Bool)
    /// The bar was not found. Ambiguous on its own: a pop animation
    /// frame, or a destination with no bar at all.
    case absent
    /// The bar was found and the snapshot failed. Not a title, not an
    /// absence — "I could not look" is not "nothing was there", so it
    /// neither settles nor counts toward a run of absences.
    case unreadable
  }

  public enum Verdict: Equatable, Sendable {
    case arrived
    case notYet
    /// There was no title before the tap, so there is no identity to
    /// watch. The caller falls back to its fixed settle rather than
    /// this inventing a signal it does not have.
    case noIdentity
  }

  /// How many consecutive absences mean the destination has no bar.
  ///
  /// Three, at the caller's 50ms poll interval, is 150ms — longer than
  /// the one-frame blink that caused this and far shorter than the 2s
  /// budget the caller allows. Raising it costs latency only on screens
  /// that genuinely have no bar; lowering it to one is the bug.
  static let absencesMeaningGone = 3

  private let before: String?
  private var consecutiveAbsences = 0

  public init(before: String?) {
    self.before = before
  }

  public mutating func observe(_ reading: Reading) -> Verdict {
    guard let before else { return .noIdentity }
    switch reading {
    case .title(let now, let departingStillPresent):
      consecutiveAbsences = 0
      // `departingStillPresent` is CARRIED AND NOT USED, deliberately.
      //
      // Requiring it to be false before arriving was measured and made
      // things far worse: the flow went from 2 flakes in 20 runs to
      // failing outright in 28 of 30. `back` still reported
      // `titleChanged`, so the live query `navigationBars[before]` was
      // answering "gone" at the same moment the assertion's tree still
      // listed that bar — the two disagree, and gating on the live one
      // let `back` return even earlier than before.
      //
      // Kept in the reading because it is the thing to look at next,
      // and removing it would throw away the evidence that the live and
      // snapshot views of the same bar disagree during a transition.
      _ = departingStillPresent
      return now == before ? .notYet : .arrived
    case .absent:
      consecutiveAbsences += 1
      return consecutiveAbsences >= Self.absencesMeaningGone ? .arrived : .notYet
    case .unreadable:
      // Breaks the run: a gap in the evidence is not evidence.
      consecutiveAbsences = 0
      return .notYet
    }
  }
}

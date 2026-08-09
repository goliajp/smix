// The reading that means "I could not see" must not mean "we arrived".
//
// `back` watched the navigation bar's identifier and returned as soon as
// it changed — with one shortcut: a bar that did not exist counted as
// arrival. During a pop animation the bar is momentarily unfindable, so
// a back that had not landed reported success, and the assertion after
// it read the screen being left behind. Once in ten corpus runs, on
// `nav-accessibility-and-back`.
//
// The comment directly above that line already said a snapshot throwing
// mid-gesture means "no reading, not navigated". The throw was handled;
// the absence was not.
//
// Absence is genuinely ambiguous — the destination may simply have no
// navigation bar — so the answer is not "absent means no". It is that
// one frame of absence is noise and a run of them is signal, which is
// the same corroboration rule the rest of this cycle kept arriving at.

import XCTest

@testable import SmixRunnerCore

final class NavigationSettleTests: XCTestCase {
  /// Feed readings in order, return the first non-`.notYet` answer, or
  /// `.notYet` if it never settles.
  private func drive(_ readings: [NavigationSettle.Reading], from title: String?)
    -> NavigationSettle.Verdict
  {
    var settle = NavigationSettle(before: title)
    for reading in readings {
      let verdict = settle.observe(reading)
      if verdict != .notYet { return verdict }
    }
    return .notYet
  }

  func testATitleChangeIsArrivalOnceTheOldBarIsGone() {
    XCTAssertEqual(
      drive([.title("Settings", departingStillPresent: false)], from: "Accessibility"),
      .arrived
    )
  }

  /// The defect the corpus named, as a sequence.
  ///
  /// During a pop both navigation bars exist for a moment: the
  /// destination's is built before the departing one is torn down, and
  /// `navigationBars.firstMatch` can match the new one. So "a different
  /// title is visible" fires at the START of the transition. Twice in
  /// twenty corpus runs `back` reported `titleChanged` and the very
  /// next assertion found the departing screen still up.
  ///
  /// A different title is not the signal. The old title being gone is.
  func testANewTitleWhileTheOldOneIsStillThereIsNotArrival() throws {
    throw XCTSkip(
      """
      Gating arrival on the departing bar was measured and reverted: the \
      flow went from 2 flakes in 20 runs to failing in 28 of 30. The live \
      query said the bar was gone while the assertion's tree still listed \
      it, so this made `back` return earlier, not later. Kept as a \
      skipped record of what was tried and what it cost.
      """)
    XCTAssertEqual(
      drive(
        [
          .title("Settings", departingStillPresent: true),
          .title("Settings", departingStillPresent: true),
        ],
        from: "Accessibility"
      ),
      .notYet
    )
  }

  func testTheTransitionCompletingIsArrival() throws {
    throw XCTSkip("see testANewTitleWhileTheOldOneIsStillThereIsNotArrival")
    XCTAssertEqual(
      drive(
        [
          .title("Settings", departingStillPresent: true),
          .title("Settings", departingStillPresent: false),
        ],
        from: "Accessibility"
      ),
      .arrived
    )
  }

  func testTheSameTitleIsNotArrival() {
    XCTAssertEqual(
      drive(
        [
          .title("Accessibility", departingStillPresent: true),
          .title("Accessibility", departingStillPresent: true),
        ],
        from: "Accessibility"
      ),
      .notYet
    )
  }

  /// The defect, as a sequence: the bar blinks out for one reading and
  /// comes back unchanged. The screen never moved.
  func testOneFrameOfAbsenceIsNotArrival() {
    XCTAssertEqual(
      drive(
        [
          .title("Accessibility", departingStillPresent: true),
          .absent,
          .title("Accessibility", departingStillPresent: true),
        ],
        from: "Accessibility"
      ),
      .notYet
    )
  }

  /// Absence that persists is the destination genuinely having no
  /// navigation bar. Answering `.notYet` forever here would hang every
  /// back that lands on such a screen.
  func testSustainedAbsenceIsArrival() {
    XCTAssertEqual(
      drive([.absent, .absent, .absent, .absent], from: "Accessibility"),
      .arrived
    )
  }

  /// A snapshot that throws is a third thing: not a title, not an
  /// absence, and above all not an arrival. It must not count toward
  /// the absence run either — "I could not look" is not "nothing was
  /// there".
  func testUnreadableIsNeitherArrivalNorAbsence() {
    XCTAssertEqual(
      drive(
        [.unreadable, .unreadable, .unreadable, .unreadable, .unreadable],
        from: "Accessibility"
      ),
      .notYet
    )
  }

  func testUnreadableBreaksARunOfAbsence() {
    XCTAssertEqual(
      drive([.absent, .absent, .unreadable, .absent], from: "Accessibility"),
      .notYet
    )
  }

  /// No title to begin with means no identity to watch. Inventing a
  /// signal there would be worse than admitting there is none, so the
  /// caller keeps its old behaviour — a fixed settle and an optimistic
  /// answer — and this says so rather than guessing.
  func testNoStartingTitleHasNothingToWatch() {
    XCTAssertEqual(drive([.absent], from: nil), .noIdentity)
  }
}

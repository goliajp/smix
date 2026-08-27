import XCTest

@testable import SmixRunnerCore

/// Being in the tree and being touchable are two different facts.
///
/// A SwiftUI modal does not remove what is behind it from the accessibility
/// tree, and a touch aimed there is swallowed by the presentation. Measured
/// on the fixture: with an alert open, tapping `fixture-submit` behind it
/// exited 0 while the app did not move; closing the alert made the same tap
/// work. smix reported success for something a user could not reach.
///
/// Answered structurally rather than by asking each element. `isHittable`
/// is a live query and under a modal those cost about a second apiece —
/// which is why this file reads a snapshot at all. The OS rule is exactly
/// "what is not inside the modal cannot be touched", so it is asked once
/// about the shape instead of once per node about the same fact.
final class HittabilityUnderAModalTests: XCTestCase {

  private func node(
    _ type: UInt,
    _ identifier: String,
    _ children: [TreeRoute.A11ySnapshotData] = []
  ) -> TreeRoute.A11ySnapshotData {
    TreeRoute.A11ySnapshotData(
      elementTypeRawValue: type, identifier: identifier, label: identifier,
      value: nil, frame: CGRect(x: 0, y: 0, width: 100, height: 40),
      isEnabled: true, isSelected: false, children: children)
  }

  private func find(_ dict: [String: Any], _ id: String) -> [String: Any]? {
    if dict["identifier"] as? String == id { return dict }
    for child in (dict["children"] as? [[String: Any]] ?? []) {
      if let hit = find(child, id) { return hit }
    }
    return nil
  }

  private func serialise(_ root: TreeRoute.A11ySnapshotData) -> [String: Any] {
    var truncated = false
    return TreeRoute.nodeToDictForTesting(
      root, rootFrame: CGRect(x: 0, y: 0, width: 400, height: 800),
      truncated: &truncated)
  }

  func testWithNoModalNobodySaysAnything() {
    // Absence is the answer when the question is uninteresting. Saying
    // `true` everywhere would turn a field meaning "asked and no" into one
    // meaning "asked", and the reader downstream tells those apart.
    let tree = node(1, "root", [node(9, "submit")])
    let dict = serialise(tree)
    XCTAssertNil(
      find(dict, "submit")?["hittable"],
      "hittable was stated with nothing covering the screen")
  }

  func testUnderAModalWhatIsBehindItSaysNo() {
    let tree = node(1, "root", [
      node(9, "submit"),
      node(7, "the-alert", [node(9, "confirm")]),
    ])
    let dict = serialise(tree)
    XCTAssertEqual(
      find(dict, "submit")?["hittable"] as? Bool, false,
      "a control behind the alert did not say it was unreachable")
  }

  func testUnderAModalWhatIsInsideItSaysYes() {
    // The paired half. A rule that answered `false` everywhere would pass
    // the test above and make the modal itself undrivable.
    let tree = node(1, "root", [
      node(9, "submit"),
      node(7, "the-alert", [node(9, "confirm")]),
    ])
    let dict = serialise(tree)
    XCTAssertEqual(
      find(dict, "confirm")?["hittable"] as? Bool, true,
      "the alert's own button was reported unreachable")
  }

  func testASheetCountsAndSoDoesADialog() {
    for type in [UInt(5), UInt(8)] {
      let tree = node(1, "root", [node(9, "behind"), node(type, "modal")])
      XCTAssertEqual(
        find(serialise(tree), "behind")?["hittable"] as? Bool, false,
        "element type \(type) was not treated as a modal")
    }
  }

  func testAModalNestedDeeplyIsStillFound() {
    // The walk meets `behind` before it meets the alert, so the presence
    // question has to be answered before the walk rather than during it.
    let tree = node(1, "root", [
      node(9, "behind"),
      node(1, "wrapper", [node(1, "deeper", [node(7, "alert")])]),
    ])
    XCTAssertEqual(
      find(serialise(tree), "behind")?["hittable"] as? Bool, false,
      "a modal below the first level was missed")
  }
}

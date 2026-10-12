import CoreGraphics
import XCTest

@testable import SmixRunnerCore

final class AimCheckTests: XCTestCase {
  private let target = TapAtCoordRoute.AimTarget(
    identifier: "btn-confirm", label: "Delete", frame: CGRect(x: 10, y: 10, width: 100, height: 40))
  private func entry(_ id: String, _ label: String = "") -> HitChainEntry {
    HitChainEntry(identifier: id, label: label, frame: CGRect(x: 0, y: 0, width: 400, height: 800))
  }

  func testATargetStillUnderThePointIsTouched() {
    XCTAssertNil(AimCheck.gone(target: target, chain: [entry("btn-confirm"), entry("sheet")], readable: true))
  }

  func testATargetGoneFromThePointIsNotTouchedAndWhatIsThereIsNamed() {
    XCTAssertEqual(
      AimCheck.gone(target: target, chain: [entry("scrim"), entry("", "Close sheet")], readable: true),
      "id=scrim < \"Close sheet\"")
  }

  func testAPointThatCouldNotBeReadLetsTheTouchGoIn() {
    XCTAssertNil(AimCheck.gone(target: target, chain: [], readable: false))
  }

  // The chain lists named elements only: an unnamed target cannot be
  // looked for in it.
  func testAnUnnamedTargetIsTouchedAsBefore() {
    let unnamed = TapAtCoordRoute.AimTarget(identifier: "", label: "", frame: .zero)
    XCTAssertNil(AimCheck.gone(target: unnamed, chain: [entry("scrim")], readable: true))
  }

  func testTheTargetIsReadFromTheRequestAndAnOlderHostSendsNone() throws {
    let with = try TapAtCoordRoute.decode(Data(
      #"{"nx":0.5,"ny":0.5,"aimedAt":{"identifier":"a","label":"A","frame":{"x":1,"y":2,"w":3,"h":4}}}"#.utf8))
    XCTAssertEqual(
      with.aimedAt,
      TapAtCoordRoute.AimTarget(identifier: "a", label: "A", frame: CGRect(x: 1, y: 2, width: 3, height: 4)))
    XCTAssertNil(try TapAtCoordRoute.decode(Data(#"{"nx":0.5,"ny":0.5}"#.utf8)).aimedAt)
  }

  func testTheRefusalIsAConflictTheHostRecognises() {
    let r = TapAtCoordRoute.targetGone(saw: "id=scrim")
    XCTAssertEqual(r.statusCode.code, 409)
  }
}

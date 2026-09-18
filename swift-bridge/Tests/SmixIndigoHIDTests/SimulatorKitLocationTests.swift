import XCTest
@testable import SmixIndigoHID

/// Where SimulatorKit is looked for, and in what order. Xcode 27 moved it
/// from `Contents/Developer/Library/PrivateFrameworks/` to
/// `Contents/SharedFrameworks/` and removed the old directory outright;
/// Xcode <= 26 has only the old one. Both are derived from the developer
/// dir, and the order is the decision — so the whole array is asserted,
/// not membership.
private final class RecordingResolver: DlsymResolver {
  static let handle = UnsafeMutableRawPointer(bitPattern: 0xBEEF_CAFE)!
  let openable: Set<String>
  var opened: [String] = []
  var error = "fake: no error"

  init(openable: Set<String>) { self.openable = openable }

  func open(_ path: String) -> UnsafeMutableRawPointer? {
    opened.append(path)
    if openable.contains(path) { return Self.handle }
    error = "fake: cannot open \(path)"
    return nil
  }

  func sym(_ handle: UnsafeMutableRawPointer, _ name: String) -> UnsafeMutableRawPointer? { nil }
  func lastErrorDescription() -> String { error }
}

final class SimulatorKitLocationTests: XCTestCase {
  let dev = "/X/Contents/Developer"
  let shared = "/X/Contents/SharedFrameworks/SimulatorKit.framework/SimulatorKit"
  let legacy = "/X/Contents/Developer/Library/PrivateFrameworks/SimulatorKit.framework/SimulatorKit"

  func testCandidatesAreExactlyTheTwoLayoutsNewestFirst() {
    XCTAssertEqual(CoreSimulatorBridge.simulatorKitCandidates(developerDir: dev), [shared, legacy])
  }

  func testXcode27LayoutOpensOnTheFirstTry() throws {
    let resolver = RecordingResolver(openable: [shared])
    let handle = try CoreSimulatorBridge.openSimulatorKit(developerDir: dev, via: resolver)
    XCTAssertEqual(handle, RecordingResolver.handle)
    XCTAssertEqual(resolver.opened, [shared])
  }

  func testXcode26LayoutOpensOnTheSecondTry() throws {
    let resolver = RecordingResolver(openable: [legacy])
    let handle = try CoreSimulatorBridge.openSimulatorKit(developerDir: dev, via: resolver)
    XCTAssertEqual(handle, RecordingResolver.handle)
    XCTAssertEqual(resolver.opened, [shared, legacy])
  }

  func testNeitherLayoutNamesBothPathsInTheError() {
    let resolver = RecordingResolver(openable: [])
    XCTAssertThrowsError(try CoreSimulatorBridge.openSimulatorKit(developerDir: dev, via: resolver)) { err in
      guard case let HostHIDError.dlopenFailed(path, detail) = err else {
        return XCTFail("expected dlopenFailed, got \(err)")
      }
      XCTAssertTrue(path.contains(shared) && path.contains(legacy), "error must name every path tried: \(path)")
      XCTAssertEqual(detail, "fake: cannot open \(legacy)")
    }
  }
}

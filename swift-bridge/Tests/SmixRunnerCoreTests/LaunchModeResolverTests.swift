import XCTest
@testable import SmixRunnerCore

final class LaunchModeResolverTests: XCTestCase {
  func test_explicitActivate_returnsActivate() {
    XCTAssertEqual(
      LaunchModeResolver.resolve(env: ["SMIX_RUNNER_LAUNCH_MODE": "activate"]),
      .activate
    )
  }

  func test_missingKey_returnsLaunch() {
    XCTAssertEqual(
      LaunchModeResolver.resolve(env: [:]),
      .launch
    )
  }

  func test_emptyString_returnsLaunch() {
    XCTAssertEqual(
      LaunchModeResolver.resolve(env: ["SMIX_RUNNER_LAUNCH_MODE": ""]),
      .launch
    )
  }

  func test_unknownValue_returnsLaunch() {
    XCTAssertEqual(
      LaunchModeResolver.resolve(env: ["SMIX_RUNNER_LAUNCH_MODE": "bogus"]),
      .launch
    )
  }
}

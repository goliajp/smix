import XCTest
@testable import SmixRunnerCore

final class TargetBundleResolverTests: XCTestCase {
  func test_explicitValue_returnsThatValue() {
    XCTAssertEqual(
      TargetBundleResolver.resolve(env: ["SMIX_RUNNER_TARGET_BUNDLE": "com.apple.mobilesafari"]),
      "com.apple.mobilesafari"
    )
  }

  func test_missingKey_returnsDefaultPreferences() {
    XCTAssertEqual(
      TargetBundleResolver.resolve(env: [:]),
      "com.apple.Preferences"
    )
  }

  func test_emptyString_returnsDefaultPreferences() {
    XCTAssertEqual(
      TargetBundleResolver.resolve(env: ["SMIX_RUNNER_TARGET_BUNDLE": ""]),
      "com.apple.Preferences"
    )
  }

  func test_whitespaceOnly_returnsDefaultPreferences() {
    XCTAssertEqual(
      TargetBundleResolver.resolve(env: ["SMIX_RUNNER_TARGET_BUNDLE": "  "]),
      "com.apple.Preferences"
    )
  }
}

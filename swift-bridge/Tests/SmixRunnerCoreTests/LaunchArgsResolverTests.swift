import XCTest
@testable import SmixRunnerCore

// R4.d (audit #6) — LaunchArgsResolver: read SMIX_RUNNER_LAUNCH_ARGS env
// (JSON array literal) and resolve to an XCUIApplication launchArguments
// array. Empty/missing/malformed env → empty array = app's natural
// locale + defaults. Mirrors TargetBundleResolver / LaunchModeResolver
// shape so the runner side stays declarative (no inline env reading).
//
// Pre-R4 the runner hard-coded
// `["-AppleLanguages","(en)","-AppleLocale","en_US"]` for every app
// launch, hijacking locale across every caller. R4.d makes locale an
// opt-in env injection: e2e that need en locale ship the JSON literal;
// runner default is empty array so the app shows in its natural locale.

final class LaunchArgsResolverTests: XCTestCase {
  func test_emptyEnv_returnsEmpty() {
    XCTAssertEqual(LaunchArgsResolver.resolve(env: [:]), [])
  }

  func test_emptyStringEnv_returnsEmpty() {
    XCTAssertEqual(
      LaunchArgsResolver.resolve(env: ["SMIX_RUNNER_LAUNCH_ARGS": ""]),
      []
    )
  }

  func test_jsonArrayParses_returnsExactArray() {
    let env = [
      "SMIX_RUNNER_LAUNCH_ARGS":
        #"["-AppleLanguages","(en)","-AppleLocale","en_US"]"#
    ]
    XCTAssertEqual(
      LaunchArgsResolver.resolve(env: env),
      ["-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
    )
  }

  func test_whitespaceAroundJson_isTrimmed() {
    let env = [
      "SMIX_RUNNER_LAUNCH_ARGS":
        "  [\"--flag\", \"value\"]  \n"
    ]
    XCTAssertEqual(
      LaunchArgsResolver.resolve(env: env),
      ["--flag", "value"]
    )
  }

  func test_malformedJson_returnsEmpty() {
    let env = [
      "SMIX_RUNNER_LAUNCH_ARGS": "not json at all"
    ]
    XCTAssertEqual(LaunchArgsResolver.resolve(env: env), [])
  }

  func test_nonArrayJson_returnsEmpty() {
    let env = [
      "SMIX_RUNNER_LAUNCH_ARGS": #"{"a":"b"}"#
    ]
    XCTAssertEqual(LaunchArgsResolver.resolve(env: env), [])
  }

  func test_arrayWithNonStringElement_returnsEmpty() {
    // Mixed-type arrays would corrupt launchArguments. Fail-closed
    // (empty) is safer than partial.
    let env = [
      "SMIX_RUNNER_LAUNCH_ARGS": #"["-flag", 42]"#
    ]
    XCTAssertEqual(LaunchArgsResolver.resolve(env: env), [])
  }
}

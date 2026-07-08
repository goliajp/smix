import XCTest
@testable import SmixRunnerCore

// R5.b (audit Med-11) — RunnerPortResolver: read SMIX_RUNNER_PORT env and
// resolve to the HTTP port the runner binds on. Empty/missing → default
// 22087 (matches DEFAULT_RUNNER_PORT in src/sim/cell.ts). Non-integer or
// out-of-range → fallback to default (fail-closed; same shape as the
// other resolvers).
//
// Pre-R5.b the runner hard-coded `runForever(port: 22087)`, so cell-pool
// N>1 (allocating DEFAULT_RUNNER_PORT + i per cell) had no way to inform
// the swift runner which port to bind. Every cell collided on 22087 → the
// pool concurrency was structurally broken at the wire level.

final class RunnerPortResolverTests: XCTestCase {
  func test_emptyEnv_returnsDefault() {
    XCTAssertEqual(RunnerPortResolver.resolve(env: [:]), 22087)
  }

  func test_explicitNumericEnv_returnsParsed() {
    let env = ["SMIX_RUNNER_PORT": "22090"]
    XCTAssertEqual(RunnerPortResolver.resolve(env: env), 22090)
  }

  func test_nonNumericEnv_returnsDefault() {
    let env = ["SMIX_RUNNER_PORT": "not-a-number"]
    XCTAssertEqual(RunnerPortResolver.resolve(env: env), 22087)
  }

  func test_outOfRangeEnv_returnsDefault() {
    // Port must fit 1..65535. Negative / zero / > 65535 → fallback.
    XCTAssertEqual(
      RunnerPortResolver.resolve(env: ["SMIX_RUNNER_PORT": "0"]),
      22087
    )
    XCTAssertEqual(
      RunnerPortResolver.resolve(env: ["SMIX_RUNNER_PORT": "-1"]),
      22087
    )
    XCTAssertEqual(
      RunnerPortResolver.resolve(env: ["SMIX_RUNNER_PORT": "70000"]),
      22087
    )
  }
}

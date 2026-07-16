import Foundation

/// Resolves the HTTP port the runner binds on from the `SMIX_RUNNER_PORT`
/// env. Mirrors `TargetBundleResolver` / `LaunchModeResolver` /
/// `LaunchArgsResolver` shape: pure over env, fail-closed to default on
/// malformed input.
///
/// The port must be injectable because `cell-pool` N>1 allocates
/// `DEFAULT_RUNNER_PORT + i` per cell; without it every cell would collide
/// on the default port and multi-cell concurrency would break at the wire
/// level. The TS side plumbs the chosen port through
/// `TEST_RUNNER_SMIX_RUNNER_PORT` — Xcode forwards `TEST_RUNNER_`-prefixed
/// vars into the XCUITest process with the prefix stripped — and this
/// resolver decodes it on the swift side.
public enum RunnerPortResolver {
  public static let envKey = "SMIX_RUNNER_PORT"
  public static let defaultPort: UInt16 = 22087

  public static func resolve(env: [String: String]) -> UInt16 {
    guard let raw = env[envKey]?
      .trimmingCharacters(in: .whitespacesAndNewlines),
      !raw.isEmpty
    else { return defaultPort }
    guard let n = Int(raw) else { return defaultPort }
    guard n >= 1, n <= 65535 else { return defaultPort }
    return UInt16(n)
  }
}

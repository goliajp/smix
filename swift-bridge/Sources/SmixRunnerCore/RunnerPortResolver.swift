import Foundation

/// R5.b (audit Med-11) — resolves the HTTP port the runner binds on
/// from the `SMIX_RUNNER_PORT` env. Mirrors `TargetBundleResolver` /
/// `LaunchModeResolver` / `LaunchArgsResolver` shape: pure over env,
/// fail-closed to default on malformed input.
///
/// Pre-R5.b `test_runForever` hard-coded `runForever(port: 22087)`, so
/// `cell-pool` N>1 (allocating `DEFAULT_RUNNER_PORT + i` per cell) had
/// no way to tell the swift runner which port to bind. Every cell
/// collided on 22087 → multi-cell concurrency was structurally broken
/// at the wire level. The TS side now plumbs the chosen port through
/// `TEST_RUNNER_SMIX_RUNNER_PORT` (Xcode forwards `TEST_RUNNER_`-prefixed
/// vars into the XCUITest process with the prefix stripped); this
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

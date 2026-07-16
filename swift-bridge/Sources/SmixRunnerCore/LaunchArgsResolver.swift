import Foundation

/// Resolves `XCUIApplication.launchArguments` from the
/// `SMIX_RUNNER_LAUNCH_ARGS` env (JSON array literal). Mirrors
/// `TargetBundleResolver` / `LaunchModeResolver` shape: pure over env,
/// fail-closed on malformed input.
///
/// Locale is opt-in rather than imposed: the runner default is an empty
/// array, so the app launches in its natural locale, and a caller that
/// wants a forced locale ships the JSON literal itself — e.g.
/// `["-AppleLanguages","(en)","-AppleLocale","en_US"]` via
/// `TEST_RUNNER_SMIX_RUNNER_LAUNCH_ARGS=`.
///
/// Fail-closed (empty) on any non-string element: partial mixed-type
/// arrays would corrupt `launchArguments` so silent strip is unsafe.
public enum LaunchArgsResolver {
  public static let envKey = "SMIX_RUNNER_LAUNCH_ARGS"

  public static func resolve(env: [String: String]) -> [String] {
    let trimmed = env[envKey]?
      .trimmingCharacters(in: .whitespacesAndNewlines)
    guard let trimmed, !trimmed.isEmpty else { return [] }
    guard let data = trimmed.data(using: .utf8) else { return [] }
    let parsed: Any
    do {
      parsed = try JSONSerialization.jsonObject(with: data, options: [])
    } catch {
      return []
    }
    guard let arr = parsed as? [Any] else { return [] }
    var out: [String] = []
    out.reserveCapacity(arr.count)
    for el in arr {
      guard let s = el as? String else { return [] }
      out.append(s)
    }
    return out
  }
}

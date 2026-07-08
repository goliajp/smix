import Foundation

/// R4.d (audit #6) — resolves `XCUIApplication.launchArguments` from the
/// `SMIX_RUNNER_LAUNCH_ARGS` env (JSON array literal). Mirrors
/// `TargetBundleResolver` / `LaunchModeResolver` shape: pure over env,
/// fail-closed on malformed input.
///
/// Pre-R4 the runner hard-coded
/// `["-AppleLanguages","(en)","-AppleLocale","en_US"]` for every app
/// launch, hijacking locale across every caller. R4.d makes locale an
/// opt-in env injection: callers wanting English locale ship the JSON
/// literal via `TEST_RUNNER_SMIX_RUNNER_LAUNCH_ARGS=`; runner default
/// is empty array so the app launches in its natural locale.
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

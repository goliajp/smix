import Foundation

public enum TargetBundleResolver {
  public static let defaultBundleId = "com.apple.Preferences"
  public static let envKey = "SMIX_RUNNER_TARGET_BUNDLE"

  public static func resolve(env: [String: String]) -> String {
    let trimmed = env[envKey]?.trimmingCharacters(in: .whitespacesAndNewlines)
    guard let trimmed, !trimmed.isEmpty else { return defaultBundleId }
    return trimmed
  }
}

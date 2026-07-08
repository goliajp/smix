import Foundation

public enum RunnerLaunchMode: String { case launch, activate }

public enum LaunchModeResolver {
  public static let envKey = "SMIX_RUNNER_LAUNCH_MODE"
  public static let defaultMode: RunnerLaunchMode = .launch

  public static func resolve(env: [String: String]) -> RunnerLaunchMode {
    let trimmed = env[envKey]?.trimmingCharacters(in: .whitespacesAndNewlines)
    guard trimmed == "activate" else { return defaultMode }
    return .activate
  }
}

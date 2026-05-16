import Foundation

/// Pure version-dispatch helper. **No IO** — no dlsym, no simctl, no
/// framework lookups. Caller (CLI / TS Driver in C6) supplies the runtime
/// version it already knows.
public enum ChannelDispatcher {

  /// Major + minor of an iOS runtime (`com.apple.CoreSimulator.SimRuntime.iOS-26-4`
  /// → major=26 minor=4). Patch is irrelevant for the dispatcher decision.
  public struct RuntimeVersion: Equatable {
    public let major: Int
    public let minor: Int

    public init(major: Int, minor: Int) {
      self.major = major
      self.minor = minor
    }

    /// Parses CoreSimulator-style identifiers like
    /// `"com.apple.CoreSimulator.SimRuntime.iOS-26-4"` → `(26, 4)`.
    /// Returns nil for inputs that do not contain `iOS-MAJOR-MINOR` (case
    /// sensitive; suffix is anchored to the trailing `iOS-<digits>-<digits>`).
    public init?(identifier: String) {
      guard let range = identifier.range(of: "iOS-") else { return nil }
      let tail = identifier[range.upperBound...]
      let parts = tail.split(separator: "-")
      guard parts.count >= 2,
            let maj = Int(parts[0]),
            let min = Int(parts[1])
      else { return nil }
      self.major = maj
      self.minor = min
    }
  }

  /// Non-fatal hints surfaced alongside a dispatch decision.
  public enum Warning: Equatable {
    /// iOS 17/18 require the 5-arg `IndigoHIDMessageForMouseNSEvent` ABI;
    /// not yet implemented in this codebase. Falls back to `.digitizer`
    /// and emits this warning so callers can surface a stderr hint.
    case fiveArgIndigoNotImplemented(version: String)
  }

  /// Decision returned by `decide(...)`.
  public struct Decision: Equatable {
    public let channel: InputChannelId
    public let warning: Warning?

    public init(channel: InputChannelId, warning: Warning?) {
      self.channel = channel
      self.warning = warning
    }
  }

  /// Decision table:
  ///   - `override != nil` → that channel, no warning (user knows best)
  ///   - `major >= 26`     → `.digitizer` (C4 path is the supported route)
  ///   - `major == 17/18`  → `.digitizer` + `.fiveArgIndigoNotImplemented`
  ///                          (5-arg legacy ABI scheduled for v0.7)
  ///   - else              → `.digitizer` + `.fiveArgIndigoNotImplemented`
  ///                          (best-effort; same warning since the legacy
  ///                          path isn't wired)
  public static func decide(
    runtimeVersion: RuntimeVersion,
    override: InputChannelId?
  ) -> Decision {
    if let chosen = override {
      return Decision(channel: chosen, warning: nil)
    }
    if runtimeVersion.major >= 26 {
      return Decision(channel: .digitizer, warning: nil)
    }
    let versionStr = "\(runtimeVersion.major).\(runtimeVersion.minor)"
    return Decision(
      channel: .digitizer,
      warning: .fiveArgIndigoNotImplemented(version: versionStr)
    )
  }

  /// Pure factory — returns an `InputChannel` for the given id. No IO.
  /// Caller is responsible for dlopen + invocation.
  public static func channel(for id: InputChannelId) -> InputChannel {
    switch id {
    case .digitizer: return DigitizerChannel()
    case .indigo9:   return IndigoChannel()
    }
  }
}

import Foundation

/// Wire-stable JSON report emitted by `simx-host-hid axp-probe`.
///
/// Schema (single line, byte-stable — `src/cli/commands/doctor.ts`
/// `JSON.parse` + `isAxpCapability` narrow depends on this contract):
/// ```
/// {
///   "ok": <bool>,
///   "framework":      {"path":"<abs>","loaded":<bool>},
///   "classes":        {"resolved":[..],"missing":[..]},
///   "selectors":      {"resolved":[..],"missing":[..]},
///   "sharedInstance": {"available":<bool>}
/// }
/// ```
public struct AxpCapabilityReport: Equatable {
  public let framework: AxpCapabilityProbe.FrameworkProbe
  public let classes: AxpCapabilityProbe.ClassesProbe
  public let selectors: AxpCapabilityProbe.SelectorsProbe
  public let sharedInstance: AxpCapabilityProbe.SharedInstanceProbe

  public init(
    framework: AxpCapabilityProbe.FrameworkProbe,
    classes: AxpCapabilityProbe.ClassesProbe,
    selectors: AxpCapabilityProbe.SelectorsProbe,
    sharedInstance: AxpCapabilityProbe.SharedInstanceProbe
  ) {
    self.framework = framework
    self.classes = classes
    self.selectors = selectors
    self.sharedInstance = sharedInstance
  }

  /// `true` iff framework loaded + all 5 classes resolved + all 3 selectors
  /// resolved + `+sharedInstance` returns a non-nil instance.
  public var ok: Bool {
    framework.loaded
      && classes.missing.isEmpty
      && selectors.missing.isEmpty
      && sharedInstance.available
  }

  /// Single-line wire JSON. Byte sequence is part of the contract with
  /// `src/cli/commands/doctor.ts` `JSON.parse` downstream.
  public func json() -> String {
    let okStr = ok ? "true" : "false"
    let frameworkJson =
      "{\"path\":\"\(SimxHostJsonEscape.escape(framework.path))\"," +
      "\"loaded\":\(framework.loaded ? "true" : "false")}"
    let classesJson = Self.namedListJson(
      resolved: classes.resolved, missing: classes.missing
    )
    let selectorsJson = Self.namedListJson(
      resolved: selectors.resolved, missing: selectors.missing
    )
    let sharedJson =
      "{\"available\":\(sharedInstance.available ? "true" : "false")}"
    return
      "{\"ok\":\(okStr)," +
      "\"framework\":\(frameworkJson)," +
      "\"classes\":\(classesJson)," +
      "\"selectors\":\(selectorsJson)," +
      "\"sharedInstance\":\(sharedJson)}"
  }

  private static func namedListJson(resolved: [String], missing: [String]) -> String {
    let r = resolved
      .map { "\"\(SimxHostJsonEscape.escape($0))\"" }
      .joined(separator: ",")
    let m = missing
      .map { "\"\(SimxHostJsonEscape.escape($0))\"" }
      .joined(separator: ",")
    return "{\"resolved\":[\(r)],\"missing\":[\(m)]}"
  }

  /// Fallback report when the host environment itself fails (dlopen of
  /// the framework returned nil). Wire JSON is still well-formed so the
  /// TS `simx doctor` side can render it deterministically.
  public static func allUnavailable() -> AxpCapabilityReport {
    return AxpCapabilityReport(
      framework: .init(path: AxpCapabilityProbe.frameworkPath, loaded: false),
      classes: .init(resolved: [], missing: AxpCapabilityProbe.requiredClassNames),
      selectors: .init(resolved: [], missing: AxpCapabilityProbe.requiredInstanceSelectorNames),
      sharedInstance: .init(available: false)
    )
  }
}

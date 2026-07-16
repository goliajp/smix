import Foundation

/// CLI args for `smix-host-hid`.
public enum SmixHostHIDArgs: Equatable {
  /// Which HID dispatch implementation to use for `tap`.
  ///
  /// - `digitizer`: IOHIDEvent-tree path (the default) — works on iOS 26.4
  ///   where the 9-arg mouseFn path is rerouted to Home gesture / dropped.
  /// - `indigo9`: 9-arg `IndigoHIDMessageForMouseNSEvent`, retained for
  ///   `smix doctor` comparisons and iOS < 26 fallback work.
  public enum Path: String, Equatable {
    case digitizer
    case indigo9
  }

  case tap(udid: String, x: Double, y: Double, path: Path)
  /// `probe` subcommand — host-side symbol availability report; no args.
  case probe
  /// `axp-probe` subcommand — host-side
  /// `AccessibilityPlatformTranslation.framework` capability report; no args.
  case axpProbe
  /// `daemon` subcommand — long-running sidecar; stdin JSON-RPC.
  case daemon

  public enum ParseError: Error, Equatable {
    case missingSubcommand
    case unknownSubcommand(String)
    case missingFlag(String)
    case invalidFlagValue(name: String, value: String)
  }

  /// Hand-rolled flag parser — intentionally no swift-argument-parser dep
  /// (YAGNI for a single subcommand with a handful of flags).
  public static func parse(_ args: [String]) throws -> SmixHostHIDArgs {
    guard let sub = args.first else { throw ParseError.missingSubcommand }
    let rest = Array(args.dropFirst())
    switch sub {
    case "tap":
      return try parseTap(rest)
    case "probe":
      return try parseProbe(rest)
    case "axp-probe":
      return try parseAxpProbe(rest)
    case "daemon":
      return try parseDaemon(rest)
    default:
      throw ParseError.unknownSubcommand(sub)
    }
  }

  /// `probe` takes no flags. Any leftover token is a user mistake — surface
  /// it as `.invalidFlagValue` so the CLI exits non-zero with a clear hint.
  private static func parseProbe(_ args: [String]) throws -> SmixHostHIDArgs {
    if let first = args.first {
      let value = args.count > 1 ? args[1] : ""
      throw ParseError.invalidFlagValue(name: first, value: value)
    }
    return .probe
  }

  /// `axp-probe` takes no flags — same hand-rolled shape as `probe`.
  private static func parseAxpProbe(_ args: [String]) throws -> SmixHostHIDArgs {
    if let first = args.first {
      let value = args.count > 1 ? args[1] : ""
      throw ParseError.invalidFlagValue(name: first, value: value)
    }
    return .axpProbe
  }

  /// `daemon` takes no flags — long-running sidecar with stdin JSON-RPC.
  private static func parseDaemon(_ args: [String]) throws -> SmixHostHIDArgs {
    if let first = args.first {
      let value = args.count > 1 ? args[1] : ""
      throw ParseError.invalidFlagValue(name: first, value: value)
    }
    return .daemon
  }

  private static func parseTap(_ args: [String]) throws -> SmixHostHIDArgs {
    var udid: String?
    var xRaw: String?
    var yRaw: String?
    var pathRaw: String?

    var i = 0
    while i < args.count {
      let flag = args[i]
      guard i + 1 < args.count else {
        throw ParseError.missingFlag(flag)
      }
      let value = args[i + 1]
      switch flag {
      case "--udid": udid = value
      case "--x":    xRaw  = value
      case "--y":    yRaw  = value
      case "--path": pathRaw = value
      default:
        throw ParseError.invalidFlagValue(name: flag, value: value)
      }
      i += 2
    }

    guard let udidValue = udid else { throw ParseError.missingFlag("--udid") }
    guard let xString = xRaw else { throw ParseError.missingFlag("--x") }
    guard let yString = yRaw else { throw ParseError.missingFlag("--y") }
    guard let x = Double(xString) else {
      throw ParseError.invalidFlagValue(name: "--x", value: xString)
    }
    guard let y = Double(yString) else {
      throw ParseError.invalidFlagValue(name: "--y", value: yString)
    }

    let path: Path
    if let rawPath = pathRaw {
      guard let parsed = Path(rawValue: rawPath) else {
        throw ParseError.invalidFlagValue(name: "--path", value: rawPath)
      }
      path = parsed
    } else {
      // Default: IOHIDEvent digitizer-tree path.
      path = .digitizer
    }

    return .tap(udid: udidValue, x: x, y: y, path: path)
  }
}

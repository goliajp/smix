import Foundation

/// Wire-stable JSON report emitted by `simx-host-hid probe`.
///
/// Schema (single line, byte-stable — `src/cli/commands/doctor.ts`
/// `JSON.parse` depends on this contract):
/// ```
/// {
///   "ok": <bool>,
///   "channels": [
///     {"name":"digitizer","available":<bool>,"resolved":[..],"missing":[..]},
///     {"name":"indigo9",  "available":<bool>,"resolved":[..],"missing":[..]}
///   ]
/// }
/// ```
public struct ChannelProbeReport: Equatable {
  public struct ChannelEntry: Equatable {
    /// Wire-stable channel name — equal to `InputChannelId.rawValue`.
    public let name: String
    public let available: Bool
    public let resolved: [String]
    public let missing: [String]

    public init(name: String, available: Bool, resolved: [String], missing: [String]) {
      self.name = name
      self.available = available
      self.resolved = resolved
      self.missing = missing
    }
  }

  public let channels: [ChannelEntry]

  public init(channels: [ChannelEntry]) {
    self.channels = channels
  }

  /// `true` iff every channel reports `available: true`.
  public var ok: Bool { channels.allSatisfy { $0.available } }

  /// Single-line wire JSON. Byte sequence is part of the contract with
  /// `src/cli/commands/doctor.ts` `JSON.parse` downstream.
  public func json() -> String {
    let okStr = ok ? "true" : "false"
    let channelsJson = channels.map(Self.channelJson).joined(separator: ",")
    return "{\"ok\":\(okStr),\"channels\":[\(channelsJson)]}"
  }

  private static func channelJson(_ c: ChannelEntry) -> String {
    let nameEsc = SimxHostJsonEscape.escape(c.name)
    let availStr = c.available ? "true" : "false"
    let resolvedArr = c.resolved
      .map { "\"\(SimxHostJsonEscape.escape($0))\"" }
      .joined(separator: ",")
    let missingArr = c.missing
      .map { "\"\(SimxHostJsonEscape.escape($0))\"" }
      .joined(separator: ",")
    return
      "{\"name\":\"\(nameEsc)\",\"available\":\(availStr)," +
      "\"resolved\":[\(resolvedArr)],\"missing\":[\(missingArr)]}"
  }

  /// Pure factory — tries to resolve each channel's `requiredSymbolNames`
  /// against the supplied handles. Never throws; per-channel `dlsym`
  /// misses are recorded in `missing` (and force `available: false`).
  ///
  /// `simulatorKitHandle` is the dlopen'd private framework; `ioKitHandle`
  /// should be `DigitizerSymbols.rtldDefault` for production (IOKit lives
  /// in the dyld shared cache).
  public static func probe(
    simulatorKitHandle: UnsafeMutableRawPointer,
    ioKitHandle: UnsafeMutableRawPointer,
    via resolver: DlsymResolver
  ) -> ChannelProbeReport {
    let digitizer = probeChannel(
      name: InputChannelId.digitizer.rawValue,
      symbolNames: DigitizerSymbolNames.allRequiredForC4
    ) { name in
      // Trackpad wrap lives in SimulatorKit; the other 3 are in IOKit
      // (resolved via the RTLD_DEFAULT sentinel by production callers).
      if name == DigitizerSymbolNames.trackpadWrap {
        return resolver.sym(simulatorKitHandle, name)
      }
      return resolver.sym(ioKitHandle, name)
    }
    let indigo = probeChannel(
      name: InputChannelId.indigo9.rawValue,
      symbolNames: IndigoSymbolNames.allRequiredForC3
    ) { name in
      resolver.sym(simulatorKitHandle, name)
    }
    return ChannelProbeReport(channels: [digitizer, indigo])
  }

  private static func probeChannel(
    name: String,
    symbolNames: [String],
    resolve: (String) -> UnsafeMutableRawPointer?
  ) -> ChannelEntry {
    var resolved: [String] = []
    var missing: [String] = []
    for symName in symbolNames {
      if resolve(symName) != nil {
        resolved.append(symName)
      } else {
        missing.append(symName)
      }
    }
    return ChannelEntry(
      name: name,
      available: missing.isEmpty,
      resolved: resolved,
      missing: missing
    )
  }
}

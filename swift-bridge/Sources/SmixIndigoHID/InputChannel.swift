import Foundation

/// Stable HID dispatch channel identifier — matches the wire-ABI `path`
/// field on `smix-host-hid tap`.
public enum InputChannelId: String, Equatable, CaseIterable {
  /// IOHIDEvent-tree path.
  case digitizer
  /// 9-arg `IndigoHIDMessageForMouseNSEvent` path.
  case indigo9
}

/// Wraps either `IOHIDDigitizerTap` (digitizer) or `IndigoHIDDispatcher`
/// (indigo9) behind a single API so the CLI dispatch, probe, and Driver
/// integration go through one site.
///
/// Channels are **wrappers** around the existing implementations — they
/// do not reimplement byte-patching / mouseFn invocation. See
/// `DigitizerChannel` / `IndigoChannel` for the thin glue.
public protocol InputChannel {
  /// Stable channel identifier.
  var id: InputChannelId { get }
  /// Wire-stable name. Equal to `id.rawValue` for both current channels; kept
  /// on the protocol so future non-rawValue names (e.g. `indigo5`) stay open.
  var name: String { get }
  /// Symbols required by this channel. Used by the probe path: missing any
  /// → channel reports `available: false`.
  var requiredSymbolNames: [String] { get }

  /// Real tap dispatch. Arg shape mirrors `IOHIDDigitizerTap.tapNormalized`
  /// / `IndigoHIDDispatcher.tapNormalized` so the Driver has one
  /// callsite. Channels reuse existing channel-class function bodies.
  func tap(
    udid: String,
    x: Double, y: Double,
    developerDir: String,
    coreSimulatorHandle: UnsafeMutableRawPointer,
    simulatorKitHandle: UnsafeMutableRawPointer,
    via resolver: DlsymResolver
  ) throws
}

extension InputChannel {
  /// Default `name == id.rawValue`. Channels that need a wire-name distinct
  /// from the id (e.g. `indigo5` later) override this property.
  public var name: String { id.rawValue }
}

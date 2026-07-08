import Foundation

/// C3 9-arg `IndigoHIDMessageForMouseNSEvent` path — thin wrapper over
/// `IndigoHIDDispatcher`.
public struct IndigoChannel: InputChannel {
  public init() {}

  public var id: InputChannelId { .indigo9 }

  public var requiredSymbolNames: [String] {
    IndigoSymbolNames.allRequiredForC3
  }

  public func tap(
    udid: String,
    x: Double, y: Double,
    developerDir: String,
    coreSimulatorHandle: UnsafeMutableRawPointer,
    simulatorKitHandle: UnsafeMutableRawPointer,
    via resolver: DlsymResolver
  ) throws {
    let indigoSyms = try IndigoSymbols.resolve(
      handle: simulatorKitHandle, via: resolver
    )
    try IndigoHIDDispatcher.tapNormalized(
      udid: udid, x: x, y: y,
      symbols: indigoSyms,
      developerDir: developerDir,
      coreSimulatorHandle: coreSimulatorHandle
    )
  }
}

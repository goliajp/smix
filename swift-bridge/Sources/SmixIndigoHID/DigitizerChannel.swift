import Foundation

/// IOHIDEvent-tree path — thin wrapper over `IOHIDDigitizerTap`.
///
/// `tap(...)` performs the dlsym resolution for both IndigoSymbols (still
/// required by `IOHIDDigitizerTap.tapNormalized`'s signature even though this
/// path never invokes them) and DigitizerSymbols, then dispatches.
public struct DigitizerChannel: InputChannel {
  public init() {}

  public var id: InputChannelId { .digitizer }

  public var requiredSymbolNames: [String] {
    DigitizerSymbolNames.allRequired
  }

  public func tap(
    udid: String,
    x: Double, y: Double,
    developerDir: String,
    coreSimulatorHandle: UnsafeMutableRawPointer,
    simulatorKitHandle: UnsafeMutableRawPointer,
    via resolver: DlsymResolver
  ) throws {
    // IndigoSymbols.resolve is still required by the legacy
    // IOHIDDigitizerTap.tapNormalized signature.
    let indigoSyms = try IndigoSymbols.resolve(
      handle: simulatorKitHandle, via: resolver
    )
    let digSyms = try DigitizerSymbols.resolve(
      ioKitHandle: DigitizerSymbols.rtldDefault,
      simulatorKitHandle: simulatorKitHandle,
      via: resolver
    )
    try IOHIDDigitizerTap.tapNormalized(
      udid: udid, x: x, y: y,
      symbols: indigoSyms,
      digitizerSymbols: digSyms,
      developerDir: developerDir,
      coreSimulatorHandle: coreSimulatorHandle
    )
  }
}

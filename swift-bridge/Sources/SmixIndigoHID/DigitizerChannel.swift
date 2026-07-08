import Foundation

/// C4 IOHIDEvent-tree path — thin wrapper over `IOHIDDigitizerTap`.
///
/// `tap(...)` performs the dlsym resolution for both IndigoSymbols (still
/// required by `IOHIDDigitizerTap.tapNormalized`'s signature — plan §S1
/// decision #6: C5 keeps the unused-resolve waste, C6 splits the symbol
/// bags) and DigitizerSymbols, then dispatches.
public struct DigitizerChannel: InputChannel {
  public init() {}

  public var id: InputChannelId { .digitizer }

  public var requiredSymbolNames: [String] {
    DigitizerSymbolNames.allRequiredForC4
  }

  public func tap(
    udid: String,
    x: Double, y: Double,
    developerDir: String,
    coreSimulatorHandle: UnsafeMutableRawPointer,
    simulatorKitHandle: UnsafeMutableRawPointer,
    via resolver: DlsymResolver
  ) throws {
    // C3 IndigoSymbols.resolve still required by the legacy
    // IOHIDDigitizerTap.tapNormalized signature; see plan §S1 decision #6.
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

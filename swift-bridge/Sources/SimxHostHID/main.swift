import Foundation
import SimxIndigoHID

@main
enum SimxHostHIDCLI {
  static func main() {
    let raw = CommandLine.arguments.dropFirst()
    do {
      let parsed = try SimxHostHIDArgs.parse(Array(raw))
      switch parsed {
      case .tap(let udid, let x, let y, let path):
        let resolved = try runTap(udid: udid, x: x, y: y, path: path)
        print(SimxHostHIDResult.success(path: path.rawValue, resolved: resolved))
        exit(0)
      case .probe:
        let report = runProbe()
        print(report.json())
        exit(0)
      case .axpProbe:
        let report = AxpCapabilityProbe.probeLive(via: SystemDlsymResolver())
        print(report.json())
        exit(0)
      }
    } catch let e as HostHIDError {
      FileHandle.standardError.write(Data("\(e)\n".utf8))
      print(SimxHostHIDResult.failure(error: e))
      exit(1)
    } catch let e as SimxHostHIDArgs.ParseError {
      FileHandle.standardError.write(Data("\(e)\n".utf8))
      print(SimxHostHIDResult.failure(error: .invalidArgument("\(e)")))
      exit(2)
    } catch {
      print(SimxHostHIDResult.failure(error: .invalidArgument("\(error)")))
      exit(3)
    }
  }

  /// Dispatches `tap` to either the C3 9-arg mouseFn path (`--path indigo9`)
  /// or the C4 IOHIDEvent digitizer path (`--path digitizer`, default).
  static func runTap(
    udid: String,
    x: Double, y: Double,
    path: SimxHostHIDArgs.Path
  ) throws -> [String] {
    let resolver = SystemDlsymResolver()
    let dev = try CoreSimulatorBridge.developerDir()

    let skPath = CoreSimulatorBridge.simulatorKitPath(dev)
    guard let skHandle = resolver.open(skPath) else {
      throw HostHIDError.dlopenFailed(path: skPath, detail: resolver.lastErrorDescription())
    }
    guard let csHandle = resolver.open(CoreSimulatorBridge.coreSimulatorPath) else {
      throw HostHIDError.dlopenFailed(
        path: CoreSimulatorBridge.coreSimulatorPath,
        detail: resolver.lastErrorDescription()
      )
    }
    // C3 IndigoSymbols.resolve still runs for both paths so the `--path
    // indigo9` branch is one decision away (plan §S1 decision #6 — C5 splits).
    let indigoSyms = try IndigoSymbols.resolve(handle: skHandle, via: resolver)

    switch path {
    case .indigo9:
      try IndigoHIDDispatcher.tapNormalized(
        udid: udid, x: x, y: y,
        symbols: indigoSyms,
        developerDir: dev,
        coreSimulatorHandle: csHandle
      )
      return IndigoSymbolNames.allRequiredForC3

    case .digitizer:
      // IOKit is public + already in dyld shared cache; use the
      // RTLD_DEFAULT sentinel handle. CLAUDE.md §9.6 still holds — no
      // `import IOKit`, no `linkedFramework`.
      let digSyms = try DigitizerSymbols.resolve(
        ioKitHandle: DigitizerSymbols.rtldDefault,
        simulatorKitHandle: skHandle,
        via: resolver
      )
      try IOHIDDigitizerTap.tapNormalized(
        udid: udid, x: x, y: y,
        symbols: indigoSyms,
        digitizerSymbols: digSyms,
        developerDir: dev,
        coreSimulatorHandle: csHandle
      )
      return DigitizerSymbolNames.allRequiredForC4
    }
  }

  /// `probe` subcommand: host-side dlopen of SimulatorKit + per-channel
  /// `dlsym` of `requiredSymbolNames`. Never throws — failures (dlopen,
  /// missing xcode-select dir) produce a report where every channel is
  /// `available: false` with all required symbols listed in `missing`.
  ///
  /// Exit-code policy (plan §S2): probe always exits 0 as long as the
  /// JSON is emitted; non-zero is reserved for args-parse failure or
  /// completely unexpected error paths handled by `main()`'s catches.
  static func runProbe() -> ChannelProbeReport {
    let resolver = SystemDlsymResolver()
    let dev: String
    do {
      dev = try CoreSimulatorBridge.developerDir()
    } catch {
      return allUnavailable()
    }
    let skPath = CoreSimulatorBridge.simulatorKitPath(dev)
    guard let skHandle = resolver.open(skPath) else {
      return allUnavailable()
    }
    return ChannelProbeReport.probe(
      simulatorKitHandle: skHandle,
      ioKitHandle: DigitizerSymbols.rtldDefault,
      via: resolver
    )
  }

  /// Fallback report for cases where the host environment itself fails
  /// (no `xcode-select -p` / `dlopen SimulatorKit` failed). The JSON is
  /// still well-formed so doctor.ts can render it deterministically.
  private static func allUnavailable() -> ChannelProbeReport {
    ChannelProbeReport(channels: [
      .init(
        name: InputChannelId.digitizer.rawValue,
        available: false,
        resolved: [],
        missing: DigitizerSymbolNames.allRequiredForC4
      ),
      .init(
        name: InputChannelId.indigo9.rawValue,
        available: false,
        resolved: [],
        missing: IndigoSymbolNames.allRequiredForC3
      ),
    ])
  }
}

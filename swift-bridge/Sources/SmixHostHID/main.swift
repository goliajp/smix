import Foundation
import SmixIndigoHID

@main
enum SmixHostHIDCLI {
  static func main() {
    let raw = CommandLine.arguments.dropFirst()
    do {
      let parsed = try SmixHostHIDArgs.parse(Array(raw))
      switch parsed {
      case .tap(let udid, let x, let y, let path):
        let resolved = try runTap(udid: udid, x: x, y: y, path: path)
        print(SmixHostHIDResult.success(path: path.rawValue, resolved: resolved))
        exit(0)
      case .probe:
        let report = runProbe()
        print(report.json())
        exit(0)
      case .axpProbe:
        let report = AxpCapabilityProbe.probeLive(via: SystemDlsymResolver())
        print(report.json())
        exit(0)
      case .daemon:
        runDaemon()  // Never returns; calls exit() internally.
      }
    } catch let e as HostHIDError {
      FileHandle.standardError.write(Data("\(e)\n".utf8))
      print(SmixHostHIDResult.failure(error: e))
      exit(1)
    } catch let e as SmixHostHIDArgs.ParseError {
      FileHandle.standardError.write(Data("\(e)\n".utf8))
      print(SmixHostHIDResult.failure(error: .invalidArgument("\(e)")))
      exit(2)
    } catch {
      print(SmixHostHIDResult.failure(error: .invalidArgument("\(error)")))
      exit(3)
    }
  }

  /// Dispatches `tap` to either the 9-arg mouseFn path (`--path indigo9`)
  /// or the IOHIDEvent digitizer path (`--path digitizer`, default).
  static func runTap(
    udid: String,
    x: Double, y: Double,
    path: SmixHostHIDArgs.Path
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
    // IndigoSymbols.resolve runs for both paths so the `--path indigo9`
    // branch is one decision away.
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
      // RTLD_DEFAULT sentinel handle. No `import IOKit`, no
      // `linkedFramework` — everything goes through dlsym.
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
  /// Exit-code policy: probe always exits 0 as long as the JSON is
  /// emitted; non-zero is reserved for args-parse failure or completely
  /// unexpected error paths handled by `main()`'s catches.
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

  /// Long-running sidecar daemon. dlopen everything once, then loop on
  /// stdin reading newline-delimited JSON ops. Each tap reuses the
  /// resolved dlsym handles → eliminates the per-call subprocess+dlopen
  /// overhead of `tap` mode.
  ///
  /// Output policy: every op writes exactly one newline-delimited JSON
  /// line to stdout (line-buffered via setbuf). EOF (parent closed stdin)
  /// → exit 0.
  static func runDaemon() -> Never {
    // Line-buffered stdout so the TS client can read JSON line-by-line
    // without manual fflush after every print.
    setbuf(stdout, nil)

    // One-time dlopen: SimulatorKit + CoreSimulator + Indigo + Digitizer symbol
    // resolution. Failure on bootstrap → emit one error JSON + exit 1 (the TS
    // client treats this as "sidecar acquire failed" and falls back to
    // spawn-per-call).
    let resolver = SystemDlsymResolver()
    let dev: String
    do {
      dev = try CoreSimulatorBridge.developerDir()
    } catch {
      print(SidecarDaemon.formatPing(ok: false))
      exit(1)
    }
    let skPath = CoreSimulatorBridge.simulatorKitPath(dev)
    guard let skHandle = resolver.open(skPath) else {
      print(SidecarDaemon.formatPing(ok: false))
      exit(1)
    }
    guard let csHandle = resolver.open(CoreSimulatorBridge.coreSimulatorPath) else {
      print(SidecarDaemon.formatPing(ok: false))
      exit(1)
    }
    let indigoSyms: IndigoSymbols
    do {
      indigoSyms = try IndigoSymbols.resolve(handle: skHandle, via: resolver)
    } catch {
      print(SidecarDaemon.formatPing(ok: false))
      exit(1)
    }
    let digSyms: DigitizerSymbols
    do {
      digSyms = try DigitizerSymbols.resolve(
        ioKitHandle: DigitizerSymbols.rtldDefault,
        simulatorKitHandle: skHandle,
        via: resolver
      )
    } catch {
      print(SidecarDaemon.formatPing(ok: false))
      exit(1)
    }

    while let line = readLine() {
      let op: SidecarOp
      do {
        op = try SidecarDaemon.parseLine(line)
      } catch {
        // Parse error: emit a tap-shaped failure (op:"tap" placeholder is fine
        // — caller can correlate by FIFO order) and continue the loop.
        print(SidecarDaemon.formatTap(ok: false, path: "parse_error", resolved: []))
        continue
      }
      switch op {
      case .ping:
        print(SidecarDaemon.formatPing(ok: true))
      case .shutdown:
        // Clean shutdown — caller asked us to die.
        print(#"{"ok":true,"op":"shutdown"}"#)
        exit(0)
      case .axTree(let udid):
        // Live AXP invocation is not yet wired here; signal failure so
        // the TS client falls back to runner /tree without breaking the
        // daemon loop.
        _ = udid
        print(SidecarDaemon.formatAxTree(ok: false, jsonBody: "{}"))
      case .tap(let udid, let x, let y, let path):
        let resolvedNames: [String]
        do {
          switch path {
          case .indigo9:
            try IndigoHIDDispatcher.tapNormalized(
              udid: udid, x: x, y: y,
              symbols: indigoSyms,
              developerDir: dev,
              coreSimulatorHandle: csHandle
            )
            resolvedNames = IndigoSymbolNames.allRequiredForC3
          case .digitizer:
            try IOHIDDigitizerTap.tapNormalized(
              udid: udid, x: x, y: y,
              symbols: indigoSyms,
              digitizerSymbols: digSyms,
              developerDir: dev,
              coreSimulatorHandle: csHandle
            )
            resolvedNames = DigitizerSymbolNames.allRequiredForC4
          }
          print(SidecarDaemon.formatTap(ok: true, path: path.rawValue, resolved: resolvedNames))
        } catch {
          // Loop continues on tap error — caller decides whether to retry.
          print(SidecarDaemon.formatTap(ok: false, path: path.rawValue, resolved: []))
        }
      }
    }
    // EOF on stdin (parent closed) → clean exit.
    exit(0)
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

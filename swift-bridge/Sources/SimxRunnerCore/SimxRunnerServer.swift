import FlyingFox
import Foundation

#if canImport(CoreGraphics)
import CoreGraphics
#endif

// Wraps FlyingFox HTTPServer so the XCUITest runner does not need to import
// FlyingFox directly — it links SimxRunnerCore only. Subsequent checkpoints
// register more routes on this server (screenshot/swipe/...).
public actor SimxRunnerServer {
  public enum TapOutcome: Sendable {
    case matched(label: String)
    case notFound
  }
  public typealias TapHandler = @Sendable (TapRoute.TapRequest) async -> TapOutcome

  /// v0.3 C1 — XCUI snapshot is constructed inside the UITest target closure
  /// (XCUIApplication.snapshot() is a throwing, blocking API) and returned
  /// as a POCO so SimxRunnerCore stays free of XCTest/XCUI imports.
  /// nil indicates the snapshot is unavailable (app not launched / crashed) —
  /// the server responds with 500 + snapshot_unavailable in that case.
  public typealias SnapshotResult = (root: TreeRoute.A11ySnapshotData, appFrame: CGRect)
  public typealias SnapshotHandler = @Sendable () async -> SnapshotResult?

  public init() {}

  public func runForever(
    port: UInt16,
    tapHandler: @escaping TapHandler,
    snapshotHandler: @escaping SnapshotHandler
  ) async throws {
    let server = HTTPServer(port: port)
    await server.appendRoute("GET /health") { _ in
      HealthRoute.response()
    }
    await server.appendRoute("POST /tap") { request in
      let body: Data
      do {
        body = try await request.bodyData
      } catch {
        return TapRoute.badRequest(reason: "failed to read body: \(error)")
      }
      let req: TapRoute.TapRequest
      do {
        req = try TapRoute.decode(body)
      } catch let e as TapRoute.DecodeError {
        return TapRoute.badRequest(reason: "\(e)")
      } catch {
        return TapRoute.badRequest(reason: "\(error)")
      }
      let outcome = await tapHandler(req)
      switch outcome {
      case .matched(let label):
        return TapRoute.success(matchedLabel: label)
      case .notFound:
        return TapRoute.notFound(selector: req.selector)
      }
    }
    await server.appendRoute("GET /tree") { _ in
      guard let snap = await snapshotHandler() else {
        return TreeRoute.unavailable()
      }
      let payload = TreeRoute.serialize(
        snap.root,
        appFrame: snap.appFrame,
        logSink: { line in
          FileHandle.standardError.write(Data((line + "\n").utf8))
        }
      )
      return TreeRoute.success(payload)
    }
    try await server.run()
  }
}

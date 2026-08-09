import XCTest
import FlyingFox
#if canImport(CoreGraphics)
import CoreGraphics
#endif
@testable import SmixRunnerCore

// `POST /find?include=` and `POST /tap?include=` scope forwarding +
// zero-regression anchor.
//
// Root cause: the see-through capture (`?include=all-windows`) was
// wired into the /tree SnapshotHandler only. /find and /tap kept
// resolving against the no-parameter masked element set, so content
// the SDK could SEE via /tree was still un-tappable (404). The fix
// threads the same `include` query value into
// the Find/Tap handlers (identical mechanism to `GET /tree` —
// `request.query["include"]`, FlyingFox `[QueryItem]` string subscript,
// method-independent). nil scope (no query) MUST keep the handler
// receiving nil = the byte-identical legacy resolution path (the
// zero-regression anchor: existing SDK runner-client posts /find & /tap
// WITHOUT a query and must be unaffected). "all-windows" reaches the
// handler verbatim so the UITest target can switch its candidate element
// source to the see-through set. Unit-decidable in SmixRunnerCore (no
// XCUI / no sim): drive the REAL `runForever` route via an IPv4 client on
// a fixed free port, recording the scope each handler received, then
// `POST /shutdown` for a clean graceful stop.
final class FindTapScopeForwardTests: XCTestCase {

  private func freePort() async throws -> UInt16 {
    let probe = SmixRunnerServer.makeServer(port: 0)
    let probeTask = Task { _ = try? await probe.run() }
    var port: UInt16 = 0
    for _ in 0..<100 {
      if let a = await probe.listeningAddress {
        switch a {
        case .ip4(_, let p), .ip6(_, let p): port = p
        case .unix: break
        }
        if port != 0 { break }
      }
      try await Task.sleep(nanoseconds: 50_000_000)
    }
    await probe.stop(timeout: 0.5)
    probeTask.cancel()
    XCTAssertNotEqual(port, 0, "probe server did not bind")
    return port
  }

  private func waitUp(_ port: UInt16) async throws {
    var up = false
    for _ in 0..<100 {
      if let (_, resp) = try? await URLSession.shared.data(
        from: URL(string: "http://127.0.0.1:\(port)/health")!),
        (resp as? HTTPURLResponse)?.statusCode == 200 {
        up = true; break
      }
      try await Task.sleep(nanoseconds: 50_000_000)
    }
    XCTAssertTrue(up, "server did not come up on :\(port)")
  }

  private func post(_ port: UInt16, _ path: String, _ json: String) async throws {
    var req = URLRequest(url: URL(string: "http://127.0.0.1:\(port)\(path)")!)
    req.httpMethod = "POST"
    req.httpBody = Data(json.utf8)
    req.setValue("application/json", forHTTPHeaderField: "Content-Type")
    _ = try await URLSession.shared.data(for: req)
  }

  private func shutdown(_ port: UInt16, _ task: Task<Void, Error>) async {
    var req = URLRequest(url: URL(string: "http://127.0.0.1:\(port)/shutdown")!)
    req.httpMethod = "POST"
    _ = try? await URLSession.shared.data(for: req)
    _ = try? await task.value
  }

  func test_findRoute_forwardsIncludeQuery_andNoParamStaysLegacyNil() async throws {
    let port = try await freePort()
    let received = ScopeBox()
    let server = SmixRunnerServer()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        findHandler: { _, scope, _ in
          await received.set(scope)
          return SmixRunnerServer.FindOutcome(found: false)
        }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    // No ?include= ⇒ handler receives nil ⇒ legacy resolution path
    // (zero-regression anchor: SDK runner-client posts no query).
    try await post(port, "/find", #"{"selector":{"text":"General"}}"#)
    let s1 = await received.value
    XCTAssertEqual(s1, .some(nil),
      "no ?include= on POST /find ⇒ scope nil (legacy element source)")

    // ?include=all-windows ⇒ forwarded verbatim to FindHandler.
    try await post(port, "/find?include=all-windows",
                   #"{"selector":{"text":"General"}}"#)
    let s2 = await received.value
    XCTAssertEqual(s2, .some("all-windows"),
      "?include=all-windows forwarded verbatim to FindHandler")

    await shutdown(port, runTask)
  }

  func test_tapRoute_forwardsIncludeQuery_andNoParamStaysLegacyNil() async throws {
    let port = try await freePort()
    let received = ScopeBox()
    let server = SmixRunnerServer()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, scope in
          await received.set(scope)
          return .notFound
        },
        snapshotHandler: { _ in nil }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    try await post(port, "/tap", #"{"selector":{"text":"General"}}"#)
    let s1 = await received.value
    XCTAssertEqual(s1, .some(nil),
      "no ?include= on POST /tap ⇒ scope nil (legacy element source)")

    try await post(port, "/tap?include=all-windows",
                   #"{"selector":{"text":"General"}}"#)
    let s2 = await received.value
    XCTAssertEqual(s2, .some("all-windows"),
      "?include=all-windows forwarded verbatim to TapHandler")

    await shutdown(port, runTask)
  }

  // Tracks every scope handed to a handler. `.some(nil)` = handler ran
  // with no query (the legacy nil scope); `.none` = handler never ran.
  actor ScopeBox {
    private var _v: String??
    func set(_ v: String?) { _v = .some(v) }
    var value: String?? { _v }
  }
}

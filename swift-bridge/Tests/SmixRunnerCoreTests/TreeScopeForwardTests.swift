import XCTest
import FlyingFox
#if canImport(CoreGraphics)
import CoreGraphics
#endif
@testable import SmixRunnerCore

// v1.4 ③-C1 A — `GET /tree?include=` scope forwarding + zero-regression
// anchor. The server's `GET /tree` route reads the `include` query value
// and hands it to the SnapshotHandler. nil scope (no query param) MUST
// produce a body byte-identical to the legacy single-snapshot path (the
// contract: existing SDK / host smoke gates parse the same bytes).
// "all-windows" reaches the handler verbatim so the UITest target can
// switch to the see-through capture. Unit-decidable in SmixRunnerCore
// (no XCUI / no sim): drive the REAL `runForever` route via an IPv4
// client on a fixed free port, recording the scope the handler received,
// then `POST /shutdown` for a clean graceful stop (no hang, no leaked
// server — the v1.4 C3 shutdown path).
final class TreeScopeForwardTests: XCTestCase {

  private func leaf() -> TreeRoute.A11ySnapshotData {
    TreeRoute.A11ySnapshotData(
      elementTypeRawValue: 2, identifier: "com.example.app", label: "root",
      value: nil, frame: CGRect(x: 0, y: 0, width: 393, height: 852),
      isEnabled: true, isSelected: false,
      children: [
        TreeRoute.A11ySnapshotData(
          elementTypeRawValue: 9, identifier: "", label: "OK", value: nil,
          frame: CGRect(x: 10, y: 10, width: 100, height: 44),
          isEnabled: true, isSelected: false, children: [])
      ])
  }

  private func freePort() async throws -> UInt16 {
    // Bind a throwaway FlyingFox server on port 0, read the OS-assigned
    // port via `listeningAddress`, stop it, then hand the (now free)
    // port to the real `runForever`. Reuses the exact mechanism the
    // existing SmixRunnerServerBindTest uses — no raw-socket fiddling.
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

  func test_treeRoute_forwardsIncludeQuery_andLegacyBodyUnchanged() async throws {
    let port = try await freePort()
    let received = ScopeBox()
    let server = SmixRunnerServer()
    let appFrame = CGRect(x: 0, y: 0, width: 393, height: 852)
    let root = leaf()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { scope in
          await received.set(scope)
          return (root: root, appFrame: appFrame)
        }
      )
    }
    defer {
      runTask.cancel()
    }

    // Wait until the server answers /health.
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

    // No ?include= ⇒ handler receives nil ⇒ legacy path. Body must be
    // byte-identical to TreeRoute.serialize(root) (the zero-regression
    // anchor: the exact bytes existing consumers already parse).
    let (legacyBody, legacyResp) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/tree")!)
    XCTAssertEqual((legacyResp as? HTTPURLResponse)?.statusCode, 200)
    let scope1 = await received.value
    XCTAssertNil(scope1, "no ?include= ⇒ scope nil")
    let expected = TreeRoute.serialize(root, appFrame: appFrame, logSink: nil)
    XCTAssertEqual(legacyBody, expected,
      "no-param /tree body MUST equal legacy TreeRoute.serialize bytes")

    // ?include=all-windows ⇒ forwarded verbatim to the handler.
    _ = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/tree?include=all-windows")!)
    let scope2 = await received.value
    XCTAssertEqual(scope2, "all-windows",
      "?include=all-windows forwarded verbatim to SnapshotHandler")

    // Graceful stop (v1.4 C3 path) so runForever returns and the test
    // does not hang / leak a server.
    var req = URLRequest(url: URL(string: "http://127.0.0.1:\(port)/shutdown")!)
    req.httpMethod = "POST"
    _ = try? await URLSession.shared.data(for: req)
    _ = try? await runTask.value
  }

  func test_legacySerialize_byteShapeUnchanged() {
    let root = leaf()
    let appFrame = CGRect(x: 0, y: 0, width: 393, height: 852)
    let a = TreeRoute.serialize(root, appFrame: appFrame, logSink: nil)
    let b = TreeRoute.serialize(root, appFrame: appFrame, logSink: nil)
    XCTAssertEqual(a, b)
    let obj = try? JSONSerialization.jsonObject(with: a) as? [String: Any]
    XCTAssertEqual(obj?["identifier"] as? String, "com.example.app")
    XCTAssertEqual(obj?["rawType"] as? String, "application")
  }

  // v1.4 ③-C1 (third restart) S3.a — `GET /system-popups?include=` mirrors
  // the same `?include=` query forwarding mechanism this file already
  // exercises for `GET /tree` (FlyingFox `[QueryItem]` string subscript,
  // method-independent). nil scope ⇒ handler receives nil ⇒ the empty
  // popups envelope is byte-identical to `SystemPopupsRoute.success(popups:[])`.
  func test_systemPopupsRoute_forwardsIncludeQuery_andEmptyEnvelopeStable() async throws {
    let port = try await freePortPopups()
    let received = ScopePopupsBox()
    let server = SmixRunnerServer()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { scope in
          await received.set(scope)
          return []
        }
      )
    }
    defer { runTask.cancel() }

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

    let (body, _) = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups")!)
    let s1 = await received.value
    XCTAssertEqual(s1, .some(nil),
      "no ?include= on GET /system-popups ⇒ scope nil (legacy/empty path)")
    let expected = try await SystemPopupsRoute.success(popups: []).bodyData
    XCTAssertEqual(body, expected,
      "empty-popups GET /system-popups body byte-identical to SystemPopupsRoute.success(popups:[])")

    var req = URLRequest(url: URL(string: "http://127.0.0.1:\(port)/shutdown")!)
    req.httpMethod = "POST"
    _ = try? await URLSession.shared.data(for: req)
    _ = try? await runTask.value
  }

  func test_systemPopupsRoute_includeAllWindows_forwardedVerbatim() async throws {
    let port = try await freePortPopups()
    let received = ScopePopupsBox()
    let server = SmixRunnerServer()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupsHandler: { scope in
          await received.set(scope)
          return []
        }
      )
    }
    defer { runTask.cancel() }

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

    _ = try await URLSession.shared.data(
      from: URL(string: "http://127.0.0.1:\(port)/system-popups?include=all-windows")!)
    let s = await received.value
    XCTAssertEqual(s, .some("all-windows"),
      "?include=all-windows forwarded verbatim to SystemPopupsHandler")

    var req = URLRequest(url: URL(string: "http://127.0.0.1:\(port)/shutdown")!)
    req.httpMethod = "POST"
    _ = try? await URLSession.shared.data(for: req)
    _ = try? await runTask.value
  }

  private func freePortPopups() async throws -> UInt16 {
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

  actor ScopeBox {
    private var _v: String?
    func set(_ v: String?) { _v = v }
    var value: String? { _v }
  }

  actor ScopePopupsBox {
    private var _v: String??
    func set(_ v: String?) { _v = .some(v) }
    var value: String?? { _v }
  }
}

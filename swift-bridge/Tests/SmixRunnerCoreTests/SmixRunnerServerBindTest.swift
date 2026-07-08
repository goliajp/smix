import XCTest
import FlyingFox
import FlyingSocks
@testable import SmixRunnerCore

/// v1.2 C1 — Runner port localhost-only invariant (#3).
///
/// `FlyingFox.HTTPServer(port:)` default binds IPv6 wildcard `::` (= all
/// interfaces). Any LAN host could then GET /tree to read the SUT a11y tree
/// or POST /tap to control the UI. This test asserts SmixRunnerServer.makeServer
/// returns a server that binds **loopback only** (IPv4 127.0.0.1 or IPv6 ::1),
/// never wildcard.
final class SmixRunnerServerBindTest: XCTestCase {

  func test_makeServer_bindsLoopbackOnly() async throws {
    let server = SmixRunnerServer.makeServer(port: 0)

    let task = Task<Void, Never> {
      _ = try? await server.run()
    }
    defer {
      task.cancel()
    }

    var addr: FlyingSocks.Socket.Address?
    for _ in 0..<100 {
      addr = await server.listeningAddress
      if addr != nil { break }
      try await Task.sleep(nanoseconds: 50_000_000)
    }

    await server.stop(timeout: 0.5)

    guard let addr else {
      XCTFail("server did not bind within 5 seconds")
      return
    }

    switch addr {
    case .ip4(let ip, _):
      XCTAssertEqual(ip, "127.0.0.1",
        "bind must be IPv4 loopback (127.0.0.1), got \(ip). v1.2 invariant #3.")
    case .ip6(let ip, _):
      XCTAssertEqual(ip, "::1",
        "bind must be IPv6 loopback (::1), got \(ip). v1.2 invariant #3.")
    case .unix:
      XCTFail("unexpected unix socket bind for Runner port")
    }
  }

  /// Regression gate for v1.2 C2 bench failure: the bind-only test passes
  /// when the server binds IPv6 ::1, but SDK RunnerClient connects via
  /// IPv4 127.0.0.1 — these are separate kernel addresses, so the bench
  /// fails to reach the runner. This test verifies an actual IPv4 client
  /// can connect to the server, catching IPv4/IPv6 mismatches.
  func test_makeServer_isReachableFromIPv4Client() async throws {
    let server = SmixRunnerServer.makeServer(port: 0)

    let task = Task<Void, Never> {
      _ = try? await server.run()
    }
    defer { task.cancel() }

    var port: UInt16 = 0
    for _ in 0..<100 {
      if let a = await server.listeningAddress {
        switch a {
        case .ip4(_, let p), .ip6(_, let p):
          port = p
        case .unix:
          break
        }
        if port != 0 { break }
      }
      try await Task.sleep(nanoseconds: 50_000_000)
    }
    XCTAssertNotEqual(port, 0, "server did not bind")

    let url = URL(string: "http://127.0.0.1:\(port)/health")!
    do {
      let (data, response) = try await URLSession.shared.data(from: url)
      let status = (response as? HTTPURLResponse)?.statusCode ?? 0
      XCTAssertEqual(status, 404,
        "expected 404 from unhandled /health route (no handler registered), got status=\(status). " +
        "If you got a connection error, the server bound an address IPv4 clients can't reach.")
      _ = data
    } catch {
      XCTFail("IPv4 client (127.0.0.1) failed to connect: \(error). " +
        "Server bind address probably IPv6-only — v1.2 invariant #3 broken in practice.")
    }

    await server.stop(timeout: 0.5)
  }
}

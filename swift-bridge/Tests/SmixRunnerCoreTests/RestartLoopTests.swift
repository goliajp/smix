import XCTest
import FlyingFox
@testable import SmixRunnerCore

// Runtime verification (methodology §2 — source-read is necessary, not
// sufficient) that the in-process soft-cycle bounce works: `runServerLoop`
// re-binds the SAME FlyingFox server on the SAME actor when the restart
// signal fires, WITHOUT ending the loop, and returns normally only on
// shutdown. No simulator / no XCUITest — this exercises the exact server
// lifecycle `test_runForever` runs, in-process, on a real socket.
final class RestartLoopTests: XCTestCase {

  // Records whether the loop returned, without a non-cancellable
  // `await task.value` (which would wedge a timeout guard).
  private actor DoneFlag {
    private(set) var done = false
    func mark() { done = true }
    func value() -> Bool { done }
  }

  private func session() -> URLSession {
    let cfg = URLSessionConfiguration.ephemeral
    cfg.timeoutIntervalForRequest = 3
    return URLSession(configuration: cfg)
  }

  // A fixed free port, mirroring the real runner (port 22087, not
  // ephemeral). The server MUST bind the same port on every restart, so
  // the test cannot use port 0 — that would reassign a new ephemeral port
  // on each `run()` and the client would keep hitting the dead old one.
  private func freePort() async throws -> UInt16 {
    let probe = SmixRunnerServer.makeServer(port: 0)
    let probeTask = Task { _ = try? await probe.run() }
    var port: UInt16 = 0
    for _ in 0..<200 {
      if let a = await probe.listeningAddress {
        switch a {
        case .ip4(_, let p), .ip6(_, let p): port = p
        case .unix: break
        }
        if port != 0 { break }
      }
      try await Task.sleep(nanoseconds: 25_000_000)
    }
    await probe.stop(timeout: 0)
    probeTask.cancel()
    XCTAssertNotEqual(port, 0, "probe server did not bind")
    return port
  }

  @discardableResult
  private func get(_ port: UInt16, _ path: String) async -> Int? {
    guard let url = URL(string: "http://127.0.0.1:\(port)\(path)") else { return nil }
    guard let (_, resp) = try? await session().data(from: url) else { return nil }
    return (resp as? HTTPURLResponse)?.statusCode
  }

  @discardableResult
  private func post(_ port: UInt16, _ path: String) async -> Int? {
    guard let url = URL(string: "http://127.0.0.1:\(port)\(path)") else { return nil }
    var req = URLRequest(url: url)
    req.httpMethod = "POST"
    guard let (_, resp) = try? await session().data(for: req) else { return nil }
    return (resp as? HTTPURLResponse)?.statusCode
  }

  private func waitPing(_ port: UInt16) async -> Bool {
    for _ in 0..<120 {
      if await get(port, "/ping") == 200 { return true }
      try? await Task.sleep(nanoseconds: 25_000_000)
    }
    return false
  }

  private func waitFlag(_ flag: DoneFlag) async -> Bool {
    for _ in 0..<120 {
      if await flag.value() { return true }
      try? await Task.sleep(nanoseconds: 25_000_000)
    }
    return false
  }

  private func makeServer(port: UInt16) async -> (HTTPServer, ShutdownSignal, RestartSignal) {
    let server = SmixRunnerServer.makeServer(port: port)
    let shutdown = ShutdownSignal()
    let restart = RestartSignal()
    await server.appendRoute("GET /ping") { _ in
      HTTPResponse(statusCode: .ok, body: Data("pong".utf8))
    }
    await server.appendRoute("POST /bounce") { _ in
      await restart.fire()
      return HTTPResponse(statusCode: .ok, body: Data("bounced".utf8))
    }
    await server.appendRoute("POST /shutdown") { _ in
      await shutdown.fire()
      return HTTPResponse(statusCode: .ok, body: Data("bye".utf8))
    }
    return (server, shutdown, restart)
  }

  // --- RestartSignal state machine ---

  func test_restartSignal_fireBeforeWait_isConsumedOnce() async {
    let sig = RestartSignal()
    await sig.fire()
    let first = await sig.wait()
    XCTAssertTrue(first, "a fire before wait must be consumed by the next wait")
  }

  func test_restartSignal_waitThenFire_returnsTrue() async {
    let sig = RestartSignal()
    let waiter = Task { await sig.wait() }
    try? await Task.sleep(nanoseconds: 50_000_000)
    await sig.fire()
    let fired = await waiter.value
    XCTAssertTrue(fired, "a parked wait must resume true when fired")
  }

  func test_restartSignal_cancelledWait_returnsFalse() async {
    let sig = RestartSignal()
    let waiter = Task { await sig.wait() }
    try? await Task.sleep(nanoseconds: 50_000_000)
    waiter.cancel()
    let fired = await waiter.value
    XCTAssertFalse(fired, "a cancelled wait must resume false, not leak")
  }

  // --- Loop: shutdown with no bounce returns (refactor-equivalence) ---

  func test_runServerLoop_shutdownWithoutBounce_returns() async throws {
    let port = try await freePort()
    let (server, shutdown, restart) = await makeServer(port: port)
    let flag = DoneFlag()
    let loop = Task {
      try? await SmixRunnerServer.runServerLoop(
        server, shutdownSignal: shutdown, restartSignal: restart, restartGraceSeconds: 2.0)
      await flag.mark()
    }
    let up = await waitPing(port)
    XCTAssertTrue(up, "server did not come up")
    _ = await post(port, "/shutdown")
    let returned = await waitFlag(flag)
    XCTAssertTrue(returned, "runServerLoop must return after /shutdown (no bounce)")
    loop.cancel()
  }

  // --- Loop: a restart bounces the server and it keeps serving ---

  func test_runServerLoop_restartBounces_keepsServing() async throws {
    let port = try await freePort()
    let (server, shutdown, restart) = await makeServer(port: port)
    let loop = Task {
      try? await SmixRunnerServer.runServerLoop(
        server, shutdownSignal: shutdown, restartSignal: restart, restartGraceSeconds: 2.0)
    }
    let up = await waitPing(port)
    XCTAssertTrue(up, "server did not come up")

    let bounceStatus = await post(port, "/bounce")
    XCTAssertEqual(bounceStatus, 200, "the /bounce response must survive the restart (grace flush)")

    let servedAfter = await waitPing(port)
    XCTAssertTrue(servedAfter, "server must serve again after the restart bounce")

    _ = await post(port, "/bounce")
    let servedAfterSecond = await waitPing(port)
    XCTAssertTrue(servedAfterSecond, "server must survive a second restart (signal re-arms)")

    loop.cancel()
  }

  // --- Loop: shutdown AFTER a bounce still returns ---

  func test_runServerLoop_shutdownAfterBounce_returns() async throws {
    let port = try await freePort()
    let (server, shutdown, restart) = await makeServer(port: port)
    let flag = DoneFlag()
    let loop = Task {
      try? await SmixRunnerServer.runServerLoop(
        server, shutdownSignal: shutdown, restartSignal: restart, restartGraceSeconds: 2.0)
      await flag.mark()
    }
    _ = await waitPing(port)
    _ = await post(port, "/bounce")
    let servedAfter = await waitPing(port)
    XCTAssertTrue(servedAfter, "server must serve again after the restart bounce")
    _ = await post(port, "/shutdown")
    let returned = await waitFlag(flag)
    XCTAssertTrue(returned, "runServerLoop must return after /shutdown following a bounce")
    loop.cancel()
  }
}

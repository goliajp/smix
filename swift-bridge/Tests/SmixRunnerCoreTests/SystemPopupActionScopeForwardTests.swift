import XCTest
import FlyingFox
@testable import SmixRunnerCore

// v4.2 c1 — G9 act side — POST /system-popup-action integration test.
// Drive the REAL `runForever` route via an IPv4 client on a fixed free
// port (mirrors SystemPopupsRouteTests / TreeScopeForwardTests). The
// real handler is mocked here to assert the wire envelope shape +
// handler-invocation contract.
final class SystemPopupActionScopeForwardTests: XCTestCase {

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

  private func shutdown(_ port: UInt16, _ task: Task<Void, Error>) async {
    var req = URLRequest(url: URL(string: "http://127.0.0.1:\(port)/shutdown")!)
    req.httpMethod = "POST"
    _ = try? await URLSession.shared.data(for: req)
    _ = try? await task.value
  }

  private func post(
    _ port: UInt16, path: String, body: Data
  ) async throws -> (Data, Int) {
    var req = URLRequest(url: URL(string: "http://127.0.0.1:\(port)\(path)")!)
    req.httpMethod = "POST"
    req.httpBody = body
    req.setValue("application/json", forHTTPHeaderField: "Content-Type")
    let (data, resp) = try await URLSession.shared.data(for: req)
    return (data, (resp as? HTTPURLResponse)?.statusCode ?? 0)
  }

  actor IdBox {
    private var _v: (String, String)?
    func set(_ p: String, _ b: String) { _v = (p, b) }
    var value: (String, String)? { _v }
  }

  // Assertion 1: handler returns .found ⇒ 200 {"ok":true} envelope; the
  // handler receives popupId + buttonId verbatim from the request body.
  func test_action_handlerReturnsFound_returns200OkEnvelope() async throws {
    let port = try await freePort()
    let received = IdBox()
    let server = SmixRunnerServer()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupActionHandler: { popupId, buttonId in
          await received.set(popupId, buttonId)
          return .found
        }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let body = Data(#"{"popupId":"p-1","buttonId":"b-open"}"#.utf8)
    let (data, code) = try await post(port, path: "/system-popup-action", body: body)
    XCTAssertEqual(code, 200)
    let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    XCTAssertEqual(obj?["ok"] as? Bool, true)
    let ids = await received.value
    XCTAssertEqual(ids?.0, "p-1")
    XCTAssertEqual(ids?.1, "b-open")

    await shutdown(port, runTask)
  }

  // Assertion 2: handler returns .notFound ⇒ 404 not_found envelope echoing
  // the input popupId + buttonId.
  func test_action_handlerReturnsNotFound_returns404NotFoundEnvelope() async throws {
    let port = try await freePort()
    let server = SmixRunnerServer()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupActionHandler: { _, _ in .notFound }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let body = Data(#"{"popupId":"p-x","buttonId":"b-x"}"#.utf8)
    let (data, code) = try await post(port, path: "/system-popup-action", body: body)
    XCTAssertEqual(code, 404)
    let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    XCTAssertEqual(obj?["ok"] as? Bool, false)
    XCTAssertEqual(obj?["error"] as? String, "not_found")
    XCTAssertEqual(obj?["popupId"] as? String, "p-x")
    XCTAssertEqual(obj?["buttonId"] as? String, "b-x")

    await shutdown(port, runTask)
  }

  // Assertion 3: malformed body (missing buttonId) ⇒ 400 bad_request;
  // handler is NOT invoked because decode fails before dispatch.
  func test_action_malformedBody_returns400BadRequest_handlerNotInvoked() async throws {
    let port = try await freePort()
    let received = IdBox()
    let server = SmixRunnerServer()

    let runTask = Task {
      try await server.runForever(
        port: port,
        tapHandler: { _, _ in .notFound },
        snapshotHandler: { _ in nil },
        systemPopupActionHandler: { popupId, buttonId in
          await received.set(popupId, buttonId)
          return .found
        }
      )
    }
    defer { runTask.cancel() }
    try await waitUp(port)

    let body = Data(#"{"popupId":"p-1"}"#.utf8)
    let (data, code) = try await post(port, path: "/system-popup-action", body: body)
    XCTAssertEqual(code, 400)
    let obj = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    XCTAssertEqual(obj?["ok"] as? Bool, false)
    XCTAssertEqual(obj?["error"] as? String, "bad_request")
    let ids = await received.value
    XCTAssertNil(ids, "handler must NOT be invoked when decode fails")

    await shutdown(port, runTask)
  }
}

import FlyingFox
import XCTest

@testable import SmixRunnerCore

// A handler that takes longer than 15 s is answered, not replaced.
//
// FlyingFox gives every handler 15 s by default and answers 500 when it
// runs out, discarding the handler's own answer. On a slow CI simulator one
// XCUITest query took 18 s inside `/fill`, so the step failed on a clock
// that neither the route nor the host had chosen. The host decides how long
// to wait; the server's own limit only has to outlast every such wait.
final class ServerTimeoutTests: XCTestCase {

  func test_a_handler_that_takes_sixteen_seconds_is_answered() async throws {
    let server = SmixRunnerServer.makeServer(port: 0)
    await server.appendRoute("POST /slow") { _ in
      try await Task.sleep(nanoseconds: 16_000_000_000)
      return HTTPResponse(statusCode: .ok, body: Data("done".utf8))
    }
    let runTask = Task { try await server.run() }
    defer { runTask.cancel() }
    var port: UInt16 = 0
    for _ in 0..<100 {
      if case .ip4(_, let p)? = await server.listeningAddress, p != 0 {
        port = p
        break
      }
      try await Task.sleep(nanoseconds: 50_000_000)
    }
    XCTAssertNotEqual(port, 0, "the server did not bind")

    var request = URLRequest(url: URL(string: "http://127.0.0.1:\(port)/slow")!)
    request.httpMethod = "POST"
    request.timeoutInterval = 60
    let (body, response) = try await URLSession.shared.data(for: request)
    let status = (response as? HTTPURLResponse)?.statusCode
    XCTAssertEqual(status, 200, "the server answered for the handler: \(String(decoding: body, as: UTF8.self))")
    XCTAssertEqual(String(decoding: body, as: UTF8.self), "done")
    await server.stop(timeout: 0.5)
  }

  func test_the_server_limit_outlasts_every_route_the_host_waits_for() {
    // the host's longest fixed wait is a relaunch with an activation and a
    // stall on top; the gate holds the exact figure against the table
    XCTAssertGreaterThan(SmixRunnerServer.handlerTimeoutSeconds, 15)
  }
}

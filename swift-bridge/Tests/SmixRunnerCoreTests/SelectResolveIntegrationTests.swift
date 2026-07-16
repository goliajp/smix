// Integration test: SmixRunnerServer + SelectResolveRoute.
//
// Boots a real HTTPServer on ephemeral port + registers /select/resolve*
// routes with a mock handler + POSTs via URLSession + asserts byte-exact
// response body. End-to-end verification that the SelectResolveRoute wire
// contract is correctly wired into the server.

import FlyingFox
import FlyingSocks
import Foundation
import XCTest
@testable import SmixRunnerCore

final class SelectResolveIntegrationTests: XCTestCase {

  func test_resolveEndpoint_returnsIdsList() async throws {
    try await withRunningServer { url in
      let body = #"{"treeJson":"{}","selectorJson":"{\"id\":\"btn-x\"}"}"#.data(using: .utf8)!
      let (data, response) = try await post(url: url.appendingPathComponent("select/resolve"), body: body)
      let status = (response as? HTTPURLResponse)?.statusCode ?? 0
      XCTAssertEqual(status, 200)
      let bodyString = String(data: data, encoding: .utf8) ?? ""
      XCTAssertEqual(bodyString, #"{"ids":["btn-a","btn-b"]}"#)
    }
  }

  func test_resolveCountEndpoint_returnsCount() async throws {
    try await withRunningServer { url in
      let body = #"{"treeJson":"{}","selectorJson":"{\"role\":\"button\"}"}"#.data(using: .utf8)!
      let (data, response) = try await post(url: url.appendingPathComponent("select/resolve-count"), body: body)
      let status = (response as? HTTPURLResponse)?.statusCode ?? 0
      XCTAssertEqual(status, 200)
      let bodyString = String(data: data, encoding: .utf8) ?? ""
      XCTAssertEqual(bodyString, #"{"count":42}"#)
    }
  }

  func test_resolveLabelsEndpoint_returnsLabelsList() async throws {
    try await withRunningServer { url in
      let body = #"{"treeJson":"{}","selectorJson":"{\"label\":\"Item\"}"}"#.data(using: .utf8)!
      let (data, response) = try await post(url: url.appendingPathComponent("select/resolve-labels"), body: body)
      let status = (response as? HTTPURLResponse)?.statusCode ?? 0
      XCTAssertEqual(status, 200)
      let bodyString = String(data: data, encoding: .utf8) ?? ""
      XCTAssertEqual(bodyString, #"{"labels":["First","Second"]}"#)
    }
  }

  func test_invalidJsonRequest_returns400() async throws {
    try await withRunningServer { url in
      let body = "not json".data(using: .utf8)!
      let (data, response) = try await post(url: url.appendingPathComponent("select/resolve"), body: body)
      let status = (response as? HTTPURLResponse)?.statusCode ?? 0
      XCTAssertEqual(status, 400)
      let bodyString = String(data: data, encoding: .utf8) ?? ""
      XCTAssertTrue(bodyString.contains(#""error""#))
    }
  }

  func test_handlerThrow_returns500() async throws {
    try await withRunningServer(failOnAction: .count) { url in
      let body = #"{"treeJson":"{}","selectorJson":"{}"}"#.data(using: .utf8)!
      let (data, response) = try await post(url: url.appendingPathComponent("select/resolve-count"), body: body)
      let status = (response as? HTTPURLResponse)?.statusCode ?? 0
      XCTAssertEqual(status, 500)
      let bodyString = String(data: data, encoding: .utf8) ?? ""
      XCTAssertTrue(bodyString.contains(#""error""#))
    }
  }

  // MARK: - helpers

  private func withRunningServer(
    failOnAction: SelectResolveRoute.Action? = nil,
    body: (URL) async throws -> Void
  ) async throws {
    let server = SmixRunnerServer.makeServer(port: 0)
    let handler: SmixRunnerServer.SelectResolveHandler = { request, action in
      if let failOnAction, failOnAction == action {
        throw NSError(domain: "test", code: 1, userInfo: [NSLocalizedDescriptionKey: "simulated FFI raise"])
      }
      switch action {
      case .resolve: return .ids(["btn-a", "btn-b"])
      case .count: return .count(42)
      case .labels: return .labels(["First", "Second"])
      }
    }
    await SmixRunnerServer.registerSelectResolveRoutes(server: server, handler: handler)

    let task = Task<Void, Never> { _ = try? await server.run() }
    defer { task.cancel() }

    var port: UInt16 = 0
    for _ in 0..<100 {
      if let a = await server.listeningAddress {
        switch a {
        case .ip4(_, let p), .ip6(_, let p): port = p
        case .unix: break
        }
        if port != 0 { break }
      }
      try await Task.sleep(nanoseconds: 50_000_000)
    }
    XCTAssertNotEqual(port, 0, "server did not bind")
    let url = URL(string: "http://127.0.0.1:\(port)/")!
    do {
      try await body(url)
    } catch {
      await server.stop(timeout: 0.5)
      throw error
    }
    await server.stop(timeout: 0.5)
  }

  private func post(url: URL, body: Data) async throws -> (Data, URLResponse) {
    var req = URLRequest(url: url)
    req.httpMethod = "POST"
    req.setValue("application/json", forHTTPHeaderField: "content-type")
    req.httpBody = body
    return try await URLSession.shared.data(for: req)
  }
}

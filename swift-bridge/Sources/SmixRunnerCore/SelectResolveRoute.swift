// v7.8 c1 — smix-runner-server route module for selector resolution.
//
// Handles 3 endpoints used by TS @smix/rn HttpSimRuntime + future
// Kotlin / Swift consumers that want HTTP-mediated FFI access:
//
//   POST /select/resolve         { treeJson, selectorJson } → { ids: [String] }
//   POST /select/resolve-count   { treeJson, selectorJson } → { count: u32 }
//   POST /select/resolve-labels  { treeJson, selectorJson } → { labels: [String] }
//
// Wire contract mirror npm/smix-rn/src/HttpRunner.ts. The actual FFI
// dispatch (calling SmixCoreFFIBindings.resolveSelector*) lives in the
// handler closure injected at server start by the test author (or by
// SmixRunnerUITests for production iOS sim usage). This route module
// is pure wire — Request decoder + Response encoder + bad-request envelope.

import FlyingFox
import Foundation

public enum SelectResolveRoute {

  // MARK: - Action discriminator + Result enum (v7.9 c1)

  /// The 3 select/resolve endpoints share Request + handler signature
  /// but differ in result shape. SelectResolveAction lets a single
  /// handler closure dispatch by action.
  public enum Action: Sendable, Equatable {
    case resolve
    case count
    case labels
  }

  /// Handler result — variant per action. Decoupled from Action so
  /// handler can return whichever shape matches its call site.
  public enum Result: Sendable, Equatable {
    case ids([String])
    case count(UInt32)
    case labels([String])
  }

  // MARK: - Request

  /// All 3 endpoints share the same request body.
  public struct Request: Sendable, Equatable {
    public let treeJson: String
    public let selectorJson: String

    public init(treeJson: String, selectorJson: String) {
      self.treeJson = treeJson
      self.selectorJson = selectorJson
    }
  }

  public struct DecodeError: Error, Equatable {
    public let reason: String
  }

  /// Decode JSON body `{"treeJson": "...", "selectorJson": "..."}`.
  public static func decode(_ body: Data) throws -> Request {
    let root: Any
    do {
      root = try JSONSerialization.jsonObject(with: body, options: [])
    } catch {
      throw DecodeError(reason: "invalid JSON: \(error)")
    }
    guard let obj = root as? [String: Any] else {
      throw DecodeError(reason: "expected JSON object at root")
    }
    guard let treeJson = obj["treeJson"] as? String else {
      throw DecodeError(reason: "missing or non-string 'treeJson'")
    }
    guard let selectorJson = obj["selectorJson"] as? String else {
      throw DecodeError(reason: "missing or non-string 'selectorJson'")
    }
    return Request(treeJson: treeJson, selectorJson: selectorJson)
  }

  // MARK: - Response builders

  /// `{ "ids": ["a", "b", ...] }`. Sorted-keys for deterministic byte shape.
  public static func successIds(_ ids: [String]) -> HTTPResponse {
    let payload = ["ids": ids]
    return jsonResponse(status: .ok, payload: payload)
  }

  /// `{ "count": N }`. UInt32 maps to JSON number.
  public static func successCount(_ count: UInt32) -> HTTPResponse {
    let payload: [String: Any] = ["count": Int(count)]
    return jsonResponse(status: .ok, payload: payload)
  }

  /// `{ "labels": ["...", "..."] }`. Sorted-keys deterministic shape.
  public static func successLabels(_ labels: [String]) -> HTTPResponse {
    let payload = ["labels": labels]
    return jsonResponse(status: .ok, payload: payload)
  }

  /// 400 envelope: `{ "error": "..." }`.
  public static func badRequest(reason: String) -> HTTPResponse {
    let payload = ["error": reason]
    return jsonResponse(status: .badRequest, payload: payload)
  }

  /// 500 envelope (FFI dispatch raised): `{ "error": "..." }`.
  public static func ffiError(reason: String) -> HTTPResponse {
    let payload = ["error": reason]
    return jsonResponse(status: .internalServerError, payload: payload)
  }

  // MARK: - Helpers

  private static func jsonResponse(status: HTTPStatusCode, payload: [String: Any]) -> HTTPResponse {
    let body = (try? JSONSerialization.data(
      withJSONObject: payload,
      options: [.sortedKeys]
    )) ?? Data()
    return HTTPResponse(
      statusCode: status,
      headers: [.contentType: "application/json"],
      body: body
    )
  }
}

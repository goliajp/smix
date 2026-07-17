import FlyingFox
import Foundation

/// `GET /health` response builder. The bare `body()` is the
/// backward-compatible shape (`{"ok": true}`), preserved so tools that
/// jq-parse the exact literal don't break. `bodyDetail(...)` emits the
/// extended payload with liveness counters that the Rust client parses
/// via `HttpRunnerClient::health_detail`.
public enum HealthRoute {
  /// The wire schemas this runner speaks.
  ///
  /// Kept equal to `smix_runner_wire::WIRE_SCHEMA_SUPPORTED`, which a test
  /// on the Rust side enforces by reading this line — the runner ships
  /// inside the CLI, so the two are one build and must agree.
  public static let wireSchemaSupported: [UInt32] = [1, 2]

  /// Legacy body — stable byte sequence.
  public static func body() -> Data {
    return Data(#"{"ok":true}"#.utf8)
  }

  /// Legacy response — used when no counters are wired in.
  public static func response() -> HTTPResponse {
    return HTTPResponse(
      statusCode: .ok,
      headers: [.contentType: "application/json"],
      body: body()
    )
  }

  /// Extended body carrying runner-side observability
  /// counters. Callers pass the currently-observed values; the JSON
  /// encode uses camelCase field names matching
  /// `smix_runner_wire::HealthResponse`.
  /// - Parameter negotiated: the schema settled on with the client that
  ///   asked, when it said what it speaks. Absent when nobody has.
  public static func bodyDetail(
    runnerVersion: String,
    uptimeMs: UInt64,
    lastRequestAtMs: UInt64,
    sessionsOpen: UInt32,
    activationsTotal: UInt64,
    negotiated: UInt32? = nil
  ) -> Data {
    let supports = wireSchemaSupported.map(String.init).joined(separator: ",")
    let negotiatedField = negotiated.map { ",\"negotiated\":\($0)" } ?? ""
    // Hand-serialize to keep the wire byte-stable across Swift JSON
    // encoder version drift. Legacy `{"ok":true}` remains a strict
    // prefix of the extended body's field list.
    let escapedVersion = runnerVersion
      .replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
    let json = """
      {"ok":true,\
      "runnerVersion":"\(escapedVersion)",\
      "uptimeMs":\(uptimeMs),\
      "lastRequestAtMs":\(lastRequestAtMs),\
      "sessionsOpen":\(sessionsOpen),\
      "activationsTotal":\(activationsTotal),\
      "wireSchema":{"supports":[\(supports)]\(negotiatedField)}}
      """
    return Data(json.utf8)
  }

  /// Extended response variant.
  public static func responseDetail(
    runnerVersion: String,
    uptimeMs: UInt64,
    lastRequestAtMs: UInt64,
    sessionsOpen: UInt32,
    activationsTotal: UInt64
  ) -> HTTPResponse {
    return HTTPResponse(
      statusCode: .ok,
      headers: [.contentType: "application/json"],
      body: bodyDetail(
        runnerVersion: runnerVersion,
        uptimeMs: uptimeMs,
        lastRequestAtMs: lastRequestAtMs,
        sessionsOpen: sessionsOpen,
        activationsTotal: activationsTotal
      )
    )
  }
}

import FlyingFox
import Foundation

/// `GET /health` response builder. The bare `body()` is the
/// backward-compatible shape (`{"ok": true}`), preserved so tools that
/// jq-parse the exact literal don't break. `bodyDetail(...)` emits the
/// extended payload with liveness counters that the Rust client parses
/// via `HttpRunnerClient::health_detail`.
public enum HealthRoute {
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
  public static func bodyDetail(
    runnerVersion: String,
    uptimeMs: UInt64,
    lastRequestAtMs: UInt64,
    sessionsOpen: UInt32,
    activationsTotal: UInt64
  ) -> Data {
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
      "activationsTotal":\(activationsTotal)}
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

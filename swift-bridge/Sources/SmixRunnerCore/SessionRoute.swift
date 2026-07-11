import FlyingFox
import Foundation

/// Session lifecycle routes — solves the activation storm at its root by
/// letting clients open a session, use it across many requests without
/// re-triggering `.activate()`, and close it when done.
///
/// Wire (all POST, JSON):
/// - `/session/open  {bundleId, activate}` → `{sessionId, activatedOnce, serverTimeMs}`
/// - `/session/close {sessionId}`          → `{ok}`
/// - `/session/renew-activation {sessionId}` → `{ok, activated}`
///
/// Session lookup on other routes: the client sends `Session-Id: <id>`
/// as a header on every subsequent request. When present, the runner's
/// `resolveApp()` skips per-request activation and reuses the cached
/// `XCUIApplication` binding tied to the session.
///
/// Backward compatibility: absent `Session-Id` header falls through to
/// the legacy per-request rebind path (as of v1.0.2, rate-limited to
/// at most one `.activate()` per 5 s per bundle-id).
public enum SessionRoute {
  // -- open ---------------------------------------------------------------

  public struct OpenRequest: Equatable, Sendable {
    public let bundleId: String
    public let activate: Bool
    public init(bundleId: String, activate: Bool) {
      self.bundleId = bundleId
      self.activate = activate
    }
  }

  public struct OpenResponse: Equatable, Sendable {
    public let sessionId: String
    public let activatedOnce: Bool
    public let serverTimeMs: UInt64
    public init(sessionId: String, activatedOnce: Bool, serverTimeMs: UInt64) {
      self.sessionId = sessionId
      self.activatedOnce = activatedOnce
      self.serverTimeMs = serverTimeMs
    }
  }

  public enum OpenDecodeError: Error, Equatable {
    case invalidJSON
    case wrongType(String)
    case missingBundleId
    case emptyBundleId
  }

  public static func decodeOpen(_ body: Data) throws -> OpenRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw OpenDecodeError.invalidJSON }
    guard let root = json as? [String: Any] else {
      throw OpenDecodeError.wrongType("root not object")
    }
    // bundleId required + non-empty (empty string → programmer error)
    guard let rawBundle = root["bundleId"] else { throw OpenDecodeError.missingBundleId }
    guard let bundle = rawBundle as? String else {
      throw OpenDecodeError.wrongType("bundleId not string")
    }
    if bundle.isEmpty { throw OpenDecodeError.emptyBundleId }
    let activate = (root["activate"] as? Bool) ?? false
    return OpenRequest(bundleId: bundle, activate: activate)
  }

  public static func openResponse(_ resp: OpenResponse) -> HTTPResponse {
    let sid = jsonEscape(resp.sessionId)
    let body = Data(#"""
      {"sessionId":"\#(sid)","activatedOnce":\#(resp.activatedOnce),"serverTimeMs":\#(resp.serverTimeMs)}
      """#.utf8)
    return envelope(.ok, body)
  }

  // -- close --------------------------------------------------------------

  public struct CloseRequest: Equatable, Sendable {
    public let sessionId: String
    public init(sessionId: String) {
      self.sessionId = sessionId
    }
  }

  public enum CloseDecodeError: Error, Equatable {
    case invalidJSON
    case wrongType(String)
    case missingSessionId
    case emptySessionId
  }

  public static func decodeClose(_ body: Data) throws -> CloseRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw CloseDecodeError.invalidJSON }
    guard let root = json as? [String: Any] else {
      throw CloseDecodeError.wrongType("root not object")
    }
    guard let raw = root["sessionId"] else { throw CloseDecodeError.missingSessionId }
    guard let sid = raw as? String else {
      throw CloseDecodeError.wrongType("sessionId not string")
    }
    if sid.isEmpty { throw CloseDecodeError.emptySessionId }
    return CloseRequest(sessionId: sid)
  }

  public static func closeResponse(ok: Bool) -> HTTPResponse {
    let body = Data(#"{"ok":\#(ok)}"#.utf8)
    return envelope(.ok, body)
  }

  // -- renew activation --------------------------------------------------

  public struct RenewRequest: Equatable, Sendable {
    public let sessionId: String
    public init(sessionId: String) {
      self.sessionId = sessionId
    }
  }

  public static func decodeRenew(_ body: Data) throws -> RenewRequest {
    // Same shape / rules as close.
    let close = try decodeClose(body)
    return RenewRequest(sessionId: close.sessionId)
  }

  public static func renewResponse(ok: Bool, activated: Bool) -> HTTPResponse {
    let body = Data(#"{"ok":\#(ok),"activated":\#(activated)}"#.utf8)
    return envelope(.ok, body)
  }

  // -- v1.0.4 §D5 close-all ----------------------------------------------

  public static func closeAllResponse(closed: Int) -> HTTPResponse {
    let body = Data(#"{"ok":true,"closed":\#(closed)}"#.utf8)
    return envelope(.ok, body)
  }

  // -- v1.0.7 §D5 diagnostic dump ----------------------------------------

  /// v1.0.10 §D5 — app-alive-cache observability counters (subset of
  /// [`AppAliveCache.Counters`] transferred over the wire).
  public struct AliveCacheCounters: Equatable, Sendable {
    public let markDeadTotal: UInt64
    public let markAliveTotal: UInt64
    public let suppressHitTotal: UInt64
    public let suppressMissTotal: UInt64
    public let reprobeAttemptedTotal: UInt64
    public let reprobeSucceededTotal: UInt64
    public let reprobeInvalidatedEarly: UInt64
    public let reprobeExhaustedWindow: UInt64
    public init(
      markDeadTotal: UInt64 = 0,
      markAliveTotal: UInt64 = 0,
      suppressHitTotal: UInt64 = 0,
      suppressMissTotal: UInt64 = 0,
      reprobeAttemptedTotal: UInt64 = 0,
      reprobeSucceededTotal: UInt64 = 0,
      reprobeInvalidatedEarly: UInt64 = 0,
      reprobeExhaustedWindow: UInt64 = 0
    ) {
      self.markDeadTotal = markDeadTotal
      self.markAliveTotal = markAliveTotal
      self.suppressHitTotal = suppressHitTotal
      self.suppressMissTotal = suppressMissTotal
      self.reprobeAttemptedTotal = reprobeAttemptedTotal
      self.reprobeSucceededTotal = reprobeSucceededTotal
      self.reprobeInvalidatedEarly = reprobeInvalidatedEarly
      self.reprobeExhaustedWindow = reprobeExhaustedWindow
    }
  }

  public struct DiagnosticSnapshot: Sendable {
    public let sessions: [SessionSummary]
    public let simHealth: String
    public let supervisorPid: UInt32?
    public let uptimeMs: UInt64
    /// v1.0.10 §D5 — app-alive cache counters. `nil` when the runner
    /// was booted without `appAliveCache` wired in (opt-out).
    public let aliveCache: AliveCacheCounters?
    public init(
      sessions: [SessionSummary],
      simHealth: String,
      supervisorPid: UInt32?,
      uptimeMs: UInt64,
      aliveCache: AliveCacheCounters? = nil
    ) {
      self.sessions = sessions
      self.simHealth = simHealth
      self.supervisorPid = supervisorPid
      self.uptimeMs = uptimeMs
      self.aliveCache = aliveCache
    }
  }

  public static func diagnosticResponse(_ snap: DiagnosticSnapshot) -> HTTPResponse {
    var s = #"{"recentSubprocesses":[],"sessions":["#
    for (i, entry) in snap.sessions.enumerated() {
      if i > 0 { s += "," }
      let sid = jsonEscape(entry.sessionId)
      let bid = jsonEscape(entry.bundleId)
      s += #"{"sessionId":""# + sid + #"","bundleId":""# + bid + #"","openedAtMs":\#(entry.openedAtMs),"lastActivatedAtMs":\#(entry.lastActivatedAtMs)}"#
    }
    let health = jsonEscape(snap.simHealth)
    let sup = snap.supervisorPid.map { String($0) } ?? "null"
    s += #"],"simHealth":""# + health + #"","supervisorPid":\#(sup),"uptimeMs":\#(snap.uptimeMs)"#
    if let c = snap.aliveCache {
      s += #","aliveCache":{"markDeadTotal":\#(c.markDeadTotal),"markAliveTotal":\#(c.markAliveTotal),"suppressHitTotal":\#(c.suppressHitTotal),"suppressMissTotal":\#(c.suppressMissTotal),"reprobeAttemptedTotal":\#(c.reprobeAttemptedTotal),"reprobeSucceededTotal":\#(c.reprobeSucceededTotal),"reprobeInvalidatedEarly":\#(c.reprobeInvalidatedEarly),"reprobeExhaustedWindow":\#(c.reprobeExhaustedWindow)}"#
    }
    s += "}"
    return envelope(.ok, Data(s.utf8))
  }

  // -- v1.0.5 §D1 list ---------------------------------------------------

  public struct SessionSummary: Equatable, Sendable {
    public let sessionId: String
    public let bundleId: String
    public let openedAtMs: UInt64
    public let lastActivatedAtMs: UInt64
    public init(
      sessionId: String,
      bundleId: String,
      openedAtMs: UInt64,
      lastActivatedAtMs: UInt64
    ) {
      self.sessionId = sessionId
      self.bundleId = bundleId
      self.openedAtMs = openedAtMs
      self.lastActivatedAtMs = lastActivatedAtMs
    }
  }

  public static func listResponse(_ sessions: [SessionSummary]) -> HTTPResponse {
    var s = #"{"sessions":["#
    for (i, entry) in sessions.enumerated() {
      if i > 0 { s += "," }
      let sid = jsonEscape(entry.sessionId)
      let bid = jsonEscape(entry.bundleId)
      s += #"{"sessionId":""# + sid + #"","bundleId":""# + bid + #"","openedAtMs":\#(entry.openedAtMs),"lastActivatedAtMs":\#(entry.lastActivatedAtMs)}"#
    }
    s += "]}"
    return envelope(.ok, Data(s.utf8))
  }

  // -- v1.0.4 §D14 relaunch-app ------------------------------------------

  public struct RelaunchRequest: Equatable, Sendable {
    public let sessionId: String
    public init(sessionId: String) {
      self.sessionId = sessionId
    }
  }

  public static func decodeRelaunch(_ body: Data) throws -> RelaunchRequest {
    // Same shape / rules as close.
    let close = try decodeClose(body)
    return RelaunchRequest(sessionId: close.sessionId)
  }

  public static func relaunchResponse(ok: Bool, wallMs: UInt64) -> HTTPResponse {
    let body = Data(#"{"ok":\#(ok),"wallMs":\#(wallMs)}"#.utf8)
    return envelope(.ok, body)
  }

  // -- v1.0.8 §D1 terminate-app / launch-app -----------------------------

  public struct AppLifecycleRequest: Equatable, Sendable {
    public let sessionId: String
    public init(sessionId: String) {
      self.sessionId = sessionId
    }
  }

  public static func decodeAppLifecycle(_ body: Data) throws -> AppLifecycleRequest {
    let close = try decodeClose(body)
    return AppLifecycleRequest(sessionId: close.sessionId)
  }

  public static func appLifecycleResponse(ok: Bool, wallMs: UInt64) -> HTTPResponse {
    let body = Data(#"{"ok":\#(ok),"wallMs":\#(wallMs)}"#.utf8)
    return envelope(.ok, body)
  }

  // -- shared error responses --------------------------------------------

  public static func notFound(reason: String) -> HTTPResponse {
    let r = jsonEscape(reason)
    let body = Data(#"{"ok":false,"error":"not_found","reason":"\#(r)"}"#.utf8)
    return envelope(.notFound, body)
  }

  public static func badRequest(reason: String) -> HTTPResponse {
    let r = jsonEscape(reason)
    let body = Data(#"{"ok":false,"error":"bad_request","reason":"\#(r)"}"#.utf8)
    return envelope(.badRequest, body)
  }

  private static func envelope(_ status: HTTPStatusCode, _ body: Data) -> HTTPResponse {
    HTTPResponse(
      statusCode: status,
      headers: [.contentType: "application/json"],
      body: body
    )
  }

  private static func jsonEscape(_ s: String) -> String {
    // Minimal escape: control chars, quote, backslash. Matches
    // ForegroundRoute / ScrollRoute policy — session ids are UUIDs so
    // this is defense-in-depth against a client sending a pathological
    // string.
    var out = ""
    out.reserveCapacity(s.count + 4)
    for c in s.unicodeScalars {
      switch c {
      case "\"": out += "\\\""
      case "\\": out += "\\\\"
      case "\n": out += "\\n"
      case "\r": out += "\\r"
      case "\t": out += "\\t"
      default:
        if c.value < 0x20 {
          out += String(format: "\\u%04x", c.value)
        } else {
          out.unicodeScalars.append(c)
        }
      }
    }
    return out
  }
}

import XCTest
import FlyingFox
@testable import SmixRunnerCore

// SessionRoute POCO unit tests — decode paths + every response builder.
// The session lifecycle (open/close/close-all/launch-app/terminate-app/
// relaunch-app/list/renew-activation) was the site of a multi-release
// crash chain; these tests lock the wire shapes to the Rust
// smix-runner-wire structs (SessionOpenResponse / SessionCloseResponse /
// SessionAppLifecycleResponse / SessionListResponse /
// DiagnosticDumpResponse — all camelCase).
//
// The session table / registration itself lives in SmixRunnerServer
// state and is exercised via integration; only decode + emission are
// unit-testable here.
final class SessionRouteTests: XCTestCase {
  // -- open: decode --

  func test_decodeOpen_validBody_returnsRequest() throws {
    let body = Data(#"{"bundleId":"com.example.app","activate":true}"#.utf8)
    let req = try SessionRoute.decodeOpen(body)
    XCTAssertEqual(req, SessionRoute.OpenRequest(bundleId: "com.example.app", activate: true))
  }

  func test_decodeOpen_missingActivate_defaultsFalse() throws {
    let body = Data(#"{"bundleId":"com.example.app"}"#.utf8)
    let req = try SessionRoute.decodeOpen(body)
    XCTAssertEqual(req, SessionRoute.OpenRequest(bundleId: "com.example.app", activate: false))
  }

  func test_decodeOpen_emptyBody_throwsInvalidJSON() {
    XCTAssertThrowsError(try SessionRoute.decodeOpen(Data())) { err in
      XCTAssertEqual(err as? SessionRoute.OpenDecodeError, .invalidJSON)
    }
  }

  func test_decodeOpen_malformedJSON_throwsInvalidJSON() {
    let body = Data("{not json".utf8)
    XCTAssertThrowsError(try SessionRoute.decodeOpen(body)) { err in
      XCTAssertEqual(err as? SessionRoute.OpenDecodeError, .invalidJSON)
    }
  }

  func test_decodeOpen_rootNotObject_throwsWrongType() {
    let body = Data(#"["com.example.app"]"#.utf8)
    XCTAssertThrowsError(try SessionRoute.decodeOpen(body)) { err in
      XCTAssertEqual(err as? SessionRoute.OpenDecodeError, .wrongType("root not object"))
    }
  }

  func test_decodeOpen_missingBundleId_throwsMissingBundleId() {
    let body = Data(#"{"activate":true}"#.utf8)
    XCTAssertThrowsError(try SessionRoute.decodeOpen(body)) { err in
      XCTAssertEqual(err as? SessionRoute.OpenDecodeError, .missingBundleId)
    }
  }

  func test_decodeOpen_bundleIdIsNumber_throwsWrongType() {
    let body = Data(#"{"bundleId":42}"#.utf8)
    XCTAssertThrowsError(try SessionRoute.decodeOpen(body)) { err in
      XCTAssertEqual(err as? SessionRoute.OpenDecodeError, .wrongType("bundleId not string"))
    }
  }

  func test_decodeOpen_emptyBundleId_throwsEmptyBundleId() {
    let body = Data(#"{"bundleId":""}"#.utf8)
    XCTAssertThrowsError(try SessionRoute.decodeOpen(body)) { err in
      XCTAssertEqual(err as? SessionRoute.OpenDecodeError, .emptyBundleId)
    }
  }

  // -- open: response --

  func test_openResponse_200ExactBody() async throws {
    let resp = SessionRoute.openResponse(
      SessionRoute.OpenResponse(sessionId: "abc-123", activatedOnce: true, serverTimeMs: 1234))
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"sessionId":"abc-123","activatedOnce":true,"serverTimeMs":1234}"#)
  }

  // -- close: decode --

  func test_decodeClose_validBody_returnsRequest() throws {
    let body = Data(#"{"sessionId":"abc-123"}"#.utf8)
    let req = try SessionRoute.decodeClose(body)
    XCTAssertEqual(req, SessionRoute.CloseRequest(sessionId: "abc-123"))
  }

  func test_decodeClose_emptyBody_throwsInvalidJSON() {
    XCTAssertThrowsError(try SessionRoute.decodeClose(Data())) { err in
      XCTAssertEqual(err as? SessionRoute.CloseDecodeError, .invalidJSON)
    }
  }

  func test_decodeClose_rootNotObject_throwsWrongType() {
    let body = Data(#""abc-123""#.utf8)
    XCTAssertThrowsError(try SessionRoute.decodeClose(body)) { err in
      // JSONSerialization (no fragment option) rejects a bare string at
      // the parse step, so this surfaces as invalidJSON, not wrongType.
      XCTAssertEqual(err as? SessionRoute.CloseDecodeError, .invalidJSON)
    }
  }

  func test_decodeClose_rootIsArray_throwsWrongType() {
    let body = Data(#"["abc-123"]"#.utf8)
    XCTAssertThrowsError(try SessionRoute.decodeClose(body)) { err in
      XCTAssertEqual(err as? SessionRoute.CloseDecodeError, .wrongType("root not object"))
    }
  }

  func test_decodeClose_missingSessionId_throwsMissingSessionId() {
    let body = Data("{}".utf8)
    XCTAssertThrowsError(try SessionRoute.decodeClose(body)) { err in
      XCTAssertEqual(err as? SessionRoute.CloseDecodeError, .missingSessionId)
    }
  }

  func test_decodeClose_sessionIdIsNumber_throwsWrongType() {
    let body = Data(#"{"sessionId":7}"#.utf8)
    XCTAssertThrowsError(try SessionRoute.decodeClose(body)) { err in
      XCTAssertEqual(err as? SessionRoute.CloseDecodeError, .wrongType("sessionId not string"))
    }
  }

  func test_decodeClose_emptySessionId_throwsEmptySessionId() {
    let body = Data(#"{"sessionId":""}"#.utf8)
    XCTAssertThrowsError(try SessionRoute.decodeClose(body)) { err in
      XCTAssertEqual(err as? SessionRoute.CloseDecodeError, .emptySessionId)
    }
  }

  // -- close: response --

  func test_closeResponse_okTrue_200ExactBody() async throws {
    let resp = SessionRoute.closeResponse(ok: true)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":true}"#)
  }

  func test_closeResponse_okFalse_200ExactBody() async throws {
    let resp = SessionRoute.closeResponse(ok: false)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false}"#)
  }

  // -- renew-activation --

  func test_decodeRenew_validBody_returnsRequest() throws {
    let body = Data(#"{"sessionId":"abc-123"}"#.utf8)
    let req = try SessionRoute.decodeRenew(body)
    XCTAssertEqual(req, SessionRoute.RenewRequest(sessionId: "abc-123"))
  }

  func test_decodeRenew_missingSessionId_rethrowsCloseDecodeError() {
    // decodeRenew delegates to decodeClose, so its failures surface as
    // CloseDecodeError — handlers catch that type, so lock it in.
    XCTAssertThrowsError(try SessionRoute.decodeRenew(Data("{}".utf8))) { err in
      XCTAssertEqual(err as? SessionRoute.CloseDecodeError, .missingSessionId)
    }
  }

  func test_renewResponse_200ExactBody() async throws {
    let resp = SessionRoute.renewResponse(ok: true, activated: false)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":true,"activated":false}"#)
  }

  // -- close-all --

  func test_closeAllResponse_200ExactBody() async throws {
    let resp = SessionRoute.closeAllResponse(closed: 3)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":true,"closed":3}"#)
  }

  // -- relaunch-app --

  func test_decodeRelaunch_validBody_returnsRequest() throws {
    let body = Data(#"{"sessionId":"abc-123"}"#.utf8)
    let req = try SessionRoute.decodeRelaunch(body)
    XCTAssertEqual(req, SessionRoute.RelaunchRequest(sessionId: "abc-123"))
  }

  func test_decodeRelaunch_missingSessionId_rethrowsCloseDecodeError() {
    XCTAssertThrowsError(try SessionRoute.decodeRelaunch(Data("{}".utf8))) { err in
      XCTAssertEqual(err as? SessionRoute.CloseDecodeError, .missingSessionId)
    }
  }

  func test_relaunchResponse_200ExactBody() async throws {
    let resp = SessionRoute.relaunchResponse(ok: true, wallMs: 88)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":true,"wallMs":88}"#)
  }

  // -- terminate-app / launch-app: decode --

  func test_decodeAppLifecycle_fullBody_returnsRequest() throws {
    let body = Data(
      #"{"sessionId":"s1","args":["-flag"],"env":{"K":"V"},"waitForForegroundMs":15000,"waitForInteractiveMs":30000}"#
        .utf8)
    let req = try SessionRoute.decodeAppLifecycle(body)
    XCTAssertEqual(
      req,
      SessionRoute.AppLifecycleRequest(
        sessionId: "s1",
        args: ["-flag"],
        env: ["K": "V"],
        waitForForegroundMs: 15000,
        waitForInteractiveMs: 30000))
  }

  func test_decodeAppLifecycle_legacyBareSessionId_returnsDefaults() throws {
    let body = Data(#"{"sessionId":"s1"}"#.utf8)
    let req = try SessionRoute.decodeAppLifecycle(body)
    XCTAssertEqual(req, SessionRoute.AppLifecycleRequest(sessionId: "s1"))
  }

  func test_decodeAppLifecycle_missingSessionId_rethrowsCloseDecodeError() {
    XCTAssertThrowsError(try SessionRoute.decodeAppLifecycle(Data("{}".utf8))) { err in
      XCTAssertEqual(err as? SessionRoute.CloseDecodeError, .missingSessionId)
    }
  }

  func test_decodeAppLifecycle_malformedArgs_fallsBackToLegacyShape() throws {
    // Deliberate back-compat behaviour: when the full-body decode fails
    // (args not an array), the route falls back to the legacy bare
    // sessionId shape and drops the malformed extras.
    let body = Data(#"{"sessionId":"s1","args":"notarray"}"#.utf8)
    let req = try SessionRoute.decodeAppLifecycle(body)
    XCTAssertEqual(req, SessionRoute.AppLifecycleRequest(sessionId: "s1"))
  }

  // -- terminate-app / launch-app: response --

  func test_appLifecycleResponse_defaults_200ExactBody() async throws {
    let resp = SessionRoute.appLifecycleResponse(ok: true, wallMs: 150)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(
      body,
      #"{"ok":true,"wallMs":150,"waitedMs":0,"terminalState":0,"terminatedCooperatively":false,"reachedInteractive":false,"interactiveNamedIds":[]}"#
    )
  }

  func test_appLifecycleResponse_allFields_200ExactBody() async throws {
    let resp = SessionRoute.appLifecycleResponse(
      ok: true,
      wallMs: 2500,
      waitedMs: 1200,
      terminalState: 4,
      terminatedCooperatively: true,
      reachedInteractive: true,
      interactiveNamedIds: ["home-tab", "profile-btn"])
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(
      body,
      #"{"ok":true,"wallMs":2500,"waitedMs":1200,"terminalState":4,"terminatedCooperatively":true,"reachedInteractive":true,"interactiveNamedIds":["home-tab","profile-btn"]}"#
    )
  }

  // -- list --

  func test_listResponse_empty_200ExactBody() async throws {
    let resp = SessionRoute.listResponse([])
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"sessions":[]}"#)
  }

  func test_listResponse_oneEntry_200ExactBody() async throws {
    let resp = SessionRoute.listResponse([
      SessionRoute.SessionSummary(
        sessionId: "s-1",
        bundleId: "com.example.app",
        openedAtMs: 100,
        lastActivatedAtMs: 200,
        interactiveNamedIds: ["home-tab"])
    ])
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(
      body,
      #"{"sessions":[{"sessionId":"s-1","bundleId":"com.example.app","openedAtMs":100,"lastActivatedAtMs":200,"interactiveNamedIds":["home-tab"]}]}"#
    )
  }

  // -- diagnostic dump --
  // AppAliveCacheCountersTests covers the aliveCache wired-sentinel; here
  // the full body is locked field-for-field.

  func test_diagnosticResponse_fullSnapshot_200ExactBody() async throws {
    let snap = SessionRoute.DiagnosticSnapshot(
      sessions: [
        SessionRoute.SessionSummary(
          sessionId: "s-1",
          bundleId: "com.example.app",
          openedAtMs: 100,
          lastActivatedAtMs: 200,
          interactiveNamedIds: ["home-tab"])
      ],
      simHealth: "healthy",
      supervisorPid: 42,
      uptimeMs: 5000,
      aliveCache: SessionRoute.AliveCacheCounters(
        wired: true,
        markDeadTotal: 1,
        markAliveTotal: 2,
        suppressHitTotal: 3,
        suppressMissTotal: 4,
        reprobeAttemptedTotal: 5,
        reprobeSucceededTotal: 6,
        reprobeInvalidatedEarly: 7,
        reprobeExhaustedWindow: 8),
      sessionCounters: SessionRoute.SessionLifecycleCounters(
        openedTotal: 1,
        closedTotal: 2,
        relaunchAppTotal: 3,
        terminateAppTotal: 4,
        terminateAppViaXCUIApplication: 5,
        terminateAppViaFallback: 6,
        launchAppTotal: 7,
        launchAppReachedForeground: 8,
        launchAppTimedOutBeforeForeground: 9,
        launchAppReachedInteractive: 10,
        launchAppTimedOutBeforeInteractive: 11),
      lastInteractiveNamedIds: ["home-tab", "profile-btn"])
    let resp = SessionRoute.diagnosticResponse(snap)
    XCTAssertEqual(resp.statusCode, .ok)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(
      body,
      #"{"recentSubprocesses":[],"sessions":[{"sessionId":"s-1","bundleId":"com.example.app","openedAtMs":100,"lastActivatedAtMs":200,"interactiveNamedIds":["home-tab"]}],"simHealth":"healthy","supervisorPid":42,"uptimeMs":5000,"aliveCache":{"wired":true,"markDeadTotal":1,"markAliveTotal":2,"suppressHitTotal":3,"suppressMissTotal":4,"reprobeAttemptedTotal":5,"reprobeSucceededTotal":6,"reprobeInvalidatedEarly":7,"reprobeExhaustedWindow":8},"sessionCounters":{"openedTotal":1,"closedTotal":2,"relaunchAppTotal":3,"terminateAppTotal":4,"terminateAppViaXCUIApplication":5,"terminateAppViaFallback":6,"launchAppTotal":7,"launchAppReachedForeground":8,"launchAppTimedOutBeforeForeground":9,"launchAppReachedInteractive":10,"launchAppTimedOutBeforeInteractive":11},"lastInteractiveNamedIds":["home-tab","profile-btn"]}"#
    )
  }

  func test_diagnosticResponse_nilSupervisorPid_emitsNull() async throws {
    let snap = SessionRoute.DiagnosticSnapshot(
      sessions: [], simHealth: "healthy", supervisorPid: nil, uptimeMs: 42)
    let resp = SessionRoute.diagnosticResponse(snap)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertTrue(body.contains(#""supervisorPid":null"#), body)
  }

  // -- shared error responses --

  func test_notFound_404ExactBody() async throws {
    let resp = SessionRoute.notFound(reason: "unknown session")
    XCTAssertEqual(resp.statusCode, .notFound)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false,"error":"not_found","reason":"unknown session"}"#)
  }

  func test_badRequest_400ExactBody() async throws {
    let resp = SessionRoute.badRequest(reason: "bad json")
    XCTAssertEqual(resp.statusCode, .badRequest)
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false,"error":"bad_request","reason":"bad json"}"#)
  }

  func test_badRequest_escapesQuoteAndNewline() async throws {
    let resp = SessionRoute.badRequest(reason: "he\"llo\nx")
    let body = try await String(decoding: resp.bodyData, as: UTF8.self)
    XCTAssertEqual(body, #"{"ok":false,"error":"bad_request","reason":"he\"llo\nx"}"#)
  }
}

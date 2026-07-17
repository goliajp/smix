// Session — runner-side session lifecycle guard over the FFI driving
// surface.
//
// A `Session` wraps an FFI `SmixSession` (the runner's cached
// application binding) plus the `SmixDriver` that opened it (used to
// reconcile against the runner's session list). Sessions cannot be
// reopened — a closed `Session` throws on further calls.
//
// Consumer flow:
//
//   let driver = SmixDriver(port: Smix.defaultRunnerPort)
//   let session = try await Session.open(driver, bundleId: "com.example.app")
//   defer { Task { try? await session.close() } }
//   // ... drive the app via an App handle ...
//   try await session.close()

import Foundation
import SmixCoreFFIBindings

/// Runner-side session guard.
///
/// A `Session` corresponds to a single `SmixDriver.openSession(...)` on
/// the runner. While it is open, the runner reuses the session's cached
/// `XCUIApplication` binding.
///
/// `Session` is a reference type so callers can pass it around and close
/// from a `defer` / `finally` block.
public final class Session: @unchecked Sendable {
    public let sessionId: String
    private let driver: SmixDriver
    private let session: SmixSession
    // Best-effort double-close guard. A duplicated close is idempotent
    // on the runner side, so the worst case of a race is one extra call.
    private var closed = false

    private init(driver: SmixDriver, session: SmixSession) {
        self.driver = driver
        self.session = session
        self.sessionId = session.id()
    }

    /// Open a session against `driver`'s runner for `bundleId`. The
    /// returned session names the runner's cached application binding
    /// that the app routes act on.
    public static func open(
        _ driver: SmixDriver,
        bundleId: String
    ) async throws -> Session {
        let ffiSession = try await driver.openSession(bundleId: bundleId, cancel: nil)
        return Session(driver: driver, session: ffiSession)
    }

    /// Probe the runner's session list and return `true` iff this
    /// session's id is still known to the runner. Consumers wire this to
    /// decide whether to keep using the session (persisted across
    /// `runner cycle`) or open a fresh one.
    public func stillValid() async throws -> Bool {
        try assertOpen()
        let json = try await driver.listSessions(cancel: nil)
        guard let data = json.data(using: .utf8) else {
            throw SessionError.decode("session list JSON not UTF-8")
        }
        struct SessionEntry: Decodable { let sessionId: String }
        struct Resp: Decodable { let sessions: [SessionEntry]? }
        let resp: Resp
        do {
            resp = try JSONDecoder().decode(Resp.self, from: data)
        } catch {
            throw SessionError.decode("session list decode: \(error)")
        }
        return resp.sessions?.contains(where: { $0.sessionId == sessionId }) ?? false
    }

    /// Terminate + launch the session's cached `XCUIApplication` in
    /// place, preserving the session id and its runner binding.
    public func relaunchApp() async throws {
        try assertOpen()
        try await session.relaunchApp(cancel: nil)
    }

    /// Re-issue `.activate()` on the session's cached binding so the app
    /// stays foregrounded across a long-running flow.
    ///
    /// Use when the client detects target-app drift (foreground steal,
    /// snapshot returning empty payloads, etc.).
    public func renewActivation() async throws {
        try assertOpen()
        try await session.renewActivation(cancel: nil)
    }

    /// Release the session on the runner (idempotent). A closed session
    /// throws on further calls.
    public func close() async throws {
        if closed { return }
        closed = true
        try await session.close(cancel: nil)
    }

    private func assertOpen() throws {
        if closed {
            throw SessionError.closed
        }
    }
}

/// Errors thrown by `Session`.
public enum SessionError: Error, LocalizedError {
    case closed
    case decode(String)

    public var errorDescription: String? {
        switch self {
        case .closed: return "session already closed"
        case let .decode(detail): return "session response decode failed: \(detail)"
        }
    }
}

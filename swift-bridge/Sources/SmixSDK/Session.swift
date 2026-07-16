// Session.
//
// Runner-side session lifecycle guard. Drives `POST /session/*` on the
// smix runner and attaches the returned `Session-Id` to every
// subsequent request from the associated `HttpSmixSimRuntime`.
//
// Consumer flow:
//
//   let runtime = HttpSmixSimRuntime(
//     baseURL: URL(string: "http://127.0.0.1:22087")!,
//     bundleId: "com.example.app"
//   )
//   let session = try await Session.open(runtime, activate: true)
//   defer {
//     Task { try? await session.close() }
//   }
//   let app = try await Smix.launchApp(
//     .bundleId("com.example.app"),
//     runtime,
//     runtime.selectorResolver
//   )
//   try await app.tap(.text("Sign In"))
//
// Or, with async/throws:
//
//   let session = try await Session.open(runtime, activate: true)
//   do {
//     // ... use runtime; every call carries Session-Id ...
//     try await session.close()
//   } catch {
//     try? await session.close()
//     throw error
//   }

import Foundation

/// Sim-health classification observed via the runner's
/// `X-Sim-Health` response header. Consumers subscribe via
/// `Session.stateStream` or poll `Session.state`.
public enum SessionState: String, Sendable {
    case healthy
    case degraded
    case cycling
    case dead

    static func from(header value: String) -> SessionState? {
        SessionState(rawValue: value.trimmingCharacters(in: .whitespaces).lowercased())
    }
}

/// Runner-side session guard.
///
/// A `Session` corresponds to a single `POST /session/open` on the
/// runner. While it is open, the associated `HttpSmixSimRuntime`
/// carries `Session-Id` on every request, and the runner reuses the
/// session's cached `XCUIApplication` binding — no per-request
/// `.activate()`.
///
/// `Session` is a reference type so callers can pass it around and
/// close from a `defer` / `finally` block. Sessions cannot be reopened
/// — a closed `Session` throws on further calls.
public final class Session: @unchecked Sendable {
    public let sessionId: String
    public let activatedOnce: Bool
    public let serverTimeMs: UInt64
    private weak var runtime: HttpSmixSimRuntime?
    // Best-effort double-close guard. Non-atomic read/write is fine
    // here — a duplicated `POST /session/close` is idempotent on the
    // runner side, so the worst case of a race is one extra request.
    private var closed = false

    /// Current sim-health classification. Updated automatically as the
    /// runtime parses `X-Sim-Health` headers. AsyncStream continuation
    /// is set at open time; nil when the runner sends no health header.
    private var currentState: SessionState = .healthy
    private var stateContinuation: AsyncStream<SessionState>.Continuation?

    /// Current classification. Reads a plain stored value; no atomic —
    /// health tracking is coarse polling, not lock-step precision.
    public var state: SessionState { currentState }

    /// Subscribe to state transitions. AsyncStream emits
    /// every time the runner's `X-Sim-Health` header changes. Buffers
    /// the most recent value.
    public lazy var stateStream: AsyncStream<SessionState> = {
        AsyncStream { continuation in
            self.stateContinuation = continuation
            continuation.yield(self.currentState)
        }
    }()

    internal func updateState(_ next: SessionState) {
        if next == currentState { return }
        currentState = next
        stateContinuation?.yield(next)
    }

    private init(
        sessionId: String,
        activatedOnce: Bool,
        serverTimeMs: UInt64,
        runtime: HttpSmixSimRuntime
    ) {
        self.sessionId = sessionId
        self.activatedOnce = activatedOnce
        self.serverTimeMs = serverTimeMs
        self.runtime = runtime
    }

    /// Open a session against the runtime's runner. `activate = true`
    /// makes the runner call `.activate()` on the target once at open
    /// time; `activate = false` opens a passive binding.
    ///
    /// The `sessionId` returned by the runner is stashed on the
    /// runtime; every subsequent request from that runtime carries the
    /// `Session-Id` header.
    public static func open(
        _ runtime: HttpSmixSimRuntime,
        activate: Bool = false
    ) async throws -> Session {
        struct Resp: Decodable {
            let sessionId: String
            let activatedOnce: Bool?
            let serverTimeMs: UInt64?
        }
        let resp: Resp = try await runtime.post(
            "/session/open",
            [
                "bundleId": runtime.bundleId,
                "activate": activate,
            ] as [String: Any]
        )
        runtime.setSessionId(resp.sessionId)
        let session = Session(
            sessionId: resp.sessionId,
            activatedOnce: resp.activatedOnce ?? false,
            serverTimeMs: resp.serverTimeMs ?? 0,
            runtime: runtime
        )
        // Install the header parser so every subsequent response
        // updates this session's state.
        runtime.setSessionStateSetter { [weak session] next in
            session?.updateState(next)
        }
        return session
    }

    /// Probe `/session/list` and return `true` iff this
    /// session's id is still known to the runner. Consumers wire this
    /// after a state transition to `cycling`/`dead` to decide whether
    /// to keep using the session (persisted across `runner cycle`) or
    /// open a fresh one.
    public func stillValid() async throws -> Bool {
        guard let runtime = runtime else { throw SessionError.closed }
        try assertOpen()
        struct SessionEntry: Decodable {
            let sessionId: String
        }
        struct Resp: Decodable {
            let sessions: [SessionEntry]?
        }
        let resp: Resp = try await runtime.post("/session/list", [String: String]())
        return resp.sessions?.contains(where: { $0.sessionId == sessionId }) ?? false
    }

    /// Instruct the runner to `terminate()` + `launch()`
    /// the session's cached `XCUIApplication` in place, preserving the
    /// session id and XCUITest binding. Returns wall-clock milliseconds.
    public func relaunchApp() async throws -> UInt64 {
        guard let runtime = runtime else { throw SessionError.closed }
        try assertOpen()
        struct Resp: Decodable {
            let ok: Bool?
            let wallMs: UInt64?
        }
        let resp: Resp = try await runtime.post(
            "/session/relaunch-app",
            ["sessionId": sessionId]
        )
        return resp.wallMs ?? 0
    }

    /// Ask the runner to re-issue `.activate()` on the session's
    /// cached binding. Subject to a 2 s per-session rate limit; when
    /// rate-limited, returns `false` (no-op, session still healthy).
    ///
    /// Use when the client detects target-app drift (foreground steal,
    /// snapshot returning empty payloads, etc.).
    public func renewActivation() async throws -> Bool {
        guard let runtime = runtime else {
            throw SessionError.closed
        }
        try assertOpen()
        struct Resp: Decodable {
            let ok: Bool?
            let activated: Bool?
        }
        let resp: Resp = try await runtime.post(
            "/session/renew-activation",
            ["sessionId": sessionId]
        )
        return resp.activated ?? false
    }

    /// Release the session — sends `POST /session/close` (idempotent),
    /// clears the `Session-Id` header from the runtime. Subsequent
    /// runtime requests fall through to the per-request rebind path
    /// (rate-limited to 1 activate / 5 s / bundle-id).
    public func close() async throws {
        if closed {
            return
        }
        closed = true
        guard let runtime = runtime else {
            return
        }
        // Best-effort — clear the client-side header regardless of
        // runner-side outcome.
        defer { runtime.setSessionId(nil) }
        try await runtime.postVoid(
            "/session/close",
            ["sessionId": sessionId]
        )
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

    public var errorDescription: String? {
        switch self {
        case .closed: return "session already closed"
        }
    }
}

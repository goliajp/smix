// HttpSmixSimRuntime — v1.0.3.
//
// URLSession-backed `SmixSimRuntime` speaking the SmixRunnerCore HTTP
// wire (mirror of TS `HttpSimRuntime` and Rust `HttpRunnerClient`).
// Adds session lifecycle: when `sessionId` is set via `setSessionId(_:)`,
// every subsequent request carries the `Session-Id` header so the
// runner reuses the session's cached `XCUIApplication` binding and
// skips per-request `.activate()`.
//
// Consumers who need session lifecycle use the `Session` type
// (Session.swift) which drives `/session/open|close|renew-activation`
// through this runtime. Consumers who want the legacy per-request path
// simply skip Session; every request goes out without `Session-Id`.

import Foundation
#if canImport(CoreGraphics)
import CoreGraphics
#endif

/// URLSession-backed runtime targeting a running SmixRunnerCore
/// instance at a client-supplied base URL. Session-aware.
///
/// Not thread-safe on `sessionId` mutation — call `setSessionId(_:)`
/// only from the same actor / dispatch queue that issues the requests.
public final class HttpSmixSimRuntime: SmixSimRuntime, @unchecked Sendable {
    public let baseURL: URL
    public let bundleId: String
    private let urlSession: URLSession
    private var sessionId: String?

    public init(
        baseURL: URL,
        bundleId: String,
        urlSession: URLSession = .shared
    ) {
        self.baseURL = baseURL
        self.bundleId = bundleId
        self.urlSession = urlSession
    }

    /// v1.0.3 — attach / clear the `Session-Id` header on every
    /// subsequent request. Typically driven by `Session.open` /
    /// `Session.close`. Consumers can call directly if they manage
    /// sessions manually.
    public func setSessionId(_ id: String?) {
        self.sessionId = id
    }

    /// The current session id, or nil when no session is attached.
    public var currentSessionId: String? {
        sessionId
    }

    // MARK: - SmixSimRuntime protocol

    public func launch(bundleId: String) async throws {
        try await postVoid("/sim/launch", ["bundleId": bundleId])
    }

    public func terminate(bundleId: String) async throws {
        try await postVoid("/sim/terminate", ["bundleId": bundleId])
    }

    public func snapshotTree() async throws -> A11yNode {
        struct Wrap: Decodable { let tree: A11yNode }
        let w: Wrap = try await post("/a11y/snapshot", [String: String]())
        return w.tree
    }

    public func synthesizeTap(at point: CGPoint) async throws {
        try await postVoid(
            "/input/tap",
            ["x": Double(point.x), "y": Double(point.y)]
        )
    }

    public func sendString(_ text: String) async throws {
        try await postVoid("/input/send-string", ["text": text])
    }

    public func pressKey(_ key: KeyName) async throws {
        try await postVoid("/input/press-key", ["key": key.wireName])
    }

    public func swipe(_ direction: SwipeDirection) async throws {
        try await postVoid("/input/swipe", ["direction": direction.wireName])
    }

    public func screenshot() async throws -> Data {
        struct Wrap: Decodable { let png: String }
        let w: Wrap = try await post("/sim/screenshot", [String: String]())
        guard let bytes = Data(base64Encoded: w.png) else {
            throw HttpSmixSimRuntimeError.malformedResponse(
                path: "/sim/screenshot",
                detail: "png base64 decode failed"
            )
        }
        return bytes
    }

    public func systemPopups() async throws -> [A11yNode] {
        struct Wrap: Decodable { let nodes: [A11yNode] }
        let w: Wrap = try await post("/sim/system-popups", [String: String]())
        return w.nodes
    }

    public func openUrl(_ url: URL) async throws {
        try await postVoid("/sim/open-url", ["url": url.absoluteString])
    }

    public func launchFresh(
        bundleId: String,
        clearState: Bool,
        clearKeychain: Bool,
        appPath: String?
    ) async throws {
        var body: [String: Any] = [
            "bundleId": bundleId,
            "clearState": clearState,
            "clearKeychain": clearKeychain,
        ]
        if let p = appPath {
            body["appPath"] = p
        }
        try await postVoid("/sim/launch-fresh", body)
    }

    public func launchFromPath(_ appPath: String) async throws {
        try await postVoid("/sim/launch-from-path", ["appPath": appPath])
    }

    public func synthesizeTapAtNormalized(nx: Double, ny: Double) async throws {
        try await postVoid("/input/tap-normalized", ["nx": nx, "ny": ny])
    }

    // MARK: - internal HTTP helpers

    /// POST that expects a JSON body back. Errors on non-2xx or decode
    /// failure.
    func post<Body: Encodable, Resp: Decodable>(
        _ path: String,
        _ body: Body
    ) async throws -> Resp {
        let data = try await requestData(path, body: body)
        do {
            return try JSONDecoder().decode(Resp.self, from: data)
        } catch {
            throw HttpSmixSimRuntimeError.malformedResponse(
                path: path,
                detail: "decode \(Resp.self): \(error)"
            )
        }
    }

    /// POST when body encoding uses `[String: Any]` (JSONSerialization
    /// path). Same shape as the strongly-typed `post`.
    func post<Resp: Decodable>(
        _ path: String,
        _ body: [String: Any]
    ) async throws -> Resp {
        let data = try await requestData(path, jsonObject: body)
        do {
            return try JSONDecoder().decode(Resp.self, from: data)
        } catch {
            throw HttpSmixSimRuntimeError.malformedResponse(
                path: path,
                detail: "decode \(Resp.self): \(error)"
            )
        }
    }

    /// POST that ignores the response body (used for void endpoints
    /// like `/sim/launch`). Errors on non-2xx.
    func postVoid<Body: Encodable>(_ path: String, _ body: Body) async throws {
        _ = try await requestData(path, body: body)
    }

    /// POST with `[String: Any]` body ignoring the response.
    func postVoid(_ path: String, _ body: [String: Any]) async throws {
        _ = try await requestData(path, jsonObject: body)
    }

    private func requestData<Body: Encodable>(
        _ path: String,
        body: Body
    ) async throws -> Data {
        let payload: Data
        do {
            payload = try JSONEncoder().encode(body)
        } catch {
            throw HttpSmixSimRuntimeError.encodeFailed(
                path: path,
                detail: "\(error)"
            )
        }
        return try await requestData(path, payload: payload)
    }

    private func requestData(
        _ path: String,
        jsonObject: [String: Any]
    ) async throws -> Data {
        let payload: Data
        do {
            payload = try JSONSerialization.data(withJSONObject: jsonObject)
        } catch {
            throw HttpSmixSimRuntimeError.encodeFailed(
                path: path,
                detail: "\(error)"
            )
        }
        return try await requestData(path, payload: payload)
    }

    /// Actual URLSession dispatch. Attaches `Session-Id` header when
    /// the runtime has one attached.
    private func requestData(
        _ path: String,
        payload: Data
    ) async throws -> Data {
        var request = URLRequest(url: baseURL.appendingPathComponent(String(path.dropFirst())))
        request.httpMethod = "POST"
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.setValue(bundleId, forHTTPHeaderField: "App-Bundle-Id")
        if let sid = sessionId {
            request.setValue(sid, forHTTPHeaderField: "Session-Id")
        }
        request.httpBody = payload
        let (data, response) = try await urlSession.data(for: request)
        guard let http = response as? HTTPURLResponse else {
            throw HttpSmixSimRuntimeError.nonHttpResponse(path: path)
        }
        guard (200 ..< 300).contains(http.statusCode) else {
            let body = String(data: data, encoding: .utf8) ?? ""
            throw HttpSmixSimRuntimeError.nonSuccessStatus(
                path: path,
                status: http.statusCode,
                body: body
            )
        }
        return data
    }
}

/// Errors thrown by `HttpSmixSimRuntime`.
public enum HttpSmixSimRuntimeError: Error, LocalizedError {
    case encodeFailed(path: String, detail: String)
    case nonHttpResponse(path: String)
    case nonSuccessStatus(path: String, status: Int, body: String)
    case malformedResponse(path: String, detail: String)

    public var errorDescription: String? {
        switch self {
        case let .encodeFailed(path, detail):
            return "smix runner \(path): encode failed: \(detail)"
        case let .nonHttpResponse(path):
            return "smix runner \(path): non-HTTP response"
        case let .nonSuccessStatus(path, status, body):
            return "smix runner \(path): HTTP \(status): \(body)"
        case let .malformedResponse(path, detail):
            return "smix runner \(path): malformed body: \(detail)"
        }
    }
}

// MARK: - key + direction wire names

extension KeyName {
    /// Wire string used by `POST /input/press-key {key}` — mirrors
    /// the Rust `KeyName` variant name in the smix-runner-wire crate.
    public var wireName: String {
        switch self {
        case .`return`: return "Return"
        case .delete: return "Delete"
        case .space: return "Space"
        case .tab: return "Tab"
        case .escape: return "Escape"
        case .enter: return "Enter"
        }
    }
}

extension SwipeDirection {
    /// Wire string used by `POST /input/swipe {direction}` — mirrors
    /// the Rust `SwipeDirection` variant name.
    public var wireName: String {
        rawValue  // enum's rawValue is already "up" / "down" / "left" / "right"
    }
}

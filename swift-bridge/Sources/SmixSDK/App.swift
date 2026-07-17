// App actor — drives the act/sense pipeline over the FFI driving
// surface. A tap is: driver.tree() → FFI resolveSelector → session.tapById.
//
// App holds both an FFI `SmixDriver` (tree / list) and an FFI
// `SmixSession` (act): a single tap uses the driver to snapshot and the
// session to act, so both handles are required.

import Foundation
import SmixCoreFFIBindings

/// A handle to a running app on the simulator. All methods are async
/// + Sendable to interop with Swift Concurrency.
///
/// Constructed via [`Smix.launchApp(_:port:)`]. Holds an FFI
/// [`SmixDriver`] (accessibility tree, session listing) and an FFI
/// [`SmixSession`] (acting on the target app), both speaking to the
/// smix runner over the FFI boundary.
public actor App {
    private let bundleId: String
    private let driver: SmixDriver
    private let session: SmixSession

    public init(bundleId: String, driver: SmixDriver, session: SmixSession) {
        self.bundleId = bundleId
        self.driver = driver
        self.session = session
    }

    /// Bundle identifier of the running target.
    public func bundleIdentifier() -> String { bundleId }

    // ---- act methods --------------------------------------------------

    /// Tap on the first element matching `selector`. Captures an a11y
    /// snapshot via the driver, resolves the selector via the Rust FFI
    /// core, picks the first match, and taps it by accessibility id.
    ///
    /// Throws [`ExpectationFailure`] with `code: .notFound` when the
    /// resolver returns zero matches — populates `visibleElements`
    /// with the first 20 nodes from the current tree for AI agent
    /// diagnosis.
    public func tap(_ selector: Selector, timeout: Duration = .seconds(5)) async throws {
        let id = try await resolveFirstOrThrow(selector: selector)
        _ = try await session.tapById(id: id, cancel: nil)
    }

    /// Type text into a selector. Element must be a text input.
    /// Snapshots the tree, resolves the selector, taps the target to
    /// focus it, then sends the string via the session's keyboard path.
    public func fill(_ selector: Selector, _ text: String) async throws {
        let id = try await resolveFirstOrThrow(selector: selector)
        _ = try await session.tapById(id: id, cancel: nil)
        try await session.inputText(text: text, cancel: nil)
    }

    /// Press a hardware-like key.
    public func pressKey(_ key: KeyName) async throws {
        try await session.pressKey(key: key.wireName, cancel: nil)
    }

    /// Shared snapshot + resolve + first-match-or-throw helper used by
    /// `tap` / `fill`. Returns the matched accessibility id. Throws
    /// [`ExpectationFailure`] with `code: .notFound` populated with
    /// `visibleElements` on miss.
    ///
    /// `driver.tree()` returns the tree already as a JSON string in the
    /// exact shape the resolver takes, so it is fed straight to
    /// `resolveSelector` with no re-encode; the tree is only decoded on
    /// the miss path, to populate `visibleElements`.
    internal func resolveFirstOrThrow(selector: Selector) async throws -> String {
        let treeJson = try await driver.tree(cancel: nil)
        let selectorJson = try encodeSelector(selector)
        let matches: [String]
        do {
            matches = try resolveSelector(treeJson: treeJson, selectorJson: selectorJson)
        } catch {
            throw ExpectationFailure(
                code: .unknown,
                message: "FFI resolveSelector raised: \(error)",
                selector: selector
            )
        }
        guard let firstId = matches.first else {
            let tree = try decodeTree(treeJson, selector: selector)
            throw ExpectationFailure(
                code: .notFound,
                message: "no candidates after spatial + index filters",
                selector: selector,
                visibleElements: tree.flatten().prefix(20).map { $0 },
                suggestions: [
                    "check `accessibilityIdentifier` is set on the target view",
                    "consider relaxing to `.text(...)` if the label is more stable than the id",
                ]
            )
        }
        return firstId
    }

    /// Internal helper — encode a selector to a JSON string for the FFI
    /// boundary. Throws ExpectationFailure(.unknown) on encode failure
    /// (which would be an SDK bug, not a test author issue).
    internal func encodeSelector(_ selector: Selector) throws -> String {
        let selData: Data
        do {
            selData = try JSONEncoder().encode(selector)
        } catch {
            throw ExpectationFailure(
                code: .unknown,
                message: "JSON encode failed before FFI dispatch: \(error)",
                selector: selector
            )
        }
        guard let selStr = String(data: selData, encoding: .utf8) else {
            throw ExpectationFailure(
                code: .unknown,
                message: "JSON bytes not UTF-8 — SDK bug",
                selector: selector
            )
        }
        return selStr
    }

    /// Internal helper — encode tree + selector to JSON strings for the
    /// FFI boundary (used by the Locator poll loop, which already holds a
    /// decoded tree).
    internal func encodeForFfi(tree: A11yNode, selector: Selector) throws -> (String, String) {
        let treeData: Data
        do {
            treeData = try JSONEncoder().encode(tree)
        } catch {
            throw ExpectationFailure(
                code: .unknown,
                message: "JSON encode failed before FFI dispatch: \(error)",
                selector: selector
            )
        }
        guard let treeStr = String(data: treeData, encoding: .utf8) else {
            throw ExpectationFailure(
                code: .unknown,
                message: "JSON bytes not UTF-8 — SDK bug",
                selector: selector
            )
        }
        return (treeStr, try encodeSelector(selector))
    }

    /// Internal helper — decode the driver's tree JSON string into an
    /// `A11yNode`. Throws ExpectationFailure(.unknown) on decode failure
    /// (a wire / SDK bug at the FFI boundary).
    internal func decodeTree(_ json: String, selector: Selector? = nil) throws -> A11yNode {
        guard let data = json.data(using: .utf8) else {
            throw ExpectationFailure(
                code: .unknown,
                message: "tree JSON not UTF-8 — SDK bug",
                selector: selector
            )
        }
        do {
            return try JSONDecoder().decode(A11yNode.self, from: data)
        } catch {
            throw ExpectationFailure(
                code: .unknown,
                message: "tree JSON decode failed: \(error)",
                selector: selector
            )
        }
    }

    /// Swipe in a cardinal direction.
    public func swipe(_ direction: SwipeDirection) async throws {
        try await session.swipeOnce(direction: direction.rawValue, cancel: nil)
    }

    /// Tap at a normalized coordinate (0..1) — escape hatch for targets
    /// with no accessibility semantics to select on.
    ///
    /// Both `nx` and `ny` must be in `[0.0, 1.0]` (normalized to the
    /// device's logical viewport). Out-of-range values throw
    /// [`ExpectationFailure`] with code `.wrongState`.
    public func tapAtCoord(_ nx: Double, _ ny: Double) async throws {
        guard nx >= 0.0, nx <= 1.0, ny >= 0.0, ny <= 1.0 else {
            throw ExpectationFailure(
                code: .wrongState,
                message: "tapAtCoord arguments out of [0,1] range: nx=\(nx), ny=\(ny)",
                suggestions: ["use normalized 0..1 — for raw points, divide by viewport size"]
            )
        }
        try await session.tapAtNormCoord(nx: nx, ny: ny, cancel: nil)
    }

    /// Terminate the running app.
    public func terminate() async throws {
        try await session.terminateApp(cancel: nil)
    }

    /// Terminate + launch the session's app in place.
    public func relaunch() async throws {
        try await session.relaunchApp(cancel: nil)
    }

    // ---- sense methods ------------------------------------------------

    /// Find a selector and return a deferred [`Locator`].
    public func find(_ selector: Selector) -> Locator {
        Locator(app: self, selector: selector)
    }

    /// Capture the current a11y tree.
    public func tree(scope: TreeScope = .focused) async throws -> A11yNode {
        try decodeTree(try await driver.tree(cancel: nil))
    }

    /// Internal helper used by Locator extension — same as `tree()`
    /// but accessible without `scope` parameter contention.
    internal func runtimeSnapshot() async throws -> A11yNode {
        try decodeTree(try await driver.tree(cancel: nil))
    }

    /// Collect system popup windows (permission prompts / system alerts).
    public func systemPopups() async throws -> [SystemPopup] {
        let json = try await session.systemPopups(cancel: nil)
        guard let data = json.data(using: .utf8) else {
            throw ExpectationFailure(
                code: .unknown,
                message: "system-popups JSON not UTF-8 — SDK bug"
            )
        }
        do {
            return try JSONDecoder().decode([SystemPopup].self, from: data)
        } catch {
            throw ExpectationFailure(
                code: .unknown,
                message: "system-popups JSON decode failed: \(error)"
            )
        }
    }
}

/// Hardware-like keys the runner can synthesize.
public enum KeyName: String, Sendable, Codable, Equatable, CaseIterable {
    case `return`
    case delete
    case space
    case tab
    case escape
    case enter

    /// The name the runner knows this key by. All but `enter` are their own
    /// rawValue; `enter` is a synonym the FFI does not carry, so it maps to
    /// `return`, which is the same keystroke. Passing `rawValue` directly
    /// would send "enter", which the runner rejects.
    var wireName: String {
        switch self {
        case .enter: return "return"
        default: return rawValue
        }
    }
}

/// Cardinal swipe directions (Maestro navigation convention).
public enum SwipeDirection: String, Sendable, Codable, Equatable, CaseIterable {
    case up, down, left, right
}

/// What to walk when capturing the a11y tree.
public enum TreeScope: String, Sendable, Codable, Equatable {
    case focused
    case all
    case systemPopups
}

// v7.1 c2 — App actor wires real pipeline via SmixSimRuntime protocol.
//
// Per master.md §1 v7.1 c2: tap = snapshot → FFI resolveSelector →
// IOHID synthesize. c1 stubs replaced for tap / launchApp; remaining
// act/sense methods still throw notImplemented(stage:api:).

import Foundation
import SmixCoreFFIBindings
#if canImport(CoreGraphics)
import CoreGraphics
#endif

/// A handle to a running app on the simulator. All methods are async
/// + Sendable to interop with Swift Concurrency.
///
/// Constructed via [`Smix.launchApp(_:runtime:)`]. Holds a reference
/// to the underlying [`SmixSimRuntime`] which performs the actual
/// XCUI / IOHID work.
public actor App {
    private let bundleId: String
    private let runtime: SmixSimRuntime

    public init(bundleId: String, runtime: SmixSimRuntime) {
        self.bundleId = bundleId
        self.runtime = runtime
    }

    /// Bundle identifier of the running target.
    public func bundleIdentifier() -> String { bundleId }

    // ---- act methods --------------------------------------------------

    /// Tap on the first element matching `selector`. Captures an a11y
    /// snapshot, resolves the selector via the Rust FFI core, picks the
    /// first match, and synthesizes a tap at its bounds center.
    ///
    /// Throws [`ExpectationFailure`] with `code: .notFound` when the
    /// resolver returns zero matches — populates `visibleElements`
    /// with the first 20 nodes from the current tree for AI agent
    /// diagnosis.
    public func tap(_ selector: Selector, timeout: Duration = .seconds(5)) async throws {
        let node = try await resolveFirstOrThrow(selector: selector)
        try await runtime.synthesizeTap(
            at: CGPoint(x: node.bounds.centerX, y: node.bounds.centerY)
        )
    }

    /// Type text into a selector. Element must be a text input.
    /// Snapshots tree, resolves selector, taps the target to focus it,
    /// then sends the string via the runtime's keyboard fast path.
    public func fill(_ selector: Selector, _ text: String) async throws {
        let node = try await resolveFirstOrThrow(selector: selector)
        try await runtime.synthesizeTap(at: CGPoint(x: node.bounds.centerX, y: node.bounds.centerY))
        try await runtime.sendString(text)
    }

    /// Press a hardware-like key.
    public func pressKey(_ key: KeyName) async throws {
        try await runtime.pressKey(key)
    }

    /// Shared snapshot + resolve + first-match-or-throw helper used by
    /// `tap` / `fill` / others. Throws [`ExpectationFailure`] with
    /// `code: .notFound` populated with `visibleElements` on miss.
    internal func resolveFirstOrThrow(selector: Selector) async throws -> A11yNode {
        let tree = try await runtime.snapshotTree()
        let (treeJson, selectorJson) = try encodeForFfi(tree: tree, selector: selector)
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
        guard let firstId = matches.first, let node = tree.findById(firstId) else {
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
        return node
    }

    /// Internal helper — encode tree + selector to JSON strings for the
    /// FFI boundary. Throws ExpectationFailure(.unknown) on encode
    /// failure (which would be an SDK bug, not a test author issue).
    internal func encodeForFfi(tree: A11yNode, selector: Selector) throws -> (String, String) {
        let treeData: Data
        let selData: Data
        do {
            treeData = try JSONEncoder().encode(tree)
            selData = try JSONEncoder().encode(selector)
        } catch {
            throw ExpectationFailure(
                code: .unknown,
                message: "JSON encode failed before FFI dispatch: \(error)",
                selector: selector
            )
        }
        guard let treeStr = String(data: treeData, encoding: .utf8),
              let selStr = String(data: selData, encoding: .utf8) else {
            throw ExpectationFailure(
                code: .unknown,
                message: "JSON bytes not UTF-8 — SDK bug",
                selector: selector
            )
        }
        return (treeStr, selStr)
    }

    /// Swipe in a cardinal direction.
    public func swipe(_ direction: SwipeDirection) async throws {
        try await runtime.swipe(direction)
    }

    /// Take a PNG screenshot of the visible screen.
    public func screenshot() async throws -> Data {
        try await runtime.screenshot()
    }

    /// Tap at a normalized coordinate (0..1) — escape hatch per §9 #3.
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
        try await runtime.synthesizeTapAtNormalized(nx: nx, ny: ny)
    }

    /// Terminate the running app.
    public func terminate() async throws {
        try await runtime.terminate(bundleId: bundleId)
    }

    /// Terminate + launch fresh.
    public func relaunch() async throws {
        try await runtime.terminate(bundleId: bundleId)
        try await runtime.launch(bundleId: bundleId)
    }

    // ---- sense methods ------------------------------------------------

    /// Find a selector and return a deferred [`Locator`].
    public func find(_ selector: Selector) -> Locator {
        Locator(app: self, selector: selector)
    }

    /// Capture the current a11y tree.
    public func tree(scope: TreeScope = .focused) async throws -> A11yNode {
        try await runtime.snapshotTree()
    }

    /// Internal helper used by Locator extension — same as `tree()`
    /// but accessible without `scope` parameter contention.
    internal func runtimeSnapshot() async throws -> A11yNode {
        try await runtime.snapshotTree()
    }

    /// Collect system popup windows.
    public func systemPopups() async throws -> [A11yNode] {
        try await runtime.systemPopups()
    }

    /// Open a URL via UIApplication.openURL.
    public func openUrl(_ url: URL) async throws {
        try await runtime.openUrl(url)
    }

    /// Launch fresh with state clearing (mirror Rust smix-sdk
    /// `App.launch_fresh`).
    ///
    /// - Parameters:
    ///   - clearState: wipe sandbox dirs before relaunch.
    ///   - clearKeychain: wipe keychain items for this bundle id.
    ///   - appPath: optional override (when reinstalling from a path).
    public func launchFresh(
        clearState: Bool = false,
        clearKeychain: Bool = false,
        appPath: String? = nil
    ) async throws {
        try await runtime.launchFresh(
            bundleId: bundleId,
            clearState: clearState,
            clearKeychain: clearKeychain,
            appPath: appPath
        )
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

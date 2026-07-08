// v7.1 c3 — Locator actor with host-side poll loop.
//
// Playwright-style: `app.find(.text("Welcome")).toBeVisible(timeout:)`.
// Each `.toBe…(...)` call polls the runtime snapshot at 250ms tick
// until either the predicate succeeds or the timeout deadline is hit.
// On timeout, throws [`ExpectationFailure`] with code = `.timeout` or
// `.notFound` populated with the last seen tree's visibleElements.

import Foundation
import SmixCoreFFIBindings
#if canImport(CoreGraphics)
import CoreGraphics
#endif

/// A deferred locator. Created by [`App.find`]. Each `.toBe…(...)` call
/// dispatches a poll loop and throws [`ExpectationFailure`] on timeout.
public actor Locator {
    private let app: App
    private let selector: Selector

    /// Poll tick — 250ms matches Rust smix-sdk default + Playwright.
    private static let pollTick: Duration = .milliseconds(250)

    public init(app: App, selector: Selector) {
        self.app = app
        self.selector = selector
    }

    /// Expect the element to become visible within `timeout`. Polls
    /// the runtime snapshot at 250ms intervals; success on first hit
    /// where the resolver returns at least one match.
    ///
    /// Throws [`ExpectationFailure`] with `code: .timeout` on deadline
    /// (populates `visibleElements` from the last polled tree).
    public func toBeVisible(timeout: Duration = .seconds(5)) async throws {
        try await poll(
            timeout: timeout,
            predicate: { tree, match in
                guard let match else { return false }
                return tree.findById(match)?.visible ?? false
            }
        )
    }

    /// Expect the element to contain `text` as a substring of either
    /// its `label`, `title`, or `text` field within `timeout`.
    public func toContainText(_ text: String, timeout: Duration = .seconds(5)) async throws {
        try await poll(
            timeout: timeout,
            predicate: { tree, match in
                guard let match, let node = tree.findById(match) else { return false }
                if node.label?.contains(text) == true { return true }
                if node.title?.contains(text) == true { return true }
                if node.text?.contains(text) == true { return true }
                return false
            }
        )
    }

    /// Expect the resolver to find exactly `n` matches within `timeout`.
    public func toHaveCount(_ n: Int, timeout: Duration = .seconds(5)) async throws {
        let deadline = Date().addingTimeInterval(timeout.seconds)
        var lastTree = try await app.snapshotForLocator()
        var lastCount = 0
        while Date() < deadline {
            let matchCount = try await app.matchCount(for: selector)
            lastCount = matchCount
            if matchCount == n {
                return
            }
            try await Task.sleep(for: Self.pollTick)
            lastTree = try await app.snapshotForLocator()
        }
        throw ExpectationFailure(
            code: .wrongState,
            message: "expected \(n) matches, last poll saw \(lastCount)",
            selector: selector,
            visibleElements: lastTree.flatten().prefix(20).map { $0 },
            suggestions: [
                "increase `timeout` if render is slow",
                "verify the selector matches the expected count in the current tree",
            ]
        )
    }

    /// Expect the matched element's `label` to equal the given string.
    public func toHaveLabel(_ label: String, timeout: Duration = .seconds(5)) async throws {
        try await poll(
            timeout: timeout,
            predicate: { tree, match in
                guard let match, let node = tree.findById(match) else { return false }
                return node.label == label
            }
        )
    }

    // MARK: - shared poll helper

    /// Poll snapshot + FFI resolve until `predicate(tree, firstMatchId)`
    /// returns true OR `timeout` expires. On expiry, throws
    /// `ExpectationFailure(.timeout)` populated with last seen tree.
    private func poll(
        timeout: Duration,
        predicate: (A11yNode, String?) -> Bool
    ) async throws {
        let deadline = Date().addingTimeInterval(timeout.seconds)
        var lastTree = try await app.snapshotForLocator()

        while Date() < deadline {
            lastTree = try await app.snapshotForLocator()
            let firstMatch = try await app.firstMatchId(for: selector, tree: lastTree)
            if predicate(lastTree, firstMatch) {
                return
            }
            try await Task.sleep(for: Self.pollTick)
        }

        // timeout expired — emit ExpectationFailure with last tree.
        throw ExpectationFailure(
            code: .timeout,
            message: "timed out after \(timeout) waiting for assertion",
            selector: selector,
            visibleElements: lastTree.flatten().prefix(20).map { $0 },
            suggestions: [
                "verify the selector resolves at all (try `.tap(...)` first)",
                "increase `timeout` if render is slow",
                "use `.toContainText(...)` if the label exists but doesn't match strictly",
            ]
        )
    }
}

// MARK: - Duration → TimeInterval bridge

extension Duration {
    /// Convert to `TimeInterval` (seconds, double) for `Date` arithmetic.
    var seconds: TimeInterval {
        let (s, attos) = components
        return Double(s) + Double(attos) / 1e18
    }
}

// MARK: - App helpers used by Locator (not part of public API)

extension App {
    /// Capture snapshot — used by Locator poll loops.
    internal func snapshotForLocator() async throws -> A11yNode {
        try await runtimeSnapshot()
    }

    /// FFI resolve to find the first match id (or nil) — used by
    /// Locator poll loops.
    internal func firstMatchId(for selector: Selector, tree: A11yNode) async throws -> String? {
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
        return matches.first
    }

    /// FFI resolve to count matches — used by `toHaveCount`.
    ///
    /// v8 polish (post-v7.6 c1): migrated from `resolveSelector` (returns
    /// `[String]` then `.count`) to dedicated `resolveSelectorCount`
    /// (returns `UInt32` directly). Skips index modifier per Playwright
    /// "match all" semantics — matches Kotlin v7.6 c1 + TS v7.6 c1 pattern.
    internal func matchCount(for selector: Selector) async throws -> Int {
        let tree = try await runtimeSnapshot()
        let (treeJson, selectorJson) = try encodeForFfi(tree: tree, selector: selector)
        let count: UInt32
        do {
            count = try resolveSelectorCount(treeJson: treeJson, selectorJson: selectorJson)
        } catch {
            throw ExpectationFailure(
                code: .unknown,
                message: "FFI resolveSelectorCount raised: \(error)",
                selector: selector
            )
        }
        return Int(count)
    }

    /// FFI resolve to fetch matched node labels — symmetric with Kotlin /
    /// TS labelsResolver path. Currently unused by Swift Locator (which
    /// uses snapshot-based predicate for toHaveLabel via findById), but
    /// exposed for consumer test authors who want per-match label access
    /// without round-tripping ids.
    internal func matchLabels(for selector: Selector) async throws -> [String] {
        let tree = try await runtimeSnapshot()
        let (treeJson, selectorJson) = try encodeForFfi(tree: tree, selector: selector)
        do {
            return try resolveSelectorLabels(treeJson: treeJson, selectorJson: selectorJson)
        } catch {
            throw ExpectationFailure(
                code: .unknown,
                message: "FFI resolveSelectorLabels raised: \(error)",
                selector: selector
            )
        }
    }
}

// SmixSimRuntime protocol + Mock impl.
//
// SmixSDK is XCTest-free (mirror SmixRunnerCore pattern). Test author
// provides a concrete runtime impl:
//   - An XCUITest-backed runtime (real, lives in the consumer's UITest
//     target — drop-in via this protocol; wraps XCUIApplication.snapshot
//     + EventSynthesizer IOHID dance)
//   - MockSimRuntime (here): deterministic in-memory tree + event
//     log. Used by SmixSDKTests for unit-level wire verification.
//
// This protocol + Mock impl never touch the app process; they live in
// test targets only.

import Foundation
#if canImport(CoreGraphics)
import CoreGraphics
#endif

/// Mock-recorded `launchFresh(...)` call — assertable from tests.
public struct LaunchFreshCall: Sendable, Equatable {
    public let bundleId: String
    public let clearState: Bool
    public let clearKeychain: Bool
    public let appPath: String?
    public init(
        bundleId: String,
        clearState: Bool,
        clearKeychain: Bool,
        appPath: String?
    ) {
        self.bundleId = bundleId
        self.clearState = clearState
        self.clearKeychain = clearKeychain
        self.appPath = appPath
    }
}

/// Low-level simulator runtime — captures + injects events on the
/// running app.
public protocol SmixSimRuntime: Sendable {
    /// Launch the target app on the simulator.
    func launch(bundleId: String) async throws
    /// Terminate the target app on the simulator.
    func terminate(bundleId: String) async throws
    /// Capture the current a11y snapshot of the focused / front-most
    /// app window. Returns the root node (children walk applies).
    func snapshotTree() async throws -> A11yNode
    /// Synthesize a single tap at logical-points coordinate (matches
    /// `A11yNode.bounds` coordinate space).
    func synthesizeTap(at point: CGPoint) async throws
    /// Send text via the runner's keyboard fast path (Apple
    /// `_XCT_sendString:maximumFrequency:`).
    func sendString(_ text: String) async throws
    /// Press a hardware-like key.
    func pressKey(_ key: KeyName) async throws
    /// Swipe in a cardinal direction across the visible viewport
    /// (maestro navigation convention: DOWN = content scrolls down,
    /// fingers move up).
    func swipe(_ direction: SwipeDirection) async throws
    /// Capture a PNG screenshot of the visible screen.
    func screenshot() async throws -> Data
    /// Collect system popup windows (permission prompts / system
    /// alerts that live outside the app's own window).
    func systemPopups() async throws -> [A11yNode]
    /// Open a URL via `UIApplication.openURL` (deeplink, etc.).
    func openUrl(_ url: URL) async throws
    /// Launch fresh with state clearing options (mirror Rust smix-sdk
    /// `App::launch_fresh`):
    ///   - `clearState`: wipe sandbox dirs (Documents / Library) before relaunch
    ///   - `clearKeychain`: wipe keychain items for this bundle id
    ///   - `appPath`: optional override (when reinstalling from a path)
    func launchFresh(
        bundleId: String,
        clearState: Bool,
        clearKeychain: Bool,
        appPath: String?
    ) async throws
    /// Install and launch an app from an absolute `.app` path. Mirror
    /// `xcrun simctl install + launch` flow for `--app-path` workflows.
    func launchFromPath(_ appPath: String) async throws
    /// Synthesize a tap at a normalized coordinate (0..1 over the
    /// device's logical viewport). Mirrors Rust smix-sdk `tap_at_coord`.
    func synthesizeTapAtNormalized(nx: Double, ny: Double) async throws
}

/// In-memory mock runtime — used by SmixSDKTests for deterministic
/// wire verification without booting a sim.
public final class MockSimRuntime: SmixSimRuntime, @unchecked Sendable {
    /// What `snapshotTree()` returns. Default = empty container only.
    /// Mutable so tests can stage state changes between calls (e.g.
    /// snapshot 1 = empty → snapshot 2 = with target node, then
    /// Locator.toBeVisible polls and finds it on iteration 2).
    public var snapshotResult: A11yNode

    /// Log of `synthesizeTap(at:)` calls — assertable from test.
    public private(set) var tapCalls: [CGPoint] = []
    /// Log of `launch(bundleId:)` calls.
    public private(set) var launchCalls: [String] = []
    /// Log of `terminate(bundleId:)` calls.
    public private(set) var terminateCalls: [String] = []
    /// Log of `sendString(_:)` calls.
    public private(set) var sendStringCalls: [String] = []
    /// Log of `pressKey(_:)` calls.
    public private(set) var pressKeyCalls: [KeyName] = []
    /// Log of `swipe(_:)` calls.
    public private(set) var swipeCalls: [SwipeDirection] = []
    /// Counter — how many times `screenshot()` was invoked.
    public private(set) var screenshotCalls: Int = 0
    /// What `screenshot()` returns. Settable by tests (default empty).
    public var screenshotResult: Data = Data()
    /// What `systemPopups()` returns. Settable by tests (default empty).
    public var systemPopupsResult: [A11yNode] = []
    /// Log of `openUrl(_:)` calls.
    public private(set) var openUrlCalls: [URL] = []
    /// Log of `launchFresh(...)` calls.
    public private(set) var launchFreshCalls: [LaunchFreshCall] = []
    /// Log of `launchFromPath(_:)` calls.
    public private(set) var launchFromPathCalls: [String] = []
    /// Log of `synthesizeTapAtNormalized(nx:ny:)` calls as `(nx, ny)` pairs.
    public private(set) var tapAtNormalizedCalls: [CGPoint] = []

    /// Optional hook fired AFTER each snapshotTree() — lets tests
    /// stage tree transitions ("first call empty, second call has
    /// target node"). Receives the call number (0-indexed); should
    /// mutate snapshotResult for the next call.
    public var afterSnapshot: ((_ callIndex: Int) -> Void)?

    /// Internal counter for afterSnapshot hook.
    private var snapshotCallCount = 0

    public init(snapshotResult: A11yNode? = nil) {
        self.snapshotResult = snapshotResult ?? Self.emptyDefault()
    }

    public func launch(bundleId: String) async throws {
        launchCalls.append(bundleId)
    }

    public func terminate(bundleId: String) async throws {
        terminateCalls.append(bundleId)
    }

    public func snapshotTree() async throws -> A11yNode {
        let result = snapshotResult
        let idx = snapshotCallCount
        snapshotCallCount += 1
        afterSnapshot?(idx)
        return result
    }

    public func synthesizeTap(at point: CGPoint) async throws {
        tapCalls.append(point)
    }

    public func sendString(_ text: String) async throws {
        sendStringCalls.append(text)
    }

    public func pressKey(_ key: KeyName) async throws {
        pressKeyCalls.append(key)
    }

    public func swipe(_ direction: SwipeDirection) async throws {
        swipeCalls.append(direction)
    }

    public func screenshot() async throws -> Data {
        screenshotCalls += 1
        return screenshotResult
    }

    public func systemPopups() async throws -> [A11yNode] {
        systemPopupsResult
    }

    public func openUrl(_ url: URL) async throws {
        openUrlCalls.append(url)
    }

    public func launchFresh(
        bundleId: String,
        clearState: Bool,
        clearKeychain: Bool,
        appPath: String?
    ) async throws {
        launchFreshCalls.append(
            LaunchFreshCall(
                bundleId: bundleId,
                clearState: clearState,
                clearKeychain: clearKeychain,
                appPath: appPath
            )
        )
    }

    public func launchFromPath(_ appPath: String) async throws {
        launchFromPathCalls.append(appPath)
    }

    public func synthesizeTapAtNormalized(nx: Double, ny: Double) async throws {
        tapAtNormalizedCalls.append(CGPoint(x: nx, y: ny))
    }

    /// Empty container default for tests that don't care about tree
    /// content (e.g. testing the throw-on-no-match path).
    public static func emptyDefault() -> A11yNode {
        A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            enabled: true,
            visible: true,
            children: []
        )
    }
}

// App.tap mock-based unit tests.
//
// Verifies the wire pipeline (snapshot → FFI resolveSelector → IOHID
// synthesize) end-to-end against an in-memory MockSimRuntime, without
// booting a sim or linking XCTest into SmixSDK proper.

import Foundation
import XCTest
@testable import SmixSDK

final class AppTapMockTests: XCTestCase {

    // MARK: - .notFound path

    func testTapByIdEmptyTreeThrowsNotFound() async throws {
        let runtime = MockSimRuntime()  // default empty container
        let app = try await Smix.launchApp(.bundleId("com.example.app"), runtime: runtime)

        do {
            try await app.tap(SmixSDK.Selector.id("btn-missing"))
            XCTFail("empty tree must yield .notFound")
        } catch let failure as ExpectationFailure {
            XCTAssertEqual(failure.code, .notFound)
            XCTAssertEqual(failure.selector, SmixSDK.Selector.id("btn-missing"))
            XCTAssertFalse(failure.suggestions.isEmpty, "suggestions must populate for AI agent")
            // The empty container itself counts as visible — verify
            // visibleElements is non-empty (root node at minimum).
            XCTAssertFalse(failure.visibleElements.isEmpty)
        }
    }

    func testTapByTextAbsentThrowsNotFound() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("com.example.app"), runtime: runtime)

        do {
            try await app.tap(SmixSDK.Selector.text("Nope"))
            XCTFail("text miss must yield .notFound")
        } catch let failure as ExpectationFailure {
            XCTAssertEqual(failure.code, .notFound)
        }
    }

    // MARK: - Hit path

    func testTapByIdHitSynthesizesAtBoundsCenter() async throws {
        let button = A11yNode(
            rawType: "button",
            role: .button,
            identifier: "btn-login",
            label: "Sign In",
            bounds: Rect(x: 100, y: 200, w: 80, h: 40),
            enabled: true,
            visible: true
        )
        let root = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            enabled: true,
            visible: true,
            children: [button]
        )
        let runtime = MockSimRuntime(snapshotResult: root)
        let app = try await Smix.launchApp(.bundleId("com.example.app"), runtime: runtime)

        try await app.tap(SmixSDK.Selector.id("btn-login"))

        XCTAssertEqual(runtime.tapCalls.count, 1, "exactly one tap dispatched")
        let tap = runtime.tapCalls[0]
        XCTAssertEqual(tap.x, 140.0, accuracy: 0.01, "center x = 100 + 80/2")
        XCTAssertEqual(tap.y, 220.0, accuracy: 0.01, "center y = 200 + 40/2")
    }

    func testTapByLabelHitSynthesizes() async throws {
        let button = A11yNode(
            rawType: "button",
            role: .button,
            identifier: "btn-foo",
            label: "Submit",
            bounds: Rect(x: 50, y: 600, w: 100, h: 50),
            enabled: true,
            visible: true
        )
        let root = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            enabled: true,
            visible: true,
            children: [button]
        )
        let runtime = MockSimRuntime(snapshotResult: root)
        let app = try await Smix.launchApp(.bundleId("com.example.app"), runtime: runtime)

        // label selector resolves by accessibilityLabel strict equal
        try await app.tap(SmixSDK.Selector.label("Submit"))
        XCTAssertEqual(runtime.tapCalls.count, 1)
        XCTAssertEqual(runtime.tapCalls[0].x, 100.0, accuracy: 0.01)
    }

    // MARK: - Lifecycle

    func testLaunchAppCallsRuntimeLaunch() async throws {
        let runtime = MockSimRuntime()
        _ = try await Smix.launchApp(.bundleId("com.example.app"), runtime: runtime)
        XCTAssertEqual(runtime.launchCalls, ["com.example.app"])
    }

    func testRelaunchCallsTerminateAndLaunch() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("com.example.app"), runtime: runtime)
        try await app.relaunch()
        XCTAssertEqual(runtime.launchCalls, ["com.example.app", "com.example.app"])
        XCTAssertEqual(runtime.terminateCalls, ["com.example.app"])
    }

    func testTerminateCallsRuntimeTerminate() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("com.example.app"), runtime: runtime)
        try await app.terminate()
        XCTAssertEqual(runtime.terminateCalls, ["com.example.app"])
    }

    // .appPath is wired via runtime.launchFromPath. See
    // AppTapAtCoordAndAppPathMockTests.testLaunchWithAppPathDispatchesToRuntime
    // for the coverage.

    // MARK: - tree() sense

    func testTreeReturnsSnapshotFromRuntime() async throws {
        let snapshot = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 1, h: 1),
            children: []
        )
        let runtime = MockSimRuntime(snapshotResult: snapshot)
        let app = try await Smix.launchApp(.bundleId("com.example.app"), runtime: runtime)
        let tree = try await app.tree()
        XCTAssertEqual(tree, snapshot)
    }
}

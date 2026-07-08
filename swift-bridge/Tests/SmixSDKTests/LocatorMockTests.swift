// v7.1 c3 — Locator poll loop mock-based tests.
//
// Verifies host-side poll behavior (250ms tick + timeout deadline) +
// ExpectationFailure(.timeout / .wrongState) populated with last
// seen tree's visibleElements.

import Foundation
import XCTest
@testable import SmixSDK

final class LocatorMockTests: XCTestCase {

    // MARK: - immediate hit

    func testToBeVisibleHitOnFirstPoll() async throws {
        let target = A11yNode(
            rawType: "button",
            role: .button,
            identifier: "btn-go",
            bounds: Rect(x: 10, y: 10, w: 100, h: 40),
            enabled: true,
            visible: true  // visible flag is the key — predicate checks node.visible
        )
        let root = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            enabled: true,
            visible: true,
            children: [target]
        )
        let runtime = MockSimRuntime(snapshotResult: root)
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let loc = await app.find(SmixSDK.Selector.id("btn-go"))
        try await loc.toBeVisible(timeout: .seconds(1))
        // No throw = pass.
    }

    func testToBeVisibleTimesOutWhenNotFound() async throws {
        // Empty container (root only, no children).
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let loc = await app.find(SmixSDK.Selector.id("btn-missing"))
        do {
            try await loc.toBeVisible(timeout: .milliseconds(300))
            XCTFail("missing selector must yield .timeout")
        } catch let failure as ExpectationFailure {
            XCTAssertEqual(failure.code, .timeout)
            XCTAssertFalse(failure.visibleElements.isEmpty, "last tree must populate visibleElements")
            XCTAssertEqual(failure.selector, SmixSDK.Selector.id("btn-missing"))
        }
    }

    // MARK: - toContainText

    func testToContainTextHitViaLabel() async throws {
        let target = A11yNode(
            rawType: "staticText",
            role: .staticText,
            identifier: "welcome-msg",
            label: "Welcome back, alice!",
            bounds: Rect(x: 0, y: 100, w: 393, h: 24),
            enabled: true,
            visible: true
        )
        let root = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            visible: true,
            children: [target]
        )
        let runtime = MockSimRuntime(snapshotResult: root)
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let loc = await app.find(SmixSDK.Selector.id("welcome-msg"))
        try await loc.toContainText("alice", timeout: .seconds(1))
        // No throw = pass.
    }

    func testToContainTextTimesOutOnMissingSubstring() async throws {
        let target = A11yNode(
            rawType: "staticText",
            identifier: "welcome-msg",
            label: "Welcome back, bob!",
            bounds: Rect(x: 0, y: 100, w: 393, h: 24),
            visible: true
        )
        let root = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            visible: true,
            children: [target]
        )
        let runtime = MockSimRuntime(snapshotResult: root)
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let loc = await app.find(SmixSDK.Selector.id("welcome-msg"))
        do {
            try await loc.toContainText("alice", timeout: .milliseconds(300))
            XCTFail("missing substring must yield .timeout")
        } catch let failure as ExpectationFailure {
            XCTAssertEqual(failure.code, .timeout)
        }
    }

    // MARK: - toHaveLabel

    func testToHaveLabelHit() async throws {
        let target = A11yNode(
            rawType: "button",
            identifier: "btn-foo",
            label: "Submit",
            bounds: Rect(x: 0, y: 0, w: 1, h: 1),
            visible: true
        )
        let root = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            visible: true,
            children: [target]
        )
        let runtime = MockSimRuntime(snapshotResult: root)
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let loc = await app.find(SmixSDK.Selector.id("btn-foo"))
        try await loc.toHaveLabel("Submit", timeout: .seconds(1))
        // No throw = pass.
    }

    func testToHaveLabelTimeoutOnMismatch() async throws {
        let target = A11yNode(
            rawType: "button",
            identifier: "btn-foo",
            label: "Cancel",
            bounds: Rect(x: 0, y: 0, w: 1, h: 1),
            visible: true
        )
        let root = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            visible: true,
            children: [target]
        )
        let runtime = MockSimRuntime(snapshotResult: root)
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let loc = await app.find(SmixSDK.Selector.id("btn-foo"))
        do {
            try await loc.toHaveLabel("Submit", timeout: .milliseconds(300))
            XCTFail("label mismatch must yield .timeout")
        } catch let failure as ExpectationFailure {
            XCTAssertEqual(failure.code, .timeout)
        }
    }

    // MARK: - toHaveCount

    func testToHaveCountMatchesExactly() async throws {
        let buttons = (1...3).map { i in
            A11yNode(
                rawType: "button",
                identifier: "btn-\(i)",
                label: "Item",
                bounds: Rect(x: 0, y: Double(i * 50), w: 100, h: 40),
                visible: true
            )
        }
        let root = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            visible: true,
            children: buttons
        )
        let runtime = MockSimRuntime(snapshotResult: root)
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let loc = await app.find(SmixSDK.Selector.label("Item"))
        try await loc.toHaveCount(3, timeout: .seconds(1))
        // No throw = pass.
    }

    func testToHaveCountWrongStateOnMismatch() async throws {
        let target = A11yNode(
            rawType: "button",
            identifier: "btn-1",
            label: "Item",
            bounds: Rect(x: 0, y: 0, w: 1, h: 1),
            visible: true
        )
        let root = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            visible: true,
            children: [target]
        )
        let runtime = MockSimRuntime(snapshotResult: root)
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let loc = await app.find(SmixSDK.Selector.label("Item"))
        do {
            try await loc.toHaveCount(5, timeout: .milliseconds(300))
            XCTFail("expected 5 but actual 1 must yield .wrongState")
        } catch let failure as ExpectationFailure {
            XCTAssertEqual(failure.code, .wrongState)
            XCTAssertTrue(failure.message.contains("expected 5"))
            XCTAssertTrue(failure.message.contains("last poll saw 1"))
        }
    }

    // MARK: - polling behavior

    func testPollKeepsTryingUntilHit() async throws {
        // First snapshot: empty. Second snapshot: contains target.
        let target = A11yNode(
            rawType: "button",
            identifier: "btn-delayed",
            label: "Now Showing",
            bounds: Rect(x: 0, y: 0, w: 100, h: 40),
            visible: true
        )
        let withTarget = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            visible: true,
            children: [target]
        )

        let runtime = MockSimRuntime()  // starts empty
        runtime.afterSnapshot = { idx in
            if idx == 0 {
                runtime.snapshotResult = withTarget
            }
        }

        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)
        let loc = await app.find(SmixSDK.Selector.id("btn-delayed"))
        try await loc.toBeVisible(timeout: .seconds(2))
        // No throw = polling succeeded by 2nd iteration.
    }
}

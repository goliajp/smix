// v7.2 c1 — App.swipe / App.screenshot mock-based unit tests.
//
// Verifies the runtime wire-through is direct (no transformation,
// no buffering) for both methods.

import Foundation
import XCTest
@testable import SmixSDK

final class AppSwipeScreenshotMockTests: XCTestCase {

    // MARK: - swipe

    func testSwipeDispatchesToRuntime() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        try await app.swipe(.down)
        XCTAssertEqual(runtime.swipeCalls, [.down])
    }

    func testSwipeAllDirectionsLogged() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        try await app.swipe(.up)
        try await app.swipe(.down)
        try await app.swipe(.left)
        try await app.swipe(.right)

        XCTAssertEqual(runtime.swipeCalls, [.up, .down, .left, .right])
    }

    // MARK: - screenshot

    func testScreenshotReturnsRuntimeData() async throws {
        let runtime = MockSimRuntime()
        let expected = Data([0xFF, 0xD8, 0xFF, 0xE0])  // JPEG SOI marker for shape
        runtime.screenshotResult = expected

        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)
        let actual = try await app.screenshot()

        XCTAssertEqual(actual, expected)
        XCTAssertEqual(runtime.screenshotCalls, 1)
    }

    func testScreenshotEmptyByDefault() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let data = try await app.screenshot()
        XCTAssertEqual(data, Data())
        XCTAssertEqual(runtime.screenshotCalls, 1)
    }

    func testScreenshotCallCountIncrements() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        for _ in 0..<5 {
            _ = try await app.screenshot()
        }
        XCTAssertEqual(runtime.screenshotCalls, 5)
    }
}

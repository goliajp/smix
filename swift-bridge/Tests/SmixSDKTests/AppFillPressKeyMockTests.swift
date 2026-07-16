// App.fill / App.pressKey mock-based unit tests.
//
// Verifies wire-through to SmixSimRuntime sendString / pressKey
// primitives after focus tap.

import Foundation
import XCTest
@testable import SmixSDK

final class AppFillPressKeyMockTests: XCTestCase {

    // MARK: - fill

    func testFillTapsFocusThenSendsString() async throws {
        let input = A11yNode(
            rawType: "textField",
            role: .textField,
            identifier: "input-username",
            bounds: Rect(x: 50, y: 300, w: 200, h: 40),
            enabled: true,
            visible: true
        )
        let root = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            visible: true,
            children: [input]
        )
        let runtime = MockSimRuntime(snapshotResult: root)
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        try await app.fill(SmixSDK.Selector.id("input-username"), "alice")

        XCTAssertEqual(runtime.tapCalls.count, 1, "fill must tap to focus first")
        XCTAssertEqual(runtime.tapCalls[0].x, 150.0, accuracy: 0.01)
        XCTAssertEqual(runtime.tapCalls[0].y, 320.0, accuracy: 0.01)
        XCTAssertEqual(runtime.sendStringCalls, ["alice"])
    }

    func testFillThrowsNotFoundWhenTargetMissing() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        do {
            try await app.fill(SmixSDK.Selector.id("input-missing"), "hello")
            XCTFail("missing target must yield .notFound")
        } catch let failure as ExpectationFailure {
            XCTAssertEqual(failure.code, .notFound)
            XCTAssertEqual(runtime.sendStringCalls, [], "no string sent when no target")
            XCTAssertEqual(runtime.tapCalls.count, 0)
        }
    }

    // MARK: - pressKey

    func testPressKeyDispatchesToRuntime() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        try await app.pressKey(.return)
        try await app.pressKey(.delete)

        XCTAssertEqual(runtime.pressKeyCalls, [.return, .delete])
    }
}

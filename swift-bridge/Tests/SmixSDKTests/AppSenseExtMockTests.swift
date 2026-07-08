// v7.2 c2 — App sense extension API mock tests (systemPopups /
// openUrl / launchFresh).

import Foundation
import XCTest
@testable import SmixSDK

final class AppSenseExtMockTests: XCTestCase {

    // MARK: - systemPopups

    func testSystemPopupsReturnsMockResult() async throws {
        let popup = A11yNode(
            rawType: "alert",
            role: .alert,
            identifier: nil,
            label: "Allow Camera?",
            bounds: Rect(x: 50, y: 300, w: 293, h: 200),
            visible: true
        )
        let runtime = MockSimRuntime()
        runtime.systemPopupsResult = [popup]

        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)
        let actual = try await app.systemPopups()

        XCTAssertEqual(actual.count, 1)
        XCTAssertEqual(actual[0].label, "Allow Camera?")
        XCTAssertEqual(actual[0].role, .alert)
    }

    func testSystemPopupsEmptyByDefault() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)
        let actual = try await app.systemPopups()
        XCTAssertTrue(actual.isEmpty)
    }

    // MARK: - openUrl

    func testOpenUrlLogged() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let url = URL(string: "smix-fixture://settings/notifications")!
        try await app.openUrl(url)

        XCTAssertEqual(runtime.openUrlCalls, [url])
    }

    func testOpenUrlMultipleCallsLogged() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        let a = URL(string: "https://example.com/a")!
        let b = URL(string: "https://example.com/b")!
        try await app.openUrl(a)
        try await app.openUrl(b)

        XCTAssertEqual(runtime.openUrlCalls, [a, b])
    }

    // MARK: - launchFresh

    func testLaunchFreshDefaultArgs() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.target"), runtime: runtime)

        try await app.launchFresh()

        XCTAssertEqual(runtime.launchFreshCalls.count, 1)
        let call = runtime.launchFreshCalls[0]
        XCTAssertEqual(call.bundleId, "dev.smix.target")
        XCTAssertEqual(call.clearState, false)
        XCTAssertEqual(call.clearKeychain, false)
        XCTAssertNil(call.appPath)
    }

    func testLaunchFreshClearStateAndKeychain() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.target"), runtime: runtime)

        try await app.launchFresh(clearState: true, clearKeychain: true)

        XCTAssertEqual(runtime.launchFreshCalls.count, 1)
        XCTAssertEqual(runtime.launchFreshCalls[0].clearState, true)
        XCTAssertEqual(runtime.launchFreshCalls[0].clearKeychain, true)
        XCTAssertNil(runtime.launchFreshCalls[0].appPath)
    }

    func testLaunchFreshWithAppPath() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.target"), runtime: runtime)

        try await app.launchFresh(clearState: false, clearKeychain: false, appPath: "/tmp/MyApp.app")

        XCTAssertEqual(runtime.launchFreshCalls[0].appPath, "/tmp/MyApp.app")
    }
}

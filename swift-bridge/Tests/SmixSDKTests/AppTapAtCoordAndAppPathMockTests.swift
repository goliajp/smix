// App.tapAtCoord escape hatch + Smix.launchApp(.appPath) mock-based
// unit tests.

import Foundation
import XCTest
@testable import SmixSDK

final class AppTapAtCoordAndAppPathMockTests: XCTestCase {

    // MARK: - tapAtCoord

    func testTapAtCoordValidRange() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        try await app.tapAtCoord(0.5, 0.5)

        XCTAssertEqual(runtime.tapAtNormalizedCalls.count, 1)
        XCTAssertEqual(runtime.tapAtNormalizedCalls[0].x, 0.5, accuracy: 0.001)
        XCTAssertEqual(runtime.tapAtNormalizedCalls[0].y, 0.5, accuracy: 0.001)
    }

    func testTapAtCoordEdgeValues() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        try await app.tapAtCoord(0.0, 0.0)
        try await app.tapAtCoord(1.0, 1.0)

        XCTAssertEqual(runtime.tapAtNormalizedCalls.count, 2)
        XCTAssertEqual(runtime.tapAtNormalizedCalls[0].x, 0.0)
        XCTAssertEqual(runtime.tapAtNormalizedCalls[1].y, 1.0)
    }

    func testTapAtCoordOutOfRangeThrowsWrongState() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        do {
            try await app.tapAtCoord(1.5, 0.5)
            XCTFail("nx > 1.0 must throw .wrongState")
        } catch let failure as ExpectationFailure {
            XCTAssertEqual(failure.code, .wrongState)
            XCTAssertTrue(failure.message.contains("out of [0,1]"))
        }
        XCTAssertEqual(runtime.tapAtNormalizedCalls.count, 0,
                       "no runtime call should occur when validation throws")
    }

    func testTapAtCoordNegativeThrowsWrongState() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.bundleId("dev.smix.fixture"), runtime: runtime)

        do {
            try await app.tapAtCoord(0.5, -0.1)
            XCTFail("ny < 0.0 must throw .wrongState")
        } catch let failure as ExpectationFailure {
            XCTAssertEqual(failure.code, .wrongState)
        }
    }

    // MARK: - Smix.launchApp(.appPath)

    func testLaunchWithAppPathDispatchesToRuntime() async throws {
        let runtime = MockSimRuntime()
        let app = try await Smix.launchApp(.appPath("/tmp/MyApp.app"), runtime: runtime)

        XCTAssertEqual(runtime.launchFromPathCalls, ["/tmp/MyApp.app"])
        XCTAssertEqual(runtime.launchCalls, [], "bundleId path not used for .appPath")
        await XCTAssertEqual_(app.bundleIdentifier(), "/tmp/MyApp.app")
    }

    func testLaunchWithBundleIdStillUsesLaunch() async throws {
        let runtime = MockSimRuntime()
        _ = try await Smix.launchApp(.bundleId("dev.smix.x"), runtime: runtime)

        XCTAssertEqual(runtime.launchCalls, ["dev.smix.x"])
        XCTAssertEqual(runtime.launchFromPathCalls, [])
    }

    // Workaround: Swift Concurrency XCTAssertEqual on actor-returning fn.
    private func XCTAssertEqual_<T: Equatable>(_ actual: T, _ expected: T) async {
        XCTAssertEqual(actual, expected)
    }
}

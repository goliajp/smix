// Swift perf baseline test. Measures Smix.launchApp + App.tap round-trip
// against MockSimRuntime. Output goes to stdout (capture via
// `swift test --filter PerfBaseline... 2>&1 | grep '^perf:'`).
//
// This test always passes; it's an information producer, not an
// assertion. Skipped in regression suite — gated by SMIX_PERF_BENCH=1.

import Foundation
import XCTest
@testable import SmixSDK

final class PerfBaselineTests: XCTestCase {

    /// Smix.launchApp + App.tap round-trip latency baseline.
    func testTapRoundTripLatency() async throws {
        guard ProcessInfo.processInfo.environment["SMIX_PERF_BENCH"] == "1" else {
            throw XCTSkip("set SMIX_PERF_BENCH=1 to run perf baseline")
        }

        let iterations = 1000
        let warmup = 100

        let button = A11yNode(
            rawType: "button",
            identifier: "btn-x",
            label: "Tap me",
            bounds: Rect(x: 100, y: 200, w: 80, h: 40),
            enabled: true,
            visible: true
        )
        let tree = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            enabled: true,
            visible: true,
            children: [button]
        )

        func setupApp() async throws -> App {
            let runtime = MockSimRuntime(snapshotResult: tree)
            return try await Smix.launchApp(.bundleId("dev.smix.bench"), runtime: runtime)
        }

        // Warmup
        for _ in 0..<warmup {
            let app = try await setupApp()
            try await app.tap(SmixSDK.Selector.id("btn-x"))
        }

        // Measure
        var samples: [Double] = []
        for _ in 0..<iterations {
            let app = try await setupApp()
            let start = Date()
            try await app.tap(SmixSDK.Selector.id("btn-x"))
            samples.append(Date().timeIntervalSince(start) * 1000.0)  // ms
        }
        samples.sort()

        let median = samples[samples.count / 2]
        let p99 = samples[Int(Double(samples.count) * 0.99)]
        let minV = samples.first ?? 0
        let maxV = samples.last ?? 0
        let avg = samples.reduce(0, +) / Double(samples.count)

        print("perf: # SmixSDK Swift perf baseline")
        print("perf: # Date: \(ISO8601DateFormatter().string(from: Date()))")
        print("perf: # Operation: Smix.launchApp + App.tap(.id)")
        print("perf: # Backend: MockSimRuntime (in-memory)")
        print("perf: # Iterations: \(iterations) (after \(warmup) warmup)")
        print("perf: ")
        print("perf: min:    \(String(format: "%.3f", minV)) ms")
        print("perf: avg:    \(String(format: "%.3f", avg)) ms")
        print("perf: median: \(String(format: "%.3f", median)) ms")
        print("perf: p99:    \(String(format: "%.3f", p99)) ms")
        print("perf: max:    \(String(format: "%.3f", maxV)) ms")
        print("perf: ")
        print("perf: # Regression gate:")
        print("perf: #   soft fail if median > \(String(format: "%.3f", median * 1.5)) ms (1.5x)")
        print("perf: #   hard fail if median > \(String(format: "%.3f", median * 3)) ms (3x)")
    }
}

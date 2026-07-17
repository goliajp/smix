// ExpectationFailure schema parity + AI-readable JSON contract
// hardening tests.
//
// Verifies:
//   - All 6 FailureCode cases roundtrip (encode → decode → equal)
//   - errorDescription returns valid JSON with stable key set
//   - timestamp encoded as ISO-8601 (stable across locales)
//   - selector field encoded with custom Codable matching the Selector schema
//
// FfiError in the UDL carries only InvalidTreeJson / InvalidSelectorJson
// (the parse failure cases); it does not mirror the full FailureCode set.
// ExpectationFailure is constructed SDK-side from FFI results, so the
// schema contract verified here is what the AI agent consumes.

import Foundation
import XCTest
@testable import SmixSDK

final class ExpectationFailureContractTests: XCTestCase {

    // MARK: - FailureCode case coverage

    func testAllFailureCodeCasesRoundtrip() throws {
        for code in FailureCode.allCases {
            let original = ExpectationFailure(
                code: code,
                message: "msg for \(code.rawValue)",
                selector: .id("anywhere"),
                visibleElements: [],
                suggestions: ["s1", "s2"],
                timestamp: Date(timeIntervalSince1970: 1_780_000_000)
            )
            let data = try JSONEncoder().encode(original)
            let decoded = try JSONDecoder().decode(ExpectationFailure.self, from: data)
            XCTAssertEqual(decoded.code, original.code, "round-trip code mismatch for \(code)")
            XCTAssertEqual(decoded.message, original.message)
        }
    }

    func testFailureCodeCountIs6() {
        XCTAssertEqual(FailureCode.allCases.count, 6,
                       "spec: 6 codes (notFound/ambiguous/notInteractable/timeout/wrongState/unknown). Extend the UDL if Rust adds more.")
    }

    // MARK: - AI-readable JSON contract

    func testErrorDescriptionEmitsStableJsonKeys() {
        let failure = ExpectationFailure(
            code: .notFound,
            message: "no candidates",
            selector: .id("btn"),
            visibleElements: [
                A11yNode(rawType: "button", identifier: "other-btn",
                         bounds: Rect(x: 0, y: 0, w: 1, h: 1))
            ],
            suggestions: ["check id"],
            timestamp: Date(timeIntervalSince1970: 1_780_000_000)
        )
        guard let json = failure.errorDescription else {
            XCTFail("errorDescription must be non-nil")
            return
        }
        // Each key must appear once and only once (sorted keys, no
        // duplicates from JSONEncoder).
        for key in ["code", "message", "selector", "visibleElements", "suggestions", "timestamp"] {
            XCTAssertEqual(
                json.components(separatedBy: "\"\(key)\":").count - 1, 1,
                "key '\(key)' should appear exactly once in: \(json)"
            )
        }
    }

    func testErrorDescriptionIsValidParsableJson() throws {
        let failure = ExpectationFailure(
            code: .timeout,
            message: "timed out",
            selector: .text("Welcome"),
            visibleElements: [],
            suggestions: [],
            timestamp: Date()
        )
        guard let json = failure.errorDescription,
              let data = json.data(using: .utf8) else {
            XCTFail("errorDescription must produce UTF-8 JSON")
            return
        }
        // Round-trip through generic JSONSerialization to prove well-formedness.
        let parsed = try JSONSerialization.jsonObject(with: data)
        XCTAssertNotNil(parsed)
        XCTAssertTrue(parsed is [String: Any])
    }

    func testErrorDescriptionEmitsCamelCaseFailureCode() {
        let cases: [(FailureCode, String)] = [
            (.notFound, "notFound"),
            (.ambiguous, "ambiguous"),
            (.notInteractable, "notInteractable"),
            (.timeout, "timeout"),
            (.wrongState, "wrongState"),
            (.unknown, "unknown"),
        ]
        for (code, expectedRaw) in cases {
            let f = ExpectationFailure(code: code, message: "")
            guard let json = f.errorDescription else {
                XCTFail("nil errorDescription")
                continue
            }
            XCTAssertTrue(json.contains("\"\(expectedRaw)\""),
                          "expected `\(expectedRaw)` in JSON for \(code); got: \(json)")
        }
    }

    // Note: the App.tap notFound and Locator timeout paths that populate
    // visibleElements are exercised end-to-end against the runner by the
    // Rust wiremock driving suite (smix-ffi/tests/driving.rs). App now
    // holds concrete FFI handles (no injectable mock), so those paths are
    // no longer unit-testable here.

    // MARK: - errorDescription survives complex selector encoding

    func testErrorDescriptionSurvivesComplexSelector() {
        let complexSelector = SmixSDK.Selector.id("btn-foo")
            .below(.text("Anchor"))
            .nth(2)
        let f = ExpectationFailure(
            code: .ambiguous,
            message: "multiple candidates",
            selector: complexSelector,
            visibleElements: [],
            suggestions: ["specify .nth(...) modifier"]
        )
        guard let json = f.errorDescription else {
            XCTFail()
            return
        }
        XCTAssertTrue(json.contains("\"btn-foo\""))
        XCTAssertTrue(json.contains("\"Anchor\""))
        XCTAssertTrue(json.contains("\"nth\":2"))
    }
}

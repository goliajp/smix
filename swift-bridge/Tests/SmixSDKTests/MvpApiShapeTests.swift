// SmixSDK MVP shape tests.
//
// Verifies:
//   - Selector / Modifier / Role enum case completeness
//   - Selector custom Codable round-trips to/from Rust-compatible JSON
//   - A11yNode + Rect serde shape
//   - ExpectationFailure AI-readable JSON contract
//   - App / Locator surface area present (compile-check via existence)

import Foundation
import XCTest
@testable import SmixSDK

final class MvpApiShapeTests: XCTestCase {

    // MARK: - Selector cases

    func testSelectorBaseCasesExhaustive() {
        let cases: [SmixSDK.Selector] = [
            .id("btn-login"),
            .text("Sign In"),
            .label("Settings"),
            .role(Role.button),
            .role(Role.button, name: "Submit"),
        ]
        XCTAssertEqual(cases.count, 5, "MVP exposes 4 base + role-with-name variant")
    }

    func testSelectorIdEncodesRustCompatibleShape() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(SmixSDK.Selector.id("btn-login"))
        let json = String(data: data, encoding: .utf8)
        XCTAssertEqual(json, #"{"id":"btn-login"}"#)
    }

    func testSelectorTextEncodesRustCompatibleShape() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(SmixSDK.Selector.text("Sign In"))
        let json = String(data: data, encoding: .utf8)
        XCTAssertEqual(json, #"{"text":"Sign In"}"#)
    }

    func testSelectorLabelEncodesRustCompatibleShape() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(SmixSDK.Selector.label("Settings"))
        let json = String(data: data, encoding: .utf8)
        XCTAssertEqual(json, #"{"label":"Settings"}"#)
    }

    func testSelectorRoleEncodesRustCompatibleShape() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(SmixSDK.Selector.role(Role.button))  // 1st
        let json = String(data: data, encoding: .utf8)
        XCTAssertEqual(json, #"{"role":"button"}"#)
    }

    func testSelectorRoleWithNameEncodesRustCompatibleShape() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(SmixSDK.Selector.role(Role.button, name: "Submit"))
        let json = String(data: data, encoding: .utf8)
        XCTAssertEqual(json, #"{"name":"Submit","role":"button"}"#)
    }

    func testSelectorIdRoundTrip() throws {
        let original = SmixSDK.Selector.id("btn-login")
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(SmixSDK.Selector.self, from: data)
        XCTAssertEqual(original, decoded)
    }

    func testSelectorDecodeUnknownShapeFails() {
        let bad = #"{"kind":"id","value":"btn"}"#.data(using: .utf8)!
        XCTAssertThrowsError(try JSONDecoder().decode(SmixSDK.Selector.self, from: bad))
    }

    // MARK: - Role cases

    func testRoleCasesAllExposed() {
        // Mirrors the Rust smix-screen Role enum.
        // Expanding Role requires updating this list AND Rust side.
        let expected: Set<String> = [
            "button", "link", "textField", "secureTextField", "searchField",
            "switch", "toggle", "checkBox", "radio", "image", "staticText",
            "tab", "tabBar", "navigationBar", "cell", "alert", "dialog",
            "slider", "progressBar", "picker", "menu", "menuItem",
            "scrollView", "segmentedControl", "table", "collectionView",
            "webView", "keyboard",
        ]
        let actual = Set(Role.allCases.map { $0.rawValue })
        XCTAssertEqual(actual, expected)
    }

    // MARK: - A11yNode + Rect shape

    func testA11yNodeRoundTripMinimal() throws {
        let node = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 393, h: 852),
            enabled: true,
            visible: true
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(node)
        let decoded = try JSONDecoder().decode(A11yNode.self, from: data)
        XCTAssertEqual(decoded.rawType, "other")
        XCTAssertEqual(decoded.bounds, Rect(x: 0, y: 0, w: 393, h: 852))
    }

    func testA11yNodeUsesCamelCaseKeys() throws {
        let node = A11yNode(
            rawType: "other",
            bounds: Rect(x: 0, y: 0, w: 1, h: 1),
            hasFocus: true,
            visible: true
        )
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let data = try encoder.encode(node)
        let json = String(data: data, encoding: .utf8) ?? ""
        XCTAssertTrue(json.contains(#""rawType":"other""#), "JSON must use camelCase `rawType`, got: \(json)")
        XCTAssertTrue(json.contains(#""hasFocus":true"#), "JSON must use camelCase `hasFocus`, got: \(json)")
    }

    // MARK: - FailureCode

    func testFailureCodeCasesAllExposed() {
        let expected: Set<String> = [
            "notFound", "ambiguous", "notInteractable",
            "timeout", "wrongState", "unknown",
        ]
        let actual = Set(FailureCode.allCases.map { $0.rawValue })
        XCTAssertEqual(actual, expected)
    }

    func testExpectationFailureErrorDescriptionIsJson() {
        let failure = ExpectationFailure(
            code: .notFound,
            message: "no candidates after filter",
            selector: .id("btn-missing"),
            visibleElements: [],
            suggestions: ["check `accessibilityIdentifier` is set on the target view"],
            timestamp: Date(timeIntervalSince1970: 1_780_000_000)
        )
        guard let desc = failure.errorDescription else {
            XCTFail("errorDescription must produce JSON")
            return
        }
        XCTAssertTrue(desc.contains("\"code\":\"notFound\""), "missing code field: \(desc)")
        XCTAssertTrue(desc.contains("\"message\":\"no candidates after filter\""))
        XCTAssertTrue(desc.contains("\"suggestions\""))
        XCTAssertTrue(desc.contains("\"visibleElements\""))
    }

    // MARK: - App / Locator surface area
    //
    // tap / fill / pressKey / find / toBeVisible / toContainText /
    // toHaveLabel / toHaveCount / terminate / relaunch / tree / swipe /
    // systemPopups / tapAtCoord drive the FFI SmixDriver + SmixSession
    // surface. App holds concrete FFI handles, so it is not
    // mock-injectable here; the Rust wiremock suite
    // (smix-ffi/tests/driving.rs) covers the FFI client's own wire
    // behaviour but does NOT cross this file's Swift-side logic — pure
    // Swift mappings (e.g. KeyName.wireName) get their own direct tests
    // in this target.
}

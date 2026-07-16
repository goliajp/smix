// Selector full 7-case + Modifiers flatten + Pattern + fluent chaining
// roundtrip + Rust-compatible wire shape tests.

import Foundation
import XCTest
@testable import SmixSDK

final class SelectorFullSchemaTests: XCTestCase {

    // MARK: - Pattern roundtrip

    func testPatternLiteralEncodesAsBareString() throws {
        let p = SmixSDK.Pattern.literal("hello")
        let data = try JSONEncoder().encode(p)
        XCTAssertEqual(String(data: data, encoding: .utf8), "\"hello\"")
    }

    func testPatternRegexEncodesAsObject() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let p = SmixSDK.Pattern.regex("foo.*", flags: "i")
        let data = try encoder.encode(p)
        XCTAssertEqual(String(data: data, encoding: .utf8), #"{"flags":"i","regex":"foo.*"}"#)
    }

    func testPatternRoundtripLiteral() throws {
        let original = SmixSDK.Pattern.literal("hello")
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(SmixSDK.Pattern.self, from: data)
        XCTAssertEqual(decoded, original)
    }

    func testPatternRoundtripRegex() throws {
        let original = SmixSDK.Pattern.regex("^foo$", flags: "i")
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(SmixSDK.Pattern.self, from: data)
        XCTAssertEqual(decoded, original)
    }

    func testPatternStringLiteralExpressible() {
        let p: SmixSDK.Pattern = "hello"  // ExpressibleByStringLiteral
        XCTAssertEqual(p, SmixSDK.Pattern.literal("hello"))
    }

    // MARK: - Selector base case encode shape

    func testSelectorIdWithModifiersFlattens() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let sel = SmixSDK.Selector.id("btn-login").below(.text("Sign In")).nth(0)
        let data = try encoder.encode(sel)
        let json = String(data: data, encoding: .utf8) ?? ""
        // Expected: {"below":{"text":"Sign In"},"id":"btn-login","nth":0}
        XCTAssertTrue(json.contains(#""id":"btn-login""#), "got: \(json)")
        XCTAssertTrue(json.contains(#""below":{"text":"Sign In"}"#), "got: \(json)")
        XCTAssertTrue(json.contains(#""nth":0"#), "got: \(json)")
    }

    func testSelectorRoleWithNamePattern() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let sel = SmixSDK.Selector.role(.button, name: .literal("Submit"))
        let data = try encoder.encode(sel)
        XCTAssertEqual(
            String(data: data, encoding: .utf8),
            #"{"name":"Submit","role":"button"}"#
        )
    }

    func testSelectorFocusedEncodesAsBoolean() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let sel = SmixSDK.Selector.focused
        let data = try encoder.encode(sel)
        XCTAssertEqual(String(data: data, encoding: .utf8), #"{"focused":true}"#)
    }

    func testSelectorAnchorWithIndex() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let sel = SmixSDK.Selector.anchor(
            AnchorBox(below: .text("Address")),
            index: IndexModifiers(nth: 2)
        )
        let data = try encoder.encode(sel)
        let json = String(data: data, encoding: .utf8) ?? ""
        XCTAssertTrue(json.contains(#""anchor":{"below":{"text":"Address"}}"#), "got: \(json)")
        XCTAssertTrue(json.contains(#""nth":2"#), "got: \(json)")
    }

    func testSelectorLocalizedText() throws {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let sel = SmixSDK.Selector.localizedText(["en": "Submit", "ja": "送信"])
        let data = try encoder.encode(sel)
        let json = String(data: data, encoding: .utf8) ?? ""
        XCTAssertTrue(json.contains(#""localizedText""#), "got: \(json)")
        XCTAssertTrue(json.contains("Submit"))
        XCTAssertTrue(json.contains("送信"))
    }

    // MARK: - Roundtrip (decode wire JSON → Selector → re-encode == same)

    func testRoundtripIdWithBelowAndNth() throws {
        let wire = #"{"below":{"text":"Sign In"},"id":"btn-login","nth":0}"#
        let sel = try JSONDecoder().decode(SmixSDK.Selector.self, from: wire.data(using: .utf8)!)
        switch sel {
        case let .id(value, modifiers):
            XCTAssertEqual(value, "btn-login")
            XCTAssertEqual(modifiers.nth, 0)
            XCTAssertEqual(modifiers.below, SmixSDK.Selector.text(.literal("Sign In")))
        default:
            XCTFail("expected .id, got \(sel)")
        }
    }

    func testRoundtripFocused() throws {
        let wire = #"{"focused":true}"#
        let sel = try JSONDecoder().decode(SmixSDK.Selector.self, from: wire.data(using: .utf8)!)
        XCTAssertEqual(sel, .focused)
    }

    func testRoundtripAnchor() throws {
        let wire = #"{"anchor":{"below":{"text":"Address"}},"nth":2}"#
        let sel = try JSONDecoder().decode(SmixSDK.Selector.self, from: wire.data(using: .utf8)!)
        switch sel {
        case let .anchor(box, idx):
            XCTAssertEqual(box.below, SmixSDK.Selector.text(.literal("Address")))
            XCTAssertEqual(idx.nth, 2)
        default:
            XCTFail("expected .anchor, got \(sel)")
        }
    }

    func testRoundtripLocalizedText() throws {
        let wire = #"{"localizedText":{"en":"Submit","ja":"送信"}}"#
        let sel = try JSONDecoder().decode(SmixSDK.Selector.self, from: wire.data(using: .utf8)!)
        switch sel {
        case let .localizedText(map, _):
            XCTAssertEqual(map["en"], "Submit")
            XCTAssertEqual(map["ja"], "送信")
        default:
            XCTFail("expected .localizedText, got \(sel)")
        }
    }

    func testRoundtripRolePatternRegex() throws {
        let wire = #"{"name":{"flags":"i","regex":"^Sub"},"role":"button"}"#
        let sel = try JSONDecoder().decode(SmixSDK.Selector.self, from: wire.data(using: .utf8)!)
        switch sel {
        case let .role(role, name, _):
            XCTAssertEqual(role, .button)
            XCTAssertEqual(name, .regex("^Sub", flags: "i"))
        default:
            XCTFail("expected .role, got \(sel)")
        }
    }

    // MARK: - Fluent chaining

    func testFluentBelowSetsModifier() {
        let s = SmixSDK.Selector.id("btn").below(.text("anchor"))
        if case let .id(value, mods) = s {
            XCTAssertEqual(value, "btn")
            XCTAssertEqual(mods.below, SmixSDK.Selector.text(.literal("anchor")))
        } else {
            XCTFail("expected .id, got \(s)")
        }
    }

    func testFluentChainedMultiple() {
        let s = SmixSDK.Selector.id("btn")
            .below(.text("address"))
            .above(.text("title"))
            .nth(3)
        if case let .id(_, mods) = s {
            XCTAssertNotNil(mods.below)
            XCTAssertNotNil(mods.above)
            XCTAssertEqual(mods.nth, 3)
        } else {
            XCTFail("expected .id")
        }
    }

    func testFluentFirstLast() {
        let f = SmixSDK.Selector.label("Item").first()
        let l = SmixSDK.Selector.label("Item").last()
        if case let .label(_, mods) = f {
            XCTAssertEqual(mods.first, true)
        }
        if case let .label(_, mods) = l {
            XCTAssertEqual(mods.last, true)
        }
    }

    func testFluentNearAncestor() {
        let s = SmixSDK.Selector.text("Cancel")
            .near(.text("Confirm"))
            .ancestor(.role(.dialog))
        if case let .text(_, mods) = s {
            XCTAssertNotNil(mods.near)
            XCTAssertNotNil(mods.ancestor)
        } else {
            XCTFail("expected .text")
        }
    }

    // MARK: - Equatable / round-trip with modifiers

    func testRoundtripIdWithMultipleModifiers() throws {
        let original = SmixSDK.Selector.id("btn-foo")
            .below(.text("Address"))
            .nth(0)
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(SmixSDK.Selector.self, from: data)
        XCTAssertEqual(decoded, original)
    }

    func testRoundtripRoleWithModifiersAndName() throws {
        let original = SmixSDK.Selector.role(.button, name: .literal("Submit"))
            .below(.text("Email"))
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(SmixSDK.Selector.self, from: data)
        XCTAssertEqual(decoded, original)
    }
}

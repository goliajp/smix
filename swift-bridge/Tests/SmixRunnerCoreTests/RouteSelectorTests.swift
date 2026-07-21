// What the tap-family routes accept as a selector.
//
// They took one key: `text`. So `dispatch: daemonProxy` — the escape
// hatch for React Native views whose Pressable swallows the normal
// gesture path — could only ever address an element by its visible
// label, which is exactly what an RN testID is not. The actions guide
// documents that combination with an id and it has never worked.
//
// The runner-side predicate already matched on identifier as well as
// label, so a text selector could accidentally hit a testID. That is a
// confused hit, not support: the caller asked for a label and got a
// match on something else. Naming the form on the wire is what lets the
// predicate stop guessing.
//
// Deliberately three forms and no more. text, id and label are what the
// existing NSPredicate can express directly. Regex, roles and spatial
// modifiers would mean a second copy of the resolver living inside
// XCUITest — one contract, two implementations, which is the drift this
// segment exists to remove. Those keep resolving host-side.

import XCTest

@testable import SmixRunnerCore

final class RouteSelectorTests: XCTestCase {

    func testDecodesTextForm() throws {
        let sel = try RouteSelector.decode(from: ["text": "Sign In"])
        XCTAssertEqual(sel, .text("Sign In"))
        XCTAssertEqual(sel.wireKey, "text")
        XCTAssertEqual(sel.raw, "Sign In")
    }

    func testDecodesIdForm() throws {
        let sel = try RouteSelector.decode(from: ["id": "btn-login"])
        XCTAssertEqual(sel, .id("btn-login"))
        XCTAssertEqual(sel.wireKey, "id")
    }

    func testDecodesLabelForm() throws {
        let sel = try RouteSelector.decode(from: ["label": "Sign In"])
        XCTAssertEqual(sel, .label("Sign In"))
        XCTAssertEqual(sel.wireKey, "label")
    }

    /// A regex arrives as an object, not a string. Refused by shape
    /// rather than silently stringified into a literal that would match
    /// nothing.
    func testRejectsRegexObjectForm() throws {
        XCTAssertThrowsError(try RouteSelector.decode(from: ["text": ["regex": "^Sign"]])) { error in
            guard case RouteSelector.Failure.wrongType = error else {
                return XCTFail("expected wrongType, got \(error)")
            }
        }
    }

    /// Roles need the rawType→Role mapping the host side owns. Refusing
    /// here is what keeps that mapping in one place.
    func testRejectsRoleForm() throws {
        XCTAssertThrowsError(try RouteSelector.decode(from: ["role": "button"])) { error in
            XCTAssertEqual(error as? RouteSelector.Failure, .unsupportedSelectorForm)
        }
    }

    func testRejectsEmptySelectorObject() throws {
        XCTAssertThrowsError(try RouteSelector.decode(from: [:])) { error in
            XCTAssertEqual(error as? RouteSelector.Failure, .unsupportedSelectorForm)
        }
    }
}

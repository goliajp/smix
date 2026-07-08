// Spike001Tests — v7.0 c3 conformance fixture #1 (empty tree + id miss → []).
//
// Pairs with `crates/smix-core-conformance/fixtures/spike-001-empty-tree.json`
// + `crates/smix-ffi/src/lib.rs#tests#spike_001_empty_tree_id_miss`.
//
// Goal: prove Swift FFI binding (UniFFI-generated) calls into the same
// Rust core (`smix_ffi::resolve_selector`) and produces byte-identical
// output to the Rust backend. Conformance T1 first cell.

import XCTest
@testable import SmixCoreFFIBindings

final class Spike001Tests: XCTestCase {
    /// Empty a11y tree + id selector miss → empty `[String]`.
    /// Byte-identical to `cargo run --bin fixture-runner -- rust spike-001`.
    func testEmptyTreeIdMissByteIdenticalToRust() throws {
        let treeJson = #"{"rawType":"other","bounds":{"x":0,"y":0,"w":393,"h":852},"enabled":true,"selected":false,"hasFocus":false,"visible":true,"children":[]}"#
        let selectorJson = #"{"id":"nope"}"#

        let result = try resolveSelector(treeJson: treeJson, selectorJson: selectorJson)

        XCTAssertEqual(result, [], "Swift FFI must return [] (byte-identical to Rust backend) on empty tree + id miss")
    }

    /// Invalid tree JSON → throws `FfiError.invalidTreeJson`. Matches
    /// Rust unit test `invalid_tree_json_returns_err`.
    func testInvalidTreeJsonThrows() {
        XCTAssertThrowsError(try resolveSelector(treeJson: "not json", selectorJson: "{}")) { error in
            guard case FfiError.InvalidTreeJson = error else {
                XCTFail("expected FfiError.InvalidTreeJson, got \(error)")
                return
            }
        }
    }

    /// Invalid selector JSON → throws `FfiError.invalidSelectorJson`.
    func testInvalidSelectorJsonThrows() {
        let treeJson = #"{"rawType":"other","bounds":{"x":0,"y":0,"w":393,"h":852},"enabled":true,"selected":false,"hasFocus":false,"visible":true,"children":[]}"#
        XCTAssertThrowsError(try resolveSelector(treeJson: treeJson, selectorJson: "not json")) { error in
            guard case FfiError.InvalidSelectorJson = error else {
                XCTFail("expected FfiError.InvalidSelectorJson, got \(error)")
                return
            }
        }
    }
}

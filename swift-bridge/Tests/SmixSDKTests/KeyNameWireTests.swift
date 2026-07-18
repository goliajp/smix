import XCTest
@testable import SmixSDK

/// `KeyName.wireName` is the only place the Swift SDK translates a key
/// to the string the runner accepts. `enter` regressed once — the FFI
/// carries no `enter` synonym, and sending the rawValue meant the
/// runner rejected it — with no test on this side to notice: a comment
/// claimed the Rust wiremock suite covered it end-to-end, but that
/// suite passes the literal `"return"` and never crosses this mapping.
final class KeyNameWireTests: XCTestCase {
  func test_enter_maps_to_return() {
    XCTAssertEqual(KeyName.enter.wireName, "return")
  }

  func test_every_other_key_is_its_own_rawValue() {
    for key in KeyName.allCases where key != .enter {
      XCTAssertEqual(key.wireName, key.rawValue, "\(key) drifted from its wire string")
    }
  }

  func test_no_wire_name_is_the_unknown_enter_spelling() {
    // The runner has no "enter" key; nothing may ever send it.
    for key in KeyName.allCases {
      XCTAssertNotEqual(key.wireName, "enter", "\(key) sends a key the runner rejects")
    }
  }
}

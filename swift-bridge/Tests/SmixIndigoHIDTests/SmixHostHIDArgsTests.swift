import XCTest
@testable import SmixIndigoHID

final class SmixHostHIDArgsTests: XCTestCase {
  func test_parse_tap_validFlags_returnsTapCase() throws {
    let parsed = try SmixHostHIDArgs.parse(["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.18"])
    // C4: no `--path` flag → default `.digitizer`.
    XCTAssertEqual(parsed, SmixHostHIDArgs.tap(udid: "ABCD", x: 0.5, y: 0.18, path: .digitizer))
  }

  func test_parse_empty_throwsMissingSubcommand() {
    XCTAssertThrowsError(try SmixHostHIDArgs.parse([])) { err in
      XCTAssertEqual(err as? SmixHostHIDArgs.ParseError, .missingSubcommand)
    }
  }

  func test_parse_unknownSubcommand_throwsUnknown() {
    XCTAssertThrowsError(try SmixHostHIDArgs.parse(["wiggle"])) { err in
      XCTAssertEqual(err as? SmixHostHIDArgs.ParseError, .unknownSubcommand("wiggle"))
    }
  }

  func test_parse_missingUdidFlag_throwsMissingFlag() {
    XCTAssertThrowsError(try SmixHostHIDArgs.parse(["tap", "--x", "0.5", "--y", "0.18"])) { err in
      XCTAssertEqual(err as? SmixHostHIDArgs.ParseError, .missingFlag("--udid"))
    }
  }

  func test_parse_xValueNotANumber_throwsInvalidFlagValue() {
    XCTAssertThrowsError(
      try SmixHostHIDArgs.parse(["tap", "--udid", "ABCD", "--x", "not-a-number", "--y", "0.5"])
    ) { err in
      XCTAssertEqual(
        err as? SmixHostHIDArgs.ParseError,
        .invalidFlagValue(name: "--x", value: "not-a-number")
      )
    }
  }

  // MARK: - v0.3 C2 axp-probe subcommand parse

  func test_parse_axpProbe_noFlags_returnsAxpProbe() throws {
    let parsed = try SmixHostHIDArgs.parse(["axp-probe"])
    XCTAssertEqual(parsed, .axpProbe)
  }

  func test_parse_axpProbe_with_unknownFlag_throws() {
    XCTAssertThrowsError(try SmixHostHIDArgs.parse(["axp-probe", "--what", "x"])) { err in
      XCTAssertEqual(
        err as? SmixHostHIDArgs.ParseError,
        .invalidFlagValue(name: "--what", value: "x")
      )
    }
  }
}

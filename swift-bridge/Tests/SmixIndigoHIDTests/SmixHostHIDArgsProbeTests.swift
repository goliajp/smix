import XCTest
@testable import SmixIndigoHID

/// `probe` subcommand args parsing. Pinned alongside the existing tap parse
/// tests so that adding `probe` leaves the `tap` forms byte-identical.
final class SmixHostHIDArgsProbeTests: XCTestCase {

  func test_parse_probe_noFlags_returnsProbe() throws {
    let parsed = try SmixHostHIDArgs.parse(["probe"])
    XCTAssertEqual(parsed, .probe)
  }

  func test_parse_probe_with_unknownFlag_throws() {
    XCTAssertThrowsError(try SmixHostHIDArgs.parse(["probe", "--what", "x"])) { err in
      XCTAssertEqual(
        err as? SmixHostHIDArgs.ParseError,
        .invalidFlagValue(name: "--what", value: "x")
      )
    }
  }

  func test_parse_tap_still_works_unchanged() throws {
    let parsed = try SmixHostHIDArgs.parse(
      ["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.25"]
    )
    XCTAssertEqual(
      parsed,
      .tap(udid: "ABCD", x: 0.5, y: 0.25, path: .digitizer)
    )
  }

  func test_parse_unknown_subcommand_returns_unknownSubcommand() {
    XCTAssertThrowsError(try SmixHostHIDArgs.parse(["bogus"])) { err in
      XCTAssertEqual(
        err as? SmixHostHIDArgs.ParseError,
        .unknownSubcommand("bogus")
      )
    }
  }
}

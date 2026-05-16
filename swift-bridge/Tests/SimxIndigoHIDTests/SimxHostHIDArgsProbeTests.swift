import XCTest
@testable import SimxIndigoHID

/// C5 — `probe` subcommand args parsing. Pinned alongside the existing tap
/// parse tests so C3/C4 forms are byte-identical to the pre-C5 behaviour.
final class SimxHostHIDArgsProbeTests: XCTestCase {

  func test_parse_probe_noFlags_returnsProbe() throws {
    let parsed = try SimxHostHIDArgs.parse(["probe"])
    XCTAssertEqual(parsed, .probe)
  }

  func test_parse_probe_with_unknownFlag_throws() {
    XCTAssertThrowsError(try SimxHostHIDArgs.parse(["probe", "--what", "x"])) { err in
      XCTAssertEqual(
        err as? SimxHostHIDArgs.ParseError,
        .invalidFlagValue(name: "--what", value: "x")
      )
    }
  }

  func test_parse_tap_still_works_unchanged() throws {
    let parsed = try SimxHostHIDArgs.parse(
      ["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.25"]
    )
    XCTAssertEqual(
      parsed,
      .tap(udid: "ABCD", x: 0.5, y: 0.25, path: .digitizer)
    )
  }

  func test_parse_unknown_subcommand_returns_unknownSubcommand() {
    XCTAssertThrowsError(try SimxHostHIDArgs.parse(["bogus"])) { err in
      XCTAssertEqual(
        err as? SimxHostHIDArgs.ParseError,
        .unknownSubcommand("bogus")
      )
    }
  }
}

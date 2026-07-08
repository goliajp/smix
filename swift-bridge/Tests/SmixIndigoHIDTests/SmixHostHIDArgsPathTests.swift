import XCTest
@testable import SmixIndigoHID

/// C4: `--path <digitizer|indigo9>` flag on `smix-host-hid tap`. Default
/// (flag absent) → `.digitizer`. Unknown value → `.invalidFlagValue`.
final class SmixHostHIDArgsPathTests: XCTestCase {
  func test_parse_tap_noPathFlag_defaultsToDigitizer() throws {
    let parsed = try SmixHostHIDArgs.parse(
      ["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.25"]
    )
    XCTAssertEqual(
      parsed,
      SmixHostHIDArgs.tap(udid: "ABCD", x: 0.5, y: 0.25, path: .digitizer)
    )
  }

  func test_parse_tap_pathDigitizer_explicit() throws {
    let parsed = try SmixHostHIDArgs.parse(
      ["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.25", "--path", "digitizer"]
    )
    XCTAssertEqual(
      parsed,
      SmixHostHIDArgs.tap(udid: "ABCD", x: 0.5, y: 0.25, path: .digitizer)
    )
  }

  func test_parse_tap_pathIndigo9_explicit() throws {
    let parsed = try SmixHostHIDArgs.parse(
      ["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.25", "--path", "indigo9"]
    )
    XCTAssertEqual(
      parsed,
      SmixHostHIDArgs.tap(udid: "ABCD", x: 0.5, y: 0.25, path: .indigo9)
    )
  }

  func test_parse_tap_pathUnknown_throwsInvalidFlagValue() {
    XCTAssertThrowsError(
      try SmixHostHIDArgs.parse(
        ["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.25", "--path", "wat"]
      )
    ) { err in
      XCTAssertEqual(
        err as? SmixHostHIDArgs.ParseError,
        .invalidFlagValue(name: "--path", value: "wat")
      )
    }
  }
}

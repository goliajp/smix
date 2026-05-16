import XCTest
@testable import SimxIndigoHID

/// C4: `--path <digitizer|indigo9>` flag on `simx-host-hid tap`. Default
/// (flag absent) → `.digitizer`. Unknown value → `.invalidFlagValue`.
final class SimxHostHIDArgsPathTests: XCTestCase {
  func test_parse_tap_noPathFlag_defaultsToDigitizer() throws {
    let parsed = try SimxHostHIDArgs.parse(
      ["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.25"]
    )
    XCTAssertEqual(
      parsed,
      SimxHostHIDArgs.tap(udid: "ABCD", x: 0.5, y: 0.25, path: .digitizer)
    )
  }

  func test_parse_tap_pathDigitizer_explicit() throws {
    let parsed = try SimxHostHIDArgs.parse(
      ["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.25", "--path", "digitizer"]
    )
    XCTAssertEqual(
      parsed,
      SimxHostHIDArgs.tap(udid: "ABCD", x: 0.5, y: 0.25, path: .digitizer)
    )
  }

  func test_parse_tap_pathIndigo9_explicit() throws {
    let parsed = try SimxHostHIDArgs.parse(
      ["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.25", "--path", "indigo9"]
    )
    XCTAssertEqual(
      parsed,
      SimxHostHIDArgs.tap(udid: "ABCD", x: 0.5, y: 0.25, path: .indigo9)
    )
  }

  func test_parse_tap_pathUnknown_throwsInvalidFlagValue() {
    XCTAssertThrowsError(
      try SimxHostHIDArgs.parse(
        ["tap", "--udid", "ABCD", "--x", "0.5", "--y", "0.25", "--path", "wat"]
      )
    ) { err in
      XCTAssertEqual(
        err as? SimxHostHIDArgs.ParseError,
        .invalidFlagValue(name: "--path", value: "wat")
      )
    }
  }
}

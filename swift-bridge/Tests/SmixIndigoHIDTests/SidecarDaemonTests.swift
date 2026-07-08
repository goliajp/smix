import XCTest
@testable import SmixIndigoHID

final class SidecarDaemonTests: XCTestCase {
  func test_parseDaemonOp() throws {
    XCTAssertEqual(try SidecarDaemon.parseLine(#"{"op":"ping"}"#), .ping)
    XCTAssertEqual(
      try SidecarDaemon.parseLine(
        #"{"op":"tap","udid":"X","x":0.5,"y":0.25,"path":"digitizer"}"#
      ),
      .tap(udid: "X", x: 0.5, y: 0.25, path: .digitizer)
    )
    XCTAssertEqual(try SidecarDaemon.parseLine(#"{"op":"shutdown"}"#), .shutdown)
  }

  func test_formatPingResponse() {
    XCTAssertEqual(SidecarDaemon.formatPing(ok: true), #"{"ok":true,"op":"ping"}"#)

    let tapLine = SidecarDaemon.formatTap(ok: true, path: "digitizer", resolved: ["x", "y"])
    XCTAssertTrue(tapLine.contains(#""ok":true"#), tapLine)
    XCTAssertTrue(tapLine.contains(#""path":"digitizer""#), tapLine)
    XCTAssertTrue(tapLine.contains(#""resolved":["x","y"]"#), tapLine)
  }

  func test_parseLineRejectsUnknownOp() {
    XCTAssertThrowsError(try SidecarDaemon.parseLine(#"{"op":"reboot"}"#)) { err in
      XCTAssertEqual(err as? SidecarParseError, .unknownOp("reboot"))
    }
    XCTAssertThrowsError(try SidecarDaemon.parseLine("not json")) { err in
      XCTAssertEqual(err as? SidecarParseError, .invalidJson)
    }
  }
}

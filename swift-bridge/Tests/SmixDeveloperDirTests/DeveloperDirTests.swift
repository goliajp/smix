import XCTest
@testable import SmixDeveloperDir

/// What `xcode-select -p` printed, judged. The process call around it is
/// covered by the checkpoint's real run; this is the pure half.
final class DeveloperDirTests: XCTestCase {
  func testTrailingNewlineIsStripped() throws {
    let dev = try DeveloperDir.parse(stdout: Data("/A/Contents/Developer\n".utf8), status: 0)
    XCTAssertEqual(dev, "/A/Contents/Developer")
  }

  func testEmptyOutputIsRefusedByName() {
    XCTAssertThrowsError(try DeveloperDir.parse(stdout: Data(), status: 0)) { err in
      XCTAssertEqual(err as? DeveloperDir.Error, .empty)
      XCTAssertTrue("\(err)".contains("empty"), "\(err)")
    }
  }

  func testNonZeroExitIsRefusedWithTheCode() {
    XCTAssertThrowsError(try DeveloperDir.parse(stdout: Data("/A\n".utf8), status: 2)) { err in
      XCTAssertEqual(err as? DeveloperDir.Error, .exit(2))
      XCTAssertTrue("\(err)".contains("exit 2"), "\(err)")
    }
  }
}

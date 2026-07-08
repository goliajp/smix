import XCTest
@testable import SmixIndigoHID

final class IndigoConstantsTests: XCTestCase {
  /// These are private-ABI wire constants. Any "drift while tidying" silently
  /// breaks real Indigo dispatch. Pin them to byte-exact values to keep plan
  /// + code in sync.
  func test_constants_byteExact() {
    XCTAssertEqual(IndigoConstants.touchDigitizer, UInt32(0x32))
    XCTAssertEqual(IndigoConstants.nsEventDown, UInt32(1))
    XCTAssertEqual(IndigoConstants.dirDown, UInt32(1))
    // Defensive: also pin the "up" pair so the plan's chosen 3-tuple shape
    // is locked end-to-end (down → up) — but this assertion intentionally
    // stays under the same single test case to match the plan's 13-case
    // total (4+3+5+1 = 13).
    XCTAssertEqual(IndigoConstants.nsEventUp, UInt32(2))
    XCTAssertEqual(IndigoConstants.dirUp, UInt32(2))
  }
}

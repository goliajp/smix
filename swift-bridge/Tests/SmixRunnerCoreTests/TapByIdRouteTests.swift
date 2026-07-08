import XCTest
@testable import SmixRunnerCore

final class TapByIdRouteTests: XCTestCase {
  func test_decode_validId_returnsRequest() throws {
    let body = Data(#"{"id":"v2-modal-sheet-dismiss-btn"}"#.utf8)
    let req = try TapByIdRoute.decode(body)
    XCTAssertEqual(req, TapByIdRoute.TapByIdRequest(id: "v2-modal-sheet-dismiss-btn"))
  }

  func test_decode_emptyBody_throwsInvalidJSON() {
    XCTAssertThrowsError(try TapByIdRoute.decode(Data())) { err in
      XCTAssertEqual(err as? TapByIdRoute.DecodeError, .invalidJSON)
    }
  }

  func test_decode_missingId_throwsMissingId() {
    let body = Data("{}".utf8)
    XCTAssertThrowsError(try TapByIdRoute.decode(body)) { err in
      XCTAssertEqual(err as? TapByIdRoute.DecodeError, .missingId)
    }
  }

  func test_decode_emptyId_throwsInvalidField() {
    let body = Data(#"{"id":""}"#.utf8)
    XCTAssertThrowsError(try TapByIdRoute.decode(body)) { err in
      guard let e = err as? TapByIdRoute.DecodeError, case .invalidField(let f, _) = e else {
        return XCTFail("expected invalidField, got \(err)")
      }
      XCTAssertEqual(f, "id")
    }
  }

  func test_decode_idIsNumber_throwsInvalidField() {
    let body = Data(#"{"id":42}"#.utf8)
    XCTAssertThrowsError(try TapByIdRoute.decode(body)) { err in
      guard let e = err as? TapByIdRoute.DecodeError, case .invalidField = e else {
        return XCTFail("expected invalidField, got \(err)")
      }
    }
  }
}

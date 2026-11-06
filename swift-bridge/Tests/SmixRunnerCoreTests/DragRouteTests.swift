import XCTest
import FlyingFox
@testable import SmixRunnerCore

final class DragRouteTests: XCTestCase {
  private let full =
    #"{"fromNx":0.2,"fromNy":0.3,"toNx":0.8,"toNy":0.9,"holdMs":800,"durationMs":600,"releaseMs":300}"#

  private func req(_ json: String) throws -> DragRoute.DragRequest {
    try DragRoute.decode(Data(json.utf8))
  }

  func test_decode_readsEveryField() throws {
    XCTAssertEqual(
      try req(full),
      DragRoute.DragRequest(
        fromNx: 0.2, fromNy: 0.3, toNx: 0.8, toNy: 0.9, holdMs: 800, durationMs: 600, releaseMs: 300))
  }

  func test_decode_aMissingTiming_isNamed_notDefaulted() {
    let noHold = full.replacingOccurrences(of: #""holdMs":800,"#, with: "")
    XCTAssertThrowsError(try req(noHold)) {
      XCTAssertEqual($0 as? DragRoute.DecodeError, .missingField("holdMs"))
    }
  }

  func test_decode_aPointOffTheScreen_isRefused() {
    let off = full.replacingOccurrences(of: #""toNy":0.9"#, with: #""toNy":1.2"#)
    XCTAssertThrowsError(try req(off)) {
      XCTAssertEqual($0 as? DragRoute.DecodeError, .outOfRange("toNy", 1.2))
    }
  }

  func test_decode_timingsOutsideTheHostsRanges_areRefused() {
    let short = full.replacingOccurrences(of: #""durationMs":600"#, with: #""durationMs":49"#)
    XCTAssertThrowsError(try req(short)) {
      XCTAssertEqual($0 as? DragRoute.DecodeError, .outOfRange("durationMs", 49))
    }
    let long = full.replacingOccurrences(of: #""releaseMs":300"#, with: #""releaseMs":5001"#)
    XCTAssertThrowsError(try req(long)) {
      XCTAssertEqual($0 as? DragRoute.DecodeError, .outOfRange("releaseMs", 5001))
    }
  }

  func test_path_holdsStill_thenMoves_thenRests() throws {
    let p = DragRoute.path(try req(full), size: CGSize(width: 100, height: 200))
    XCTAssertEqual(
      p,
      [
        .init(CGPoint(x: 20, y: 60), 0),
        .init(CGPoint(x: 20, y: 60), 0.8),
        .init(CGPoint(x: 80, y: 180), 1.4),
        .init(CGPoint(x: 80, y: 180), 1.7),
      ])
  }

  func test_path_withoutHoldOrRest_isAStraightMove() throws {
    let r = DragRoute.DragRequest(
      fromNx: 0, fromNy: 0, toNx: 1, toNy: 1, holdMs: 0, durationMs: 500, releaseMs: 0)
    let p = DragRoute.path(r, size: CGSize(width: 10, height: 10))
    XCTAssertEqual(p, [.init(.zero, 0), .init(CGPoint(x: 10, y: 10), 0.5)])
  }
}

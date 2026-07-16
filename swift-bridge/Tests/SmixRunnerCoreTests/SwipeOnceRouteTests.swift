import XCTest
@testable import SmixRunnerCore

// SwipeOnceRoute decode tests. SwipeOnceRoute is otherwise only covered
// implicitly via integration, so these decode tests guard the parser shape
// directly (callers wire HTTP body → decode → handler dispatch).
final class SwipeOnceRouteTests: XCTestCase {

  func test_decode_up_direction() throws {
    let body = Data(#"{"direction":"up"}"#.utf8)
    let req = try SwipeOnceRoute.decode(body)
    XCTAssertEqual(req.direction, .up)
  }

  func test_decode_down_direction() throws {
    let body = Data(#"{"direction":"down"}"#.utf8)
    let req = try SwipeOnceRoute.decode(body)
    XCTAssertEqual(req.direction, .down)
  }

  // left / right are advertised by the cli docs, so the runner must accept
  // them rather than raise invalidDirection.
  func test_decode_left_direction() throws {
    let body = Data(#"{"direction":"left"}"#.utf8)
    let req = try SwipeOnceRoute.decode(body)
    XCTAssertEqual(req.direction, .left)
  }

  func test_decode_right_direction() throws {
    let body = Data(#"{"direction":"right"}"#.utf8)
    let req = try SwipeOnceRoute.decode(body)
    XCTAssertEqual(req.direction, .right)
  }

  // Diagonal / non-direction string still rejected.
  func test_decode_invalid_direction_throws() {
    let body = Data(#"{"direction":"diagonal"}"#.utf8)
    XCTAssertThrowsError(try SwipeOnceRoute.decode(body)) { error in
      guard case SwipeOnceRoute.DecodeError.invalidDirection(let s) =
        error as? SwipeOnceRoute.DecodeError ?? .invalidJSON else {
        XCTFail("expected invalidDirection, got \(error)")
        return
      }
      XCTAssertEqual(s, "diagonal")
    }
  }

  func test_decode_missing_direction_throws() {
    let body = Data(#"{}"#.utf8)
    XCTAssertThrowsError(try SwipeOnceRoute.decode(body)) { error in
      XCTAssertEqual(
        error as? SwipeOnceRoute.DecodeError,
        SwipeOnceRoute.DecodeError.missingDirection,
      )
    }
  }
}

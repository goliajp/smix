import XCTest
@testable import SmixRunnerCore

// v6.10 c1 — SwipeOnceRoute decode tests. Previously only ScrollRouteTests
// existed; SwipeOnceRoute had implicit coverage via integration. With
// left/right added to the Direction enum, separate decode tests guard
// the parser shape (callers wire HTTP body → decode → handler dispatch).
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

  // v6.10 c1 — left / right newly accepted (cli docs advertised them,
  // runner rejected pre-v6.10 with invalidDirection("left")).
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

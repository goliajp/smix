import XCTest
import FlyingFox
@testable import SmixRunnerCore

final class ScrollRouteTests: XCTestCase {

  private func parse(_ resp: HTTPResponse) async throws -> [String: Any] {
    let data = try await resp.bodyData
    return (try? JSONSerialization.jsonObject(with: data, options: []))
      as? [String: Any] ?? [:]
  }

  // case H: decode happy path — {selector:{text:"X"}, direction:"down"}
  func test_decode_happy_text_down() throws {
    let body = Data(#"{"selector":{"text":"Log out"},"direction":"down"}"#.utf8)
    let req = try ScrollRoute.decode(body)
    XCTAssertEqual(req.selector.text, "Log out")
    XCTAssertEqual(req.direction, "down")
  }

  // case I: decode direction not in {up,down,left,right} → DecodeError
  // left/right are accepted, so the negative path needs a string that is
  // not a direction at all.
  func test_decode_invalidDirection_throws() {
    let body = Data(#"{"selector":{"text":"X"},"direction":"diagonal"}"#.utf8)
    XCTAssertThrowsError(try ScrollRoute.decode(body)) { error in
      XCTAssertTrue(error is ScrollRoute.DecodeError)
    }
  }

  // direction "left" / "right" decode happy path — both are advertised
  // by the cli docs, so the runner must accept them.
  func test_decode_left_direction_accepted() throws {
    let body = Data(#"{"selector":{"text":"X"},"direction":"left"}"#.utf8)
    let req = try ScrollRoute.decode(body)
    XCTAssertEqual(req.direction, "left")
  }

  func test_decode_right_direction_accepted() throws {
    let body = Data(#"{"selector":{"text":"X"},"direction":"right"}"#.utf8)
    let req = try ScrollRoute.decode(body)
    XCTAssertEqual(req.direction, "right")
  }

  // case J: decode missing selector → DecodeError
  func test_decode_missingSelector_throws() {
    let body = Data(#"{"direction":"down"}"#.utf8)
    XCTAssertThrowsError(try ScrollRoute.decode(body)) { error in
      XCTAssertEqual(
        error as? ScrollRoute.DecodeError,
        ScrollRoute.DecodeError.missingSelector
      )
    }
  }

  // case K: success(matched:true, swipes:3) → HTTPResponse 200 body {"matched":true,"swipes":3}
  func test_success_matched_true_serializes() async throws {
    let resp = ScrollRoute.success(matched: true, swipes: 3)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["matched"] as? Bool, true)
    XCTAssertEqual(json["swipes"] as? Int, 3)
  }

  // case L: success(matched:false, swipes:30) → HTTPResponse 200 body {"matched":false,"swipes":30}
  func test_success_matched_false_serializes() async throws {
    let resp = ScrollRoute.success(matched: false, swipes: 30)
    XCTAssertEqual(resp.statusCode, .ok)
    let json = try await parse(resp)
    XCTAssertEqual(json["matched"] as? Bool, false)
    XCTAssertEqual(json["swipes"] as? Int, 30)
  }
}

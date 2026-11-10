import XCTest

@testable import SmixRunnerCore

final class CrownRouteTests: XCTestCase {
  private func decode(_ json: String) throws -> CrownRoute.Request {
    try CrownRoute.decode(Data(json.utf8))
  }

  func testATurnAndAnOptionalVelocityAreRead() throws {
    XCTAssertEqual(try decode(#"{"rotations": -0.5}"#), .init(rotations: -0.5, velocity: nil))
    XCTAssertEqual(try decode(#"{"rotations": 2, "velocity": 1.5}"#), .init(rotations: 2, velocity: 1.5))
    XCTAssertEqual(try decode(#"{"rotations": 1, "velocity": null}"#), .init(rotations: 1, velocity: nil))
  }

  func testNoTurnIsRefusedRatherThanDoneAsNothing() {
    XCTAssertThrowsError(try decode(#"{"rotations": 0}"#)) {
      XCTAssertEqual($0 as? CrownRoute.DecodeError, .zeroRotations)
    }
    XCTAssertThrowsError(try decode(#"{}"#)) {
      XCTAssertEqual($0 as? CrownRoute.DecodeError, .missingRotations)
    }
    XCTAssertThrowsError(try decode(#"{"rotations": "one"}"#)) {
      XCTAssertEqual($0 as? CrownRoute.DecodeError, .notANumber("rotations"))
    }
    XCTAssertThrowsError(try decode(#"{"rotations": 1, "velocity": -2}"#)) {
      XCTAssertEqual($0 as? CrownRoute.DecodeError, .notANumber("velocity"))
    }
  }

  func testADeviceWithoutACrownSaysSoByName() async throws {
    let r = CrownRoute.response(.noCrown("an Apple TV has no Digital Crown"))
    let data = try await r.bodyData
    let body = try JSONSerialization.jsonObject(with: data) as? [String: Any]
    XCTAssertEqual(body?["ok"] as? Bool, false)
    XCTAssertEqual(body?["error"] as? String, "unsupported_on_platform")
    XCTAssertEqual(body?["saw"] as? String, "an Apple TV has no Digital Crown")
  }
}

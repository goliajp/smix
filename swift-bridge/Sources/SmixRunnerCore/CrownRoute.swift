import FlyingFox
import Foundation

// POST /rotate-crown {"rotations": <number>, "velocity": <number>?} → 200.
// Turns the Digital Crown of an Apple Watch or Apple Vision Pro through
// `XCUIDevice.rotateDigitalCrown`; 1 is one full turn, and the sign is the
// direction as XCTest defines it (positive is an upward scroll). A device
// with no crown answers so by name rather than doing nothing.
public enum CrownRoute {
  public struct Request: Equatable, Sendable {
    /// Turns of the crown; 1 is one full turn.
    public let rotations: Double
    /// Turns per second, when given.
    public let velocity: Double?
    public init(rotations: Double, velocity: Double?) {
      self.rotations = rotations
      self.velocity = velocity
    }
  }

  public enum Outcome: Equatable, Sendable {
    case rotated
    /// The device has no crown; the sentence says what it is instead.
    case noCrown(String)
    /// XCUITest raised while turning it.
    case raised
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingRotations
    case notANumber(String)
    case zeroRotations
  }

  public static func decode(_ body: Data) throws -> Request {
    guard let root = (try? JSONSerialization.jsonObject(with: body)) as? [String: Any] else {
      throw DecodeError.invalidJSON
    }
    guard let raw = root["rotations"] else { throw DecodeError.missingRotations }
    guard let rotations = (raw as? NSNumber)?.doubleValue, rotations.isFinite else {
      throw DecodeError.notANumber("rotations")
    }
    guard rotations != 0 else { throw DecodeError.zeroRotations }
    var velocity: Double?
    if let v = root["velocity"], !(v is NSNull) {
      guard let n = (v as? NSNumber)?.doubleValue, n.isFinite, n > 0 else {
        throw DecodeError.notANumber("velocity")
      }
      velocity = n
    }
    return Request(rotations: rotations, velocity: velocity)
  }

  public static func response(_ outcome: Outcome) -> HTTPResponse {
    switch outcome {
    case .rotated:
      return json(.ok, ["ok": true])
    case .noCrown(let saw):
      return json(.ok, ["ok": false, "error": "unsupported_on_platform", "saw": saw])
    case .raised:
      return json(.ok, ["ok": false, "error": "raised", "saw": "XCUITest raised while turning the crown"])
    }
  }

  public static func badRequest(reason: String) -> HTTPResponse {
    json(.badRequest, ["ok": false, "error": "bad_request", "reason": reason])
  }

  private static func json(_ status: HTTPStatusCode, _ object: [String: Any]) -> HTTPResponse {
    // strings and booleans only, which always encode; a string from the
    // device goes through the encoder rather than into a literal
    let body = try! JSONSerialization.data(withJSONObject: object, options: [.sortedKeys])
    return envelope(status, body)
  }

  private static func envelope(_ status: HTTPStatusCode, _ body: Data) -> HTTPResponse {
    HTTPResponse(
      statusCode: status,
      headers: [.contentType: "application/json"],
      body: body
    )
  }
}

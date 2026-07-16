import FlyingFox
import Foundation

// POST /set-orientation {"orientation": "portrait|portraitUpsideDown
// |landscapeLeft|landscapeRight"} → 200 {ok:true|false}.
// Drives `XCUIDevice.shared.orientation`, a documented public framework
// property — no private symbol needs to be dlsym'd for this route.
public enum SetOrientationRoute {
  public struct SetOrientationRequest: Equatable, Sendable {
    public let orientation: String
    public init(orientation: String) {
      self.orientation = orientation
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingOrientation
    case unknownOrientation(String)
    case wrongType(String)
  }

  public static let validOrientations: [String] = [
    "portrait",
    "portraitUpsideDown",
    "landscapeLeft",
    "landscapeRight",
  ]

  public static func decode(_ body: Data) throws -> SetOrientationRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else {
      throw DecodeError.wrongType("root not object")
    }
    guard let raw = root["orientation"] else {
      throw DecodeError.missingOrientation
    }
    guard let s = raw as? String else {
      throw DecodeError.wrongType("orientation not string")
    }
    guard validOrientations.contains(s) else {
      throw DecodeError.unknownOrientation(s)
    }
    return SetOrientationRequest(orientation: s)
  }

  public static func success(ok: Bool) -> HTTPResponse {
    let body = Data(#"{"ok":\#(ok)}"#.utf8)
    return envelope(.ok, body)
  }

  public static func badRequest(reason: String) -> HTTPResponse {
    let r = jsonEscape(reason)
    let body = Data(#"{"ok":false,"error":"bad_request","reason":"\#(r)"}"#.utf8)
    return envelope(.badRequest, body)
  }

  private static func envelope(_ status: HTTPStatusCode, _ body: Data) -> HTTPResponse {
    HTTPResponse(
      statusCode: status,
      headers: [.contentType: "application/json"],
      body: body
    )
  }

  private static func jsonEscape(_ s: String) -> String {
    var out = ""
    out.reserveCapacity(s.count)
    for ch in s {
      switch ch {
      case "\"": out += "\\\""
      case "\\": out += "\\\\"
      case "\n": out += "\\n"
      case "\r": out += "\\r"
      case "\t": out += "\\t"
      default:
        if let scalar = ch.unicodeScalars.first, scalar.value < 0x20 {
          out += String(format: "\\u%04x", scalar.value)
        } else {
          out.append(ch)
        }
      }
    }
    return out
  }
}

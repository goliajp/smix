import FlyingFox
import Foundation

// v1.5 C1 — POST /scroll {selector:{text|id}, direction:"up"|"down"} →
// 200 {matched:<bool>,swipes:<int>}. Mirrors FindRoute / TapRoute envelope
// shape. Runner-side handler reads selector + direction, drives the
// XCUITest swipe-until-visible loop, and returns matched/swipes; this
// route module owns request decoding + response serialization only.
//
// `direction` accepts "up" or "down" only (cold-plan v1.5 §40 maestro
// `scrollUntilVisible` direction set = UP|DOWN; LEFT/RIGHT not in scope
// for this checkpoint). `selector` mirrors FindRoute.Selector but
// additionally accepts `id` so id-based scroll targets are wire-supported
// even though the SimctlDriver currently routes only text selectors here.
public enum ScrollRoute {
  public struct Selector: Equatable, Sendable {
    public let text: String?
    public let id: String?
    public init(text: String? = nil, id: String? = nil) {
      self.text = text
      self.id = id
    }
  }

  public struct ScrollRequest: Equatable, Sendable {
    public let selector: Selector
    public let direction: String
    public init(selector: Selector, direction: String) {
      self.selector = selector
      self.direction = direction
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingSelector
    case missingSelectorField
    case missingDirection
    case invalidDirection(String)
    case wrongType(String)
  }

  public static func decode(_ body: Data) throws -> ScrollRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.wrongType("root not object") }
    guard let sel = root["selector"] else { throw DecodeError.missingSelector }
    guard let selObj = sel as? [String: Any] else { throw DecodeError.wrongType("selector not object") }
    let text = selObj["text"] as? String
    let id = selObj["id"] as? String
    if text == nil && id == nil {
      throw DecodeError.missingSelectorField
    }
    guard let rawDir = root["direction"] else { throw DecodeError.missingDirection }
    guard let direction = rawDir as? String else { throw DecodeError.wrongType("direction not string") }
    // v6.10 c1 — accept left/right in addition to up/down. See
    // SwipeOnceRoute.swift comment for cross-axis semantic note.
    if direction != "up" && direction != "down"
        && direction != "left" && direction != "right" {
      throw DecodeError.invalidDirection(direction)
    }
    return ScrollRequest(
      selector: Selector(text: text, id: id),
      direction: direction
    )
  }

  public static func success(matched: Bool, swipes: Int) -> HTTPResponse {
    let body = Data(#"{"matched":\#(matched),"swipes":\#(swipes)}"#.utf8)
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

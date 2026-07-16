import FlyingFox
import Foundation

// POST /long-press {selector: {text}, durationMs: N} → 200 {ok:true|false}.
// Uses the public XCUIElement.press(forDuration:) API. `durationMs` is in
// milliseconds and defaults to 500 — the maestro cli-2.2.0 default, which
// also matches the XCUIElement standard press of 0.5s.
public enum LongPressRoute {
  public struct LongPressRequest: Equatable, Sendable {
    public let selectorText: String
    public let durationMs: UInt32
    public init(selectorText: String, durationMs: UInt32) {
      self.selectorText = selectorText
      self.durationMs = durationMs
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingSelector
    case missingText
    case missingDuration
    case wrongType(String)
  }

  public static func decode(_ body: Data) throws -> LongPressRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else {
      throw DecodeError.wrongType("root not object")
    }
    guard let selector = root["selector"] else { throw DecodeError.missingSelector }
    guard let selectorObj = selector as? [String: Any] else {
      throw DecodeError.wrongType("selector not object")
    }
    guard let rawText = selectorObj["text"] else { throw DecodeError.missingText }
    guard let text = rawText as? String else {
      throw DecodeError.wrongType("selector.text not string")
    }
    guard let rawDuration = root["durationMs"] else { throw DecodeError.missingDuration }
    let durationMs: UInt32
    if let n = rawDuration as? UInt32 {
      durationMs = n
    } else if let n = rawDuration as? Int, n >= 0 {
      durationMs = UInt32(n)
    } else if let n = rawDuration as? NSNumber {
      durationMs = UInt32(truncating: n)
    } else {
      throw DecodeError.wrongType("durationMs not unsigned int")
    }
    return LongPressRequest(selectorText: text, durationMs: durationMs)
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

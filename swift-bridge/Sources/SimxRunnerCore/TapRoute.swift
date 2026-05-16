import FlyingFox
import Foundation

public enum TapRoute {
  public struct TapRequest: Equatable, Sendable {
    public struct Selector: Equatable, Sendable {
      public let text: String
      public init(text: String) { self.text = text }
    }
    public let selector: Selector
    public init(selector: Selector) { self.selector = selector }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingSelector
    case unsupportedSelectorForm
    case missingText
    case wrongType(String)
  }

  public static func decode(_ body: Data) throws -> TapRequest {
    let json: Any
    do {
      json = try JSONSerialization.jsonObject(with: body, options: [])
    } catch {
      throw DecodeError.invalidJSON
    }
    guard let root = json as? [String: Any] else { throw DecodeError.wrongType("root not object") }
    guard let selector = root["selector"] else { throw DecodeError.missingSelector }
    guard let selectorObj = selector as? [String: Any] else { throw DecodeError.wrongType("selector not object") }
    guard let rawText = selectorObj["text"] else { throw DecodeError.missingText }
    guard let text = rawText as? String else { throw DecodeError.wrongType("selector.text not string") }
    return TapRequest(selector: .init(text: text))
  }

  public static func success(matchedLabel: String) -> HTTPResponse {
    let label = jsonEscape(matchedLabel)
    let body = Data(#"{"ok":true,"matched":{"label":"\#(label)"}}"#.utf8)
    return envelope(.ok, body)
  }

  public static func notFound(selector: TapRequest.Selector) -> HTTPResponse {
    let text = jsonEscape(selector.text)
    let body = Data(#"{"ok":false,"error":"not_found","selector":{"text":"\#(text)"},"visible":[]}"#.utf8)
    return envelope(.notFound, body)
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

  // Minimal JSON-string escape (quote, backslash, control chars). Selector text
  // and matched labels come from user input / iOS a11y values, so escape both.
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

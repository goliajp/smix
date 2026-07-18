import FlyingFox
import Foundation

// POST /double-tap {selector: {text}} → 200 {ok:true} on a tapped
// element, 404 not_found on a miss (mirrors TapRoute). Uses the public
// XCUIElement.doubleTap() API.
//
// A miss used to be 200 {ok:false} — and the Rust client parses this
// body into TapResult, which has no `ok` field, so every miss
// deserialized to an empty success and a double-tap on a non-existent
// element reported Ok end-to-end. Misses are 404 now so the client's
// error path engages.
//
// Body must carry selector.text; missing / non-string → 400 bad_request.
public enum DoubleTapRoute {
  public struct DoubleTapRequest: Equatable, Sendable {
    public let selectorText: String
    public init(selectorText: String) {
      self.selectorText = selectorText
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingSelector
    case missingText
    case wrongType(String)
  }

  public static func decode(_ body: Data) throws -> DoubleTapRequest {
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
    return DoubleTapRequest(selectorText: text)
  }

  public static func success() -> HTTPResponse {
    envelope(.ok, Data(#"{"ok":true}"#.utf8))
  }

  public static func notFound(selectorText: String) -> HTTPResponse {
    let text = jsonEscape(selectorText)
    let body = Data(#"{"ok":false,"error":"not_found","selector":{"text":"\#(text)"}}"#.utf8)
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

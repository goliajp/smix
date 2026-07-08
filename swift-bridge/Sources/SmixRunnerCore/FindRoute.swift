import FlyingFox
import Foundation

// v1.2 C4 — POST /find {selector:{text}} -> {ok:true,found:<bool>}.
//
// `expect.toBeVisible()` for a simple text selector pays a full /tree
// fetch + serialization + SDK-side JS predicate walk. /find lets the
// runner short-circuit with a direct XCUIElement query, returning a
// boolean without snapshotting the whole tree.
//
// Wire shape: `{"selector":{"text":"Foo"}}` request, `{"ok":true,"found":true|false}`
// response. Selector model mirrors KeyboardRoute (single plain-text
// selector); the SDK side only dispatches simple selectors to /find,
// falling back to /tree for complex selectors (inside / below / regex /
// multi-modifier). 404 is not used — element-not-found = `found:false`,
// transport/validation errors use the standard 400/500 envelopes.
public enum FindRoute {
  public struct Selector: Equatable, Sendable {
    public let text: String
    public init(text: String) { self.text = text }
  }

  public struct FindRequest: Equatable, Sendable {
    public let selector: Selector
    public init(selector: Selector) { self.selector = selector }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingSelector
    case missingText
    case wrongType(String)
  }

  public static func decode(_ body: Data) throws -> FindRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.wrongType("root not object") }
    guard let sel = root["selector"] else { throw DecodeError.missingSelector }
    guard let selObj = sel as? [String: Any] else { throw DecodeError.wrongType("selector not object") }
    guard let rawText = selObj["text"] else { throw DecodeError.missingText }
    guard let text = rawText as? String else { throw DecodeError.wrongType("selector.text not string") }
    return FindRequest(selector: Selector(text: text))
  }

  public static func success(found: Bool) -> HTTPResponse {
    let body = Data(#"{"ok":true,"found":\#(found)}"#.utf8)
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

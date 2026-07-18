import FlyingFox
import Foundation

// POST /input-text {"text": <string>} → 200 {ok:<bool>}.
// Mirrors BackRoute / HideKeyboardRoute envelope shape. Types the text into
// the CURRENTLY FOCUSED element — no selector, no focus-tap: the caller
// (FFI SmixSession.input_text; Rust HttpRunnerClient::input_text) is
// responsible for focusing the field first. Same wire contract as the
// Android runner's /input-text (RunnerWire.decodeInputText:
// `getString("text")` — "text" required, must be a string, extra fields
// ignored). Runner-side handler submits the string via the same
// `_XCT_sendString` daemon fast path /fill uses after its focus-tap.
// Route owns request decoding + response serialization only.
public enum InputTextRoute {
  public struct InputTextRequest: Equatable, Sendable {
    public let text: String
    public init(text: String) { self.text = text }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingText
    case wrongType(String)
  }

  public static func decode(_ body: Data) throws -> InputTextRequest {
    // Empty body is NOT acceptable — unlike /back and /hide-keyboard,
    // input-text is not parameterless ("text" is required, matching the
    // Kotlin runner which throws on an absent key).
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.wrongType("root not object") }
    guard let rawText = root["text"] else { throw DecodeError.missingText }
    guard let text = rawText as? String else { throw DecodeError.wrongType("text not string") }
    return InputTextRequest(text: text)
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
    // Same minimal-escape policy as BackRoute / HideKeyboardRoute (control +
    // quote + backslash). Local copy keeps route modules self-contained.
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

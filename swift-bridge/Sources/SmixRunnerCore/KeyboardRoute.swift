import FlyingFox
import Foundation

// v1.2 — runner-side keyboard input routes (/fill, /clear, /press-key).
// Wire shape mirrors TapRoute (single `selector.text` plain selector +
// `text` / `key` payload). Body JSON only — no querystring. Response is
// minimal `{"ok":<bool>}` envelope; success/notFound/badRequest envelopes
// match TapRoute conventions so SDK error mapping can be shared.
public enum KeyboardRoute {
  public struct Selector: Equatable, Sendable {
    public let text: String
    public init(text: String) { self.text = text }
  }

  public struct FillRequest: Equatable, Sendable {
    public let selector: Selector
    public let text: String
    public init(selector: Selector, text: String) {
      self.selector = selector
      self.text = text
    }
  }

  public struct ClearRequest: Equatable, Sendable {
    public let selector: Selector
    public init(selector: Selector) { self.selector = selector }
  }

  public struct PressKeyRequest: Equatable, Sendable {
    public let key: String
    public init(key: String) { self.key = key }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingSelector
    case missingText
    case missingKey
    case wrongType(String)
  }

  public static func decodeFill(_ body: Data) throws -> FillRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.wrongType("root not object") }
    let selector = try decodeSelector(root)
    guard let rawText = root["text"] else { throw DecodeError.missingText }
    guard let text = rawText as? String else { throw DecodeError.wrongType("text not string") }
    return FillRequest(selector: selector, text: text)
  }

  public static func decodeClear(_ body: Data) throws -> ClearRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.wrongType("root not object") }
    let selector = try decodeSelector(root)
    return ClearRequest(selector: selector)
  }

  public static func decodePressKey(_ body: Data) throws -> PressKeyRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.wrongType("root not object") }
    guard let rawKey = root["key"] else { throw DecodeError.missingKey }
    guard let key = rawKey as? String else { throw DecodeError.wrongType("key not string") }
    return PressKeyRequest(key: key)
  }

  // R4.e (audit Low-19) — recognise the explicit `{focused: true}`
  // base form alongside the legacy `{text: "_focused_"}` magic. Both
  // resolve to the same internal `Selector(text: "_focused_")` value
  // so existing handler logic (KeyboardCache hot path / focus-tap
  // skip in tapHandler/fillHandler) keeps working unchanged. SDK
  // consumers now post the new wire shape; legacy magic stays
  // supported as a backward-compat anchor.
  private static func decodeSelector(_ root: [String: Any]) throws -> Selector {
    guard let selector = root["selector"] else { throw DecodeError.missingSelector }
    guard let selectorObj = selector as? [String: Any] else { throw DecodeError.wrongType("selector not object") }
    if let focused = selectorObj["focused"] as? Bool, focused == true {
      return Selector(text: "_focused_")
    }
    guard let rawText = selectorObj["text"] else { throw DecodeError.missingText }
    guard let text = rawText as? String else { throw DecodeError.wrongType("selector.text not string") }
    return Selector(text: text)
  }

  public static func success() -> HTTPResponse {
    let body = Data(#"{"ok":true}"#.utf8)
    return envelope(.ok, body)
  }

  /// v1.2 C2 — success with stage timing (focus_ms = element resolve + focus
  /// tap latency; daemon_send_ms = `_XCT_sendString:` round-trip). Mirrors
  /// TapRoute.TapStages wire convention so SDK-side recordStageLine consumes
  /// both paths uniformly.
  public static func successWithStages(focusMs: UInt32, daemonSendMs: UInt32) -> HTTPResponse {
    let body = Data(
      #"{"ok":true,"stages":{"daemon_send_ms":\#(daemonSendMs),"focus_ms":\#(focusMs)}}"#.utf8
    )
    return envelope(.ok, body)
  }

  /// v1.2 P2 — success with embedded post-action AX tree snapshot.
  /// `treeJsonBody` MUST be a valid JSON object (already serialized via
  /// TreeRoute.serialize); the route splices it inline. Saves the SDK
  /// one HTTP round-trip when an `expect` follows a fill/clear/pressKey.
  public static func successWithTree(treeJsonBody: Data) -> HTTPResponse {
    let treeStr = String(data: treeJsonBody, encoding: .utf8) ?? "null"
    let body = Data(#"{"ok":true,"tree":\#(treeStr)}"#.utf8)
    return envelope(.ok, body)
  }

  public static func notFound(selector: Selector) -> HTTPResponse {
    let text = jsonEscape(selector.text)
    let body = Data(#"{"ok":false,"error":"not_found","selector":{"text":"\#(text)"}}"#.utf8)
    return envelope(.notFound, body)
  }

  public static func unsupportedKey(key: String) -> HTTPResponse {
    let k = jsonEscape(key)
    let body = Data(#"{"ok":false,"error":"unsupported_key","key":"\#(k)"}"#.utf8)
    return envelope(.badRequest, body)
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

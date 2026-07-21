import FlyingFox
import Foundation

// POST /long-press {selector: {text}, durationMs: N} → 200 {ok:true}
// on a pressed element, 404 not_found on a miss (mirrors TapRoute; a
// 200 {ok:false} miss used to deserialize into an empty TapResult on
// the Rust side and report success end-to-end). Uses the public
// XCUIElement.press(forDuration:) API. `durationMs` is in milliseconds
// and defaults to 500 — the maestro cli-2.2.0 default, which also
// matches the XCUIElement standard press of 0.5s.
public enum LongPressRoute {
  public struct LongPressRequest: Equatable, Sendable {
    public let selector: RouteSelector
    public let durationMs: UInt32
    public init(selector: RouteSelector, durationMs: UInt32) {
      self.selector = selector
      self.durationMs = durationMs
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingSelector
    /// No recognised selector key, or a form this route does not take.
    case unsupportedSelectorForm
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
    let sel: RouteSelector
    do { sel = try RouteSelector.decode(from: selectorObj) }
    catch RouteSelector.Failure.wrongType(let what) { throw DecodeError.wrongType(what) }
    catch { throw DecodeError.unsupportedSelectorForm }
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
    return LongPressRequest(selector: sel, durationMs: durationMs)
  }

  public static func success() -> HTTPResponse {
    envelope(.ok, Data(#"{"ok":true}"#.utf8))
  }

  /// Names the key the caller sent rather than always saying `text`.
  public static func notFound(selector: RouteSelector) -> HTTPResponse {
    let raw = jsonEscape(selector.raw)
    let key = selector.wireKey
    let body = Data(#"{"ok":false,"error":"not_found","selector":{"\#(key)":"\#(raw)"}}"#.utf8)
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

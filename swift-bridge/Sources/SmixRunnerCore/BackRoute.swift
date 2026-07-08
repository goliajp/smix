import FlyingFox
import Foundation

// v1.5 C5d' — POST /back {} → 200 {ok:<bool>}.
// Mirrors ForegroundRoute / ScrollRoute / TapRoute envelope shape. Runner-side
// handler queries XCUIApplication.navigationBars.buttons.firstMatch and calls
// .tap() (XCUITest standard navbar back button path, i18n-safe — `firstMatch`
// is positional not label-based, so works across English/Chinese/Japanese
// localization).
// Route owns request decoding + response serialization only.
//
// Empty body or `{}` JSON object both decode to BackRequest. Back is
// app-bound (operates on the runner-bound XCUIApplication) — no bundleId
// or scope parameter, unlike c4b /foreground which takes caller-supplied
// bundleId for cross-app activation.
public enum BackRoute {
  public struct BackRequest: Equatable, Sendable {
    public init() {}
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
  }

  public static func decode(_ body: Data) throws -> BackRequest {
    // Empty body is acceptable — back is parameterless.
    if body.isEmpty { return BackRequest() }
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    // Any valid JSON object body (including `{}`) is acceptable; ignore any
    // unexpected fields — back's contract is parameterless and additive
    // payload fields are forward-compatible (future c{N} may add e.g. count).
    guard json is [String: Any] else { throw DecodeError.invalidJSON }
    return BackRequest()
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
    // Same minimal-escape policy as ScrollRoute / ForegroundRoute (control +
    // quote + backslash). Local copy to keep route modules self-contained.
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

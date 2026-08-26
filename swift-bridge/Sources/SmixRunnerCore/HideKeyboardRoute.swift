import FlyingFox
import Foundation

// POST /hide-keyboard {} → 200 {ok:<bool>}.
// Mirrors BackRoute / ForegroundRoute envelope shape. Runner-side handler
// queries XCUIApplication.shared.keyboards.firstMatch and calls swipeDown()
// when the keyboard is on screen (XCUITest portable; no private API). The
// route owns request decoding + response serialization only.
//
// Empty body or `{}` JSON object both decode to HideKeyboardRequest. Hide-
// keyboard is bound-app frontmost-keyboard scope — no bundleId / no selector
// (mirrors /back which is also parameterless).
public enum HideKeyboardRoute {
  public struct HideKeyboardRequest: Equatable, Sendable {
    public init() {}
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
  }

  public static func decode(_ body: Data) throws -> HideKeyboardRequest {
    // Empty body acceptable — hide-keyboard is parameterless.
    if body.isEmpty { return HideKeyboardRequest() }
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    // Any valid JSON object body (including `{}`) is acceptable; ignore any
    // unexpected fields — hide-keyboard's contract is parameterless and
    // additive payload fields are forward-compatible.
    guard json is [String: Any] else { throw DecodeError.invalidJSON }
    return HideKeyboardRequest()
  }

  /// What actually happened, so the answer can say which.
  ///
  /// `ok:false` used to mean three things at once — the strategies ran and
  /// the keyboard stayed, XCUITest raised inside the guarded block, and the
  /// request context was lost — and a caller cannot act on the union. A
  /// consumer met it with the keyboard unmistakably on screen and could not
  /// tell it from the answer for no keyboard at all.
  ///
  /// The two successes stay one answer on the wire: whether the keyboard was
  /// already gone or was just dismissed, the caller's next step is the same.
  public enum Outcome: Equatable, Sendable {
    /// There was no keyboard. The intent is already satisfied.
    case alreadyGone
    /// There was one, and it is gone now.
    case dismissed
    /// There was one, every strategy ran, and it is still there.
    case stillPresent(tried: String)
    /// Something went wrong while looking. NOT evidence the keyboard is up.
    case couldNotTell(why: String)
  }

  public static func outcome(_ o: Outcome) -> HTTPResponse {
    switch o {
    case .alreadyGone, .dismissed:
      return success(ok: true)
    case .stillPresent(let tried):
      let t = jsonEscape(tried)
      return envelope(.ok, Data(
        #"{"ok":false,"error":"keyboard_did_not_close","saw":"\#(t)"}"#.utf8))
    case .couldNotTell(let why):
      let w = jsonEscape(why)
      return envelope(.ok, Data(
        #"{"ok":false,"error":"keyboard_state_unknown","saw":"\#(w)"}"#.utf8))
    }
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
    // Same minimal-escape policy as BackRoute / ForegroundRoute (control +
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

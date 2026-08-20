import FlyingFox
import Foundation

// POST /back {} → 200 {ok:<bool>}.
// Mirrors ForegroundRoute / ScrollRoute / TapRoute envelope shape. Runner-side
// handler queries XCUIApplication.navigationBars.buttons.firstMatch and calls
// .tap() (XCUITest standard navbar back button path, i18n-safe — `firstMatch`
// is positional not label-based, so works across English/Chinese/Japanese
// localization).
// Route owns request decoding + response serialization only.
//
// Empty body or `{}` JSON object both decode to BackRequest. Back is
// app-bound (operates on the runner-bound XCUIApplication) — no bundleId
// or scope parameter, unlike /foreground which takes a caller-supplied
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
    // payload fields are forward-compatible.
    guard json is [String: Any] else { throw DecodeError.invalidJSON }
    return BackRequest()
  }

  /// Which branch decided the navigation had landed.
  ///
  /// `back` answers a boolean and the boolean was wrong once in ten
  /// corpus runs — the assertion after it read the screen being left
  /// behind. A first fix, made from reading the source, did not change
  /// the rate at all, which is the point at which guessing again is the
  /// wrong move. This says which path the decision took, so the next
  /// change attacks the branch the data names.
  ///
  /// Diagnostic only. Nothing here changes `ok`.
  public enum SettledBy: String, Sendable {
    /// The navigation bar's identifier changed. The signal it was
    /// written for.
    case titleChanged
    /// The bar stayed unfindable long enough to mean the destination
    /// has none.
    case sustainedAbsence
    /// There was no title to compare against, so the handler slept and
    /// reported success without looking. Preserved from before this
    /// route had diagnostics; if the flake correlates with this, it is
    /// the same "no reading treated as an answer" one line earlier.
    case noIdentity
    /// The budget ran out with nothing observed.
    case gaveUp
  }

  /// `saw` is the readings behind the verdict, and it exists because
  /// `gaveUp` alone names no branch.
  ///
  /// The enum says which path decided; on the give-up path there is no
  /// path — the same word covers "there was no back button to tap",
  /// "it was tapped and the title never moved", and "the edge gesture
  /// did nothing". `portable-nav-detail-and-back` has come back
  /// refused on CI since 2026-08-19, four runs, passing on retry and
  /// never once on the author's machine, and no change could be aimed
  /// at it: the refusal reports a budget, not an observation.
  ///
  /// Written only when there is something to report. An empty string on
  /// every ordinary success is how a field stops being read.
  public static func success(
    ok: Bool,
    settledBy: SettledBy? = nil,
    saw: String? = nil
  ) -> HTTPResponse {
    var fields = [#""ok":\#(ok)"#]
    if let settledBy { fields.append(#""settledBy":"\#(settledBy.rawValue)""#) }
    if let saw, !saw.isEmpty { fields.append(#""saw":"\#(jsonEscape(saw))""#) }
    return envelope(.ok, Data("{\(fields.joined(separator: ","))}".utf8))
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

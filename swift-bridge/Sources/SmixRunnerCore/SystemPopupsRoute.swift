import FlyingFox
import Foundation

// v1.4 ③-C1 (third restart) S3.a — `GET /system-popups` route. Per
// CLAUDE.md §9 #8 + §12 (2026-05-20) popup sense is a smix core flat
// capability (peer of /tree, /find, /tap, /fill). The runner UITest target
// enumerates SpringBoard alerts / sheets / dialogs into a list of
// `Popup` POCOs (all Sendable so they flow across the actor boundary back
// into the server); this enum owns the wire envelope serialization,
// keeping SmixRunnerCore free of XCUI / XCTest imports — the same shape
// `TreeRoute` and `FindRoute` use.
public enum SystemPopupsRoute {
  public struct PopupButton: Sendable {
    public let id: String
    public let label: String
    public let role: String
    public let dangerous: Bool
    public let outcomeHint: String?

    public init(
      id: String,
      label: String,
      role: String,
      dangerous: Bool,
      outcomeHint: String?
    ) {
      self.id = id
      self.label = label
      self.role = role
      self.dangerous = dangerous
      self.outcomeHint = outcomeHint
    }
  }

  public struct Popup: Sendable {
    public let id: String
    public let type: String
    public let source: String
    public let title: String
    public let body: String
    public let buttons: [PopupButton]

    public init(
      id: String,
      type: String,
      source: String,
      title: String,
      body: String,
      buttons: [PopupButton]
    ) {
      self.id = id
      self.type = type
      self.source = source
      self.title = title
      self.body = body
      self.buttons = buttons
    }
  }

  public static func success(popups: [Popup]) -> HTTPResponse {
    let body = serialize(popups: popups)
    return HTTPResponse(
      statusCode: .ok,
      headers: [.contentType: "application/json"],
      body: body
    )
  }

  /// v1.0.4 §D4 — 429 emitted when the per-session `/system-popups`
  /// interval floor (500 ms) is not yet elapsed. Callers back off for
  /// `retryAfterMs` then retry. Body carries the retry hint verbatim
  /// so consumers without a Retry-After parser still see the number.
  public static func tooManyRequests(retryAfterMs: Int) -> HTTPResponse {
    let body = Data(
      #"{"ok":false,"error":"rate_limited","retryAfterMs":\#(retryAfterMs)}"#.utf8
    )
    return HTTPResponse(
      statusCode: .tooManyRequests,
      headers: [
        .contentType: "application/json",
        HTTPHeader("Retry-After"): "\((retryAfterMs + 999) / 1000)"
      ],
      body: body
    )
  }

  static func serialize(popups: [Popup]) -> Data {
    var s = #"{"ok":true,"popups":["#
    for (i, p) in popups.enumerated() {
      if i > 0 { s += "," }
      s += encode(popup: p)
    }
    s += "]}"
    return Data(s.utf8)
  }

  private static func encode(popup p: Popup) -> String {
    var s = "{"
    s += #""id":""# + jsonEscape(p.id) + "\","
    s += #""type":""# + jsonEscape(p.type) + "\","
    s += #""source":""# + jsonEscape(p.source) + "\","
    s += #""title":""# + jsonEscape(p.title) + "\","
    s += #""body":""# + jsonEscape(p.body) + "\","
    s += #""buttons":["#
    for (i, b) in p.buttons.enumerated() {
      if i > 0 { s += "," }
      s += encode(button: b)
    }
    s += "]}"
    return s
  }

  private static func encode(button b: PopupButton) -> String {
    var s = "{"
    s += #""id":""# + jsonEscape(b.id) + "\","
    s += #""label":""# + jsonEscape(b.label) + "\","
    s += #""role":""# + jsonEscape(b.role) + "\","
    s += #""dangerous":"# + (b.dangerous ? "true" : "false") + ","
    if let h = b.outcomeHint {
      s += #""outcomeHint":""# + jsonEscape(h) + "\""
    } else {
      s += #""outcomeHint":null"#
    }
    s += "}"
    return s
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

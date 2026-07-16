import FlyingFox
import Foundation

// POST /swipe-at-norm-coord {"fromNx", "fromNy", "toNx", "toNy"}
// → 200 {ok:<bool>}. From-to coordinate swipe gesture via the Apple native
// event chain (XCSynthesizedEventRecord + XCPointerEventPath
// `initForTouchAtPoint:` → `moveToPoint:atOffset:` → `liftUpAtOffset:`).
//
// Companion to `/tap-at-norm-coord`. Swipe is the only coordinate form
// exposed here; fill / anchor / hover coordinate forms are intentionally NOT
// provided.
//
// Envelope shape matches BackRoute / SwipeOnceRoute / TapAtCoordRoute 1:1.
// Body must carry fromNx / fromNy / toNx / toNy ∈ [0,1]; out of range /
// missing / non-numeric → 400 bad_request. Single mode (no `mode` field), so
// the handler dispatches directly.
public enum SwipeAtCoordRoute {
  public struct SwipeAtCoordRequest: Equatable, Sendable {
    public let fromNx: Double
    public let fromNy: Double
    public let toNx: Double
    public let toNy: Double
    public init(fromNx: Double, fromNy: Double, toNx: Double, toNy: Double) {
      self.fromNx = fromNx
      self.fromNy = fromNy
      self.toNx = toNx
      self.toNy = toNy
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingField(String)
    case invalidField(String, String)
    case outOfRange(String, Double)
  }

  public static func decode(_ body: Data) throws -> SwipeAtCoordRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.invalidJSON }
    func num(_ key: String) throws -> Double {
      guard let raw = root[key] else { throw DecodeError.missingField(key) }
      if let n = raw as? Double { return n }
      if let n = raw as? NSNumber { return n.doubleValue }
      throw DecodeError.invalidField(key, "\(raw)")
    }
    func range(_ key: String, _ v: Double) throws {
      if v < 0 || v > 1 { throw DecodeError.outOfRange(key, v) }
    }
    let fromNx = try num("fromNx")
    let fromNy = try num("fromNy")
    let toNx = try num("toNx")
    let toNy = try num("toNy")
    try range("fromNx", fromNx)
    try range("fromNy", fromNy)
    try range("toNx", toNx)
    try range("toNy", toNy)
    return SwipeAtCoordRequest(
      fromNx: fromNx, fromNy: fromNy, toNx: toNx, toNy: toNy)
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

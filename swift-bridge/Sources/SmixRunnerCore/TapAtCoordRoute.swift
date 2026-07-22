import FlyingFox
import Foundation

// POST /tap-at-norm-coord {"nx":<0..1>, "ny":<0..1>} → 200 {ok:<bool>}.
// Coordinate-based tap via `XCUIApplication.coordinate(withNormalizedOffset:).tap()`
// — the Apple native UI event chain, which does fire RN Pressable React
// events. Pairs a host-side DFS-first resolve with the runner's native event
// chain: the host supplies the concrete coord, so the runner never re-queries
// Apple's element table and thereby sidesteps ambiguous-text ordering.
//
// Envelope shape matches BackRoute / SwipeOnceRoute 1:1. Body must carry
// nx + ny ∈ [0,1]; out of range / missing / non-numeric → 400 bad_request.
// Single mode (no `mode` field), so the handler dispatches directly.
public enum TapAtCoordRoute {
  public struct TapAtCoordRequest: Equatable, Sendable {
    public let nx: Double
    public let ny: Double
    public init(nx: Double, ny: Double) {
      self.nx = nx
      self.ny = ny
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingField(String)
    case invalidField(String, String)
    case outOfRange(String, Double)
  }

  public static func decode(_ body: Data) throws -> TapAtCoordRequest {
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
    let nx = try num("nx")
    let ny = try num("ny")
    if nx < 0 || nx > 1 { throw DecodeError.outOfRange("nx", nx) }
    if ny < 0 || ny > 1 { throw DecodeError.outOfRange("ny", ny) }
    return TapAtCoordRequest(nx: nx, ny: ny)
  }

  public static func success(ok: Bool) -> HTTPResponse {
    success(ok: ok, chain: [])
  }

  /// Success, carrying what the tapped point turned out to be inside.
  ///
  /// The route answered `{"ok":true}` and nothing else, which meant "a
  /// touch was synthesised at that coordinate" and was read as "the
  /// element was tapped". A consumer watched taps succeed against a
  /// button whose counter never moved and found out which one they
  /// were getting.
  ///
  /// `chain` is every named element containing the point, innermost
  /// first — not one element, because the innermost thing at a button's
  /// centre is usually the button's own label. An older host ignores
  /// the extra key.
  public static func success(ok: Bool, chain: [HitChainEntry]) -> HTTPResponse {
    let entries = chain.map { e in
      let id = jsonEscape(e.identifier)
      let label = jsonEscape(e.label)
      return #"{"identifier":"\#(id)","label":"\#(label)","frame":"#
        + #"{"x":\#(e.frame.origin.x),"y":\#(e.frame.origin.y),"#
        + #""w":\#(e.frame.size.width),"h":\#(e.frame.size.height)}}"#
    }
    let body = Data(#"{"ok":\#(ok),"chain":[\#(entries.joined(separator: ","))]}"#.utf8)
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

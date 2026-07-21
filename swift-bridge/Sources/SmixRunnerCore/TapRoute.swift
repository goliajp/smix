import FlyingFox
import Foundation

#if canImport(CoreGraphics)
import CoreGraphics
#endif

public enum TapRoute {
  // `resolve` returns the matched element's frame so the SDK can inject the
  // tap via host-HID at that coordinate; `resolveAndTap` performs the
  // XCUIElement.tap() call in-process, which blocks on a synchronous
  // animation wait.
  public enum TapMode: String, Sendable {
    case resolve
    case resolveAndTap
    case daemonProxySynthesize
  }

  public struct TapRequest: Equatable, Sendable {
    public let selector: RouteSelector
    public let mode: TapMode
    public init(selector: RouteSelector, mode: TapMode = .resolveAndTap) {
      self.selector = selector
      self.mode = mode
    }
  }

  // Runner-side per-tap latency breakdown.
  // `resolveMs` covers element resolution (predicate match + isHittable).
  // `tapCallMs` covers the XCUIElement.tap() synchronous call, typically the
  // dominant stage. `totalMs` covers the whole tapHandler closure.
  //
  // `waitExistenceMs` / `frameReadMs` are optional sub-stage timers within
  // `resolveMs`: both are nil on the resolveAndTap path and are filled only
  // when mode=resolve.
  public struct TapStages: Equatable, Sendable {
    public let resolveMs: Double
    public let tapCallMs: Double
    public let totalMs: Double
    public let waitExistenceMs: Double?
    public let frameReadMs: Double?
    public init(
      resolveMs: Double,
      tapCallMs: Double,
      totalMs: Double,
      waitExistenceMs: Double? = nil,
      frameReadMs: Double? = nil
    ) {
      self.resolveMs = resolveMs
      self.tapCallMs = tapCallMs
      self.totalMs = totalMs
      self.waitExistenceMs = waitExistenceMs
      self.frameReadMs = frameReadMs
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingSelector
    /// No recognised selector key, or a form this route does not take.
    case unsupportedSelectorForm
    case wrongType(String)
  }

  public static func decode(_ body: Data) throws -> TapRequest {
    let json: Any
    do {
      json = try JSONSerialization.jsonObject(with: body, options: [])
    } catch {
      throw DecodeError.invalidJSON
    }
    guard let root = json as? [String: Any] else { throw DecodeError.wrongType("root not object") }
    guard let selector = root["selector"] else { throw DecodeError.missingSelector }
    guard let selectorObj = selector as? [String: Any] else { throw DecodeError.wrongType("selector not object") }
    let sel: RouteSelector
    do { sel = try RouteSelector.decode(from: selectorObj) }
    catch RouteSelector.Failure.wrongType(let what) { throw DecodeError.wrongType(what) }
    catch { throw DecodeError.unsupportedSelectorForm }
    let mode: TapMode
    if let rawMode = root["mode"] {
      guard let modeStr = rawMode as? String, let parsed = TapMode(rawValue: modeStr) else {
        throw DecodeError.wrongType("mode not 'resolve' | 'resolveAndTap'")
      }
      mode = parsed
    } else {
      mode = .resolveAndTap
    }
    return TapRequest(selector: sel, mode: mode)
  }

  // The body shape is the Rust `smix_runner_wire::TapResult` contract:
  // top-level camelCase fields, `Rect` always emitted with all four of
  // x/y/w/h. The previous emission nested frame/appFrame under a
  // "matched" object and wrote stages keys in snake_case; every
  // `#[serde(default)]` on the Rust side made that parse "successfully"
  // to all-None/zero, so the drift was invisible until probed. The shape
  // here is now asserted against the Rust crate by
  // crates/smix-runner-wire/tests/tap_route_shape.rs.
  public static func success(
    matchedLabel: String,
    stages: TapStages? = nil,
    frame: CGRect? = nil,
    appFrame: CGRect? = nil
  ) -> HTTPResponse {
    let label = jsonEscape(matchedLabel)
    var fields = #""ok":true,"matchedLabel":"\#(label)""#
    if let f = frame {
      fields += #","frame":\#(rectJson(f))"#
    }
    if let af = appFrame {
      fields += #","appFrame":\#(rectJson(af))"#
    }
    if let s = stages {
      let r = String(format: "%.1f", s.resolveMs)
      let t = String(format: "%.1f", s.tapCallMs)
      let n = String(format: "%.1f", s.totalMs)
      var stagesFields = #""resolveMs":\#(r),"tapCallMs":\#(t),"totalMs":\#(n)"#
      if let w = s.waitExistenceMs {
        stagesFields += #","waitExistenceMs":\#(String(format: "%.1f", w))"#
      }
      if let f = s.frameReadMs {
        stagesFields += #","frameReadMs":\#(String(format: "%.1f", f))"#
      }
      fields += #","stages":{\#(stagesFields)}"#
    }
    return envelope(.ok, Data("{\(fields)}".utf8))
  }

  private static func rectJson(_ r: CGRect) -> String {
    let x = String(format: "%.2f", r.origin.x)
    let y = String(format: "%.2f", r.origin.y)
    let w = String(format: "%.2f", r.size.width)
    let h = String(format: "%.2f", r.size.height)
    return #"{"x":\#(x),"y":\#(y),"w":\#(w),"h":\#(h)}"#
  }

  /// Reports the miss under the key the caller actually sent, rather
  /// than always claiming `text`. The Rust client treats this body as
  /// an opaque string, so widening it breaks no parser.
  public static func notFound(selector: RouteSelector) -> HTTPResponse {
    let raw = jsonEscape(selector.raw)
    let key = selector.wireKey
    let body = Data(#"{"ok":false,"error":"not_found","selector":{"\#(key)":"\#(raw)"},"visible":[]}"#.utf8)
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

  // Minimal JSON-string escape (quote, backslash, control chars). Selector text
  // and matched labels come from user input / iOS a11y values, so escape both.
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

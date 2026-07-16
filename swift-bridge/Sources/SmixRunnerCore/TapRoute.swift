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
    public struct Selector: Equatable, Sendable {
      public let text: String
      public init(text: String) { self.text = text }
    }
    public let selector: Selector
    public let mode: TapMode
    public init(selector: Selector, mode: TapMode = .resolveAndTap) {
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
    case unsupportedSelectorForm
    case missingText
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
    guard let rawText = selectorObj["text"] else { throw DecodeError.missingText }
    guard let text = rawText as? String else { throw DecodeError.wrongType("selector.text not string") }
    let mode: TapMode
    if let rawMode = root["mode"] {
      guard let modeStr = rawMode as? String, let parsed = TapMode(rawValue: modeStr) else {
        throw DecodeError.wrongType("mode not 'resolve' | 'resolveAndTap'")
      }
      mode = parsed
    } else {
      mode = .resolveAndTap
    }
    return TapRequest(selector: .init(text: text), mode: mode)
  }

  public static func success(
    matchedLabel: String,
    stages: TapStages? = nil,
    frame: CGRect? = nil,
    appFrame: CGRect? = nil
  ) -> HTTPResponse {
    let label = jsonEscape(matchedLabel)
    var matchedFields = #""label":"\#(label)""#
    if let f = frame {
      let x = String(format: "%.2f", f.origin.x)
      let y = String(format: "%.2f", f.origin.y)
      let w = String(format: "%.2f", f.size.width)
      let h = String(format: "%.2f", f.size.height)
      matchedFields += #","frame":{"x":\#(x),"y":\#(y),"w":\#(w),"h":\#(h)}"#
    }
    if let af = appFrame {
      let w = String(format: "%.2f", af.size.width)
      let h = String(format: "%.2f", af.size.height)
      matchedFields += #","appFrame":{"w":\#(w),"h":\#(h)}"#
    }
    let body: Data
    if let s = stages {
      let r = String(format: "%.1f", s.resolveMs)
      let t = String(format: "%.1f", s.tapCallMs)
      let n = String(format: "%.1f", s.totalMs)
      var stagesFields = #""resolve_ms":\#(r),"tap_call_ms":\#(t),"total_ms":\#(n)"#
      if let w = s.waitExistenceMs {
        stagesFields += #","wait_existence_ms":\#(String(format: "%.1f", w))"#
      }
      if let f = s.frameReadMs {
        stagesFields += #","frame_read_ms":\#(String(format: "%.1f", f))"#
      }
      body = Data(#"{"ok":true,"matched":{\#(matchedFields)},"stages":{\#(stagesFields)}}"#.utf8)
    } else {
      body = Data(#"{"ok":true,"matched":{\#(matchedFields)}}"#.utf8)
    }
    return envelope(.ok, body)
  }

  public static func notFound(selector: TapRequest.Selector) -> HTTPResponse {
    let text = jsonEscape(selector.text)
    let body = Data(#"{"ok":false,"error":"not_found","selector":{"text":"\#(text)"},"visible":[]}"#.utf8)
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

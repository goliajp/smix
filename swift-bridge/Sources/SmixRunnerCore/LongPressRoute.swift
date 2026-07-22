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

  /// Bounds on when the touch was actually down, measured around the
  /// synthesised gesture.
  ///
  /// The call that performs the gesture is opaque — it returns after
  /// the touch lifts, and nothing reports the instant it went down.
  /// What is measurable is the call's own span `[A, B]` and the hold
  /// `d` the timeline was authored with. A hold of `d` contained in
  /// `[A, B]` means the touch went down no later than `B - d` and
  /// lifted no earlier than `A + d`. Those two bounds hold whatever the
  /// call did with the rest of its time, which is why they, rather
  /// than a guessed instant, go on the wire.
  ///
  /// This is sound only because the caller authors the timeline. It was
  /// applied to `XCUIElement.press(forDuration:)` first, and that is
  /// where it broke: on iPhone 17 Pro / iOS 26.5 that call took a
  /// constant ~2.6s for every hold from 500ms to 6000ms, so `B - A`
  /// bore no relation to `d` and a 4000ms request produced a "4000ms
  /// certainly held" window inside a 2.6s call. Measured overhead
  /// around the synthesised gesture is 290-342ms and independent of
  /// `d`.
  public struct PressTimings: Equatable, Sendable {
    /// Handler entry → latest instant the touch could have gone down.
    public let latestDownOffsetMs: UInt32
    /// Handler entry → earliest instant the touch could have lifted.
    public let earliestUpOffsetMs: UInt32
    /// Handler entry → handler return.
    public let handlerWallMs: UInt32

    public init(latestDownOffsetMs: UInt32, earliestUpOffsetMs: UInt32, handlerWallMs: UInt32) {
      self.latestDownOffsetMs = latestDownOffsetMs
      self.earliestUpOffsetMs = earliestUpOffsetMs
      self.handlerWallMs = handlerWallMs
    }

    /// Derive the bounds from the call span and the requested hold.
    public static func around(
      callStartMs: Double, callEndMs: Double, holdMs: UInt32, handlerEntryMs: Double
    ) -> PressTimings {
      let hold = Double(holdMs)
      let latestDown = max(callStartMs, callEndMs - hold) - handlerEntryMs
      let earliestUp = callStartMs + hold - handlerEntryMs
      return PressTimings(
        latestDownOffsetMs: UInt32(max(0, latestDown.rounded())),
        earliestUpOffsetMs: UInt32(max(0, earliestUp.rounded())),
        handlerWallMs: UInt32(max(0, (callEndMs - handlerEntryMs).rounded()))
      )
    }
  }

  public static func success(timings: PressTimings? = nil) -> HTTPResponse {
    guard let t = timings else {
      return envelope(.ok, Data(#"{"ok":true}"#.utf8))
    }
    let body = Data(#"{"ok":true,"latestDownOffsetMs":\#(t.latestDownOffsetMs),"earliestUpOffsetMs":\#(t.earliestUpOffsetMs),"handlerWallMs":\#(t.handlerWallMs)}"#.utf8)
    return envelope(.ok, body)
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

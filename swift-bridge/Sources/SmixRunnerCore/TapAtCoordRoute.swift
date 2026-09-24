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
    /// How many touches to deliver. Absent or 1 is an ordinary tap.
    ///
    /// A burst is one synthesise carrying several pointer paths, so the
    /// gap between touches is the stated interval rather than a round
    /// trip — which at ~400 ms each is what made a rapid-tap gesture
    /// undriveable.
    public let times: Int
    /// Milliseconds between touches in a burst.
    public let intervalMs: Int
    /// Milliseconds each touch stays down.
    public let holdMs: Int

    public init(
      nx: Double, ny: Double,
      times: Int = 1,
      intervalMs: Int = TouchTimeline.defaultIntervalMs,
      holdMs: Int = TouchTimeline.defaultHoldMs
    ) {
      self.nx = nx
      self.ny = ny
      self.times = times
      self.intervalMs = intervalMs
      self.holdMs = holdMs
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
    let times = (root["times"] as? NSNumber)?.intValue ?? 1
    let intervalMs =
      (root["intervalMs"] as? NSNumber)?.intValue ?? TouchTimeline.defaultIntervalMs
    let holdMs = (root["holdMs"] as? NSNumber)?.intValue ?? TouchTimeline.defaultHoldMs
    return TapAtCoordRequest(
      nx: nx, ny: ny, times: times, intervalMs: intervalMs, holdMs: holdMs)
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
    success(ok: ok, chain: chain, press: nil)
  }

  /// Success for one held touch, carrying when it was down.
  ///
  /// This route became the long press when `/long-press` retired, so the
  /// bounds that route answered with ride here, in the same words.
  /// Absent for a burst and from a press that could not be timed; the
  /// host reads absence as "cannot be placed".
  public static func success(
    ok: Bool, chain: [HitChainEntry], press: PressTimings?
  ) -> HTTPResponse {
    let entries = chain.map { e in
      let id = jsonEscape(e.identifier)
      let label = jsonEscape(e.label)
      return #"{"identifier":"\#(id)","label":"\#(label)","frame":"#
        + #"{"x":\#(e.frame.origin.x),"y":\#(e.frame.origin.y),"#
        + #""w":\#(e.frame.size.width),"h":\#(e.frame.size.height)}}"#
    }
    var json = #"{"ok":\#(ok),"chain":[\#(entries.joined(separator: ","))]"#
    if let t = press {
      json += #","latestDownOffsetMs":\#(t.latestDownOffsetMs),"#
        + #""earliestUpOffsetMs":\#(t.earliestUpOffsetMs),"handlerWallMs":\#(t.handlerWallMs)"#
    }
    let body = Data((json + "}").utf8)
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

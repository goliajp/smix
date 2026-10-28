import CoreGraphics
import FlyingFox
import Foundation

// POST /pinch-at-norm-coord
// {"cx","cy","fromSpan","toSpan","durationMs","panDx"?,"panDy"?,"panMs"?}
// → 200 {ok, fingers} | 400 bad_request.
//
// Two fingers side by side on a horizontal line through (cx, cy), moved
// apart or together from `fromSpan` to `toSpan`. Spans are shares of the
// screen's shorter side, so a ratio means the same zoom in either
// orientation. With `panMs > 0` both fingers then move together by
// (panDx × width, panDy × height) before lifting — the drag after a zoom,
// done in the same touch because a touch cannot be held across requests.
//
// A finger that would leave the screen at any moment is refused, naming
// the finger and the point. Clamping it would pinch by a different ratio
// than the one asked for and report it as done.
public enum PinchRoute {
  public struct PinchRequest: Equatable, Sendable {
    public let cx: Double
    public let cy: Double
    public let fromSpan: Double
    public let toSpan: Double
    public let durationMs: Int
    public let panDx: Double
    public let panDy: Double
    public let panMs: Int
    public init(
      cx: Double, cy: Double, fromSpan: Double, toSpan: Double, durationMs: Int,
      panDx: Double = 0, panDy: Double = 0, panMs: Int = 0
    ) {
      self.cx = cx
      self.cy = cy
      self.fromSpan = fromSpan
      self.toSpan = toSpan
      self.durationMs = durationMs
      self.panDx = panDx
      self.panDy = panDy
      self.panMs = panMs
    }
  }

  /// Where each finger is at the three moments of the gesture, in the
  /// same units as the size the plan was made for.
  public struct PinchPlan: Equatable, Sendable {
    /// Where the fingers come down: `prerollPoints` short of `start`.
    public let down: Fingers
    public let start: Fingers
    public let pinched: Fingers
    public let end: Fingers
    public init(down: Fingers, start: Fingers, pinched: Fingers, end: Fingers) {
      self.down = down
      self.start = start
      self.pinched = pinched
      self.end = end
    }
  }

  public struct Fingers: Equatable, Sendable {
    public let first: CGPoint
    public let second: CGPoint
    public init(_ first: CGPoint, _ second: CGPoint) {
      self.first = first
      self.second = second
    }
  }

  public enum DecodeError: Error, Equatable, CustomStringConvertible {
    case invalidJSON
    case missingField(String)
    case invalidField(String, String)
    case outOfRange(String, Double)
    case offScreen(finger: Int, moment: String, x: Double, y: Double)

    public var description: String {
      switch self {
      case .invalidJSON: return "body is not a JSON object"
      case .missingField(let k): return "missing field \(k)"
      case .invalidField(let k, let v): return "\(k) is not a number: \(v)"
      case .outOfRange(let k, let v): return "\(k) out of range: \(v)"
      case .offScreen(let finger, let moment, let x, let y):
        return String(
          format: "finger %d leaves the screen %@ at (%.0f, %.0f); move the centre or use smaller spans",
          finger, moment, x, y)
      }
    }
  }

  /// What the host-side handler did with a request.
  public enum Outcome: Sendable {
    /// Both fingers' paths went to the event daemon.
    case dispatched(PinchPlan)
    /// The plan was sound but the touches could not be synthesised.
    case notDispatched
    /// The request cannot be performed on this screen; nothing was sent.
    case refused(String)
  }

  public static func response(for outcome: Outcome) -> HTTPResponse {
    switch outcome {
    case .dispatched(let plan): return success(ok: true, plan: plan)
    case .notDispatched: return failure()
    case .refused(let reason): return badRequest(reason: reason)
    }
  }

  /// A pinch recognizer counts from where it begins, not from where the
  /// fingers came down, and it begins only after the fingers have moved a
  /// few points: on a UIScrollView about 4.5 points each, whatever the
  /// span. A 1 → 3 pinch sent as asked left it at 2.2-2.7x. So each finger
  /// comes down `prerollPoints` short of its start, moves there over
  /// `prerollMs`, and the counted pinch starts with the recognizer already
  /// running; measured 1 → 3 at 3.03-3.12x and 1 → 2 at 2.03x over
  /// 300-1500 ms.
  public static let prerollMs = 200
  public static let prerollPoints = 5.0

  /// Both fingers stay still this long at the end before lifting.
  public static let holdBeforeLiftMs = 200

  public static let durationRange = 50...10_000
  public static let panRange = 0...10_000

  public static func decode(_ body: Data) throws -> PinchRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.invalidJSON }
    func num(_ key: String, default value: Double? = nil) throws -> Double {
      guard let raw = root[key] else {
        if let value { return value }
        throw DecodeError.missingField(key)
      }
      if let n = raw as? NSNumber, CFGetTypeID(n) != CFBooleanGetTypeID() { return n.doubleValue }
      throw DecodeError.invalidField(key, "\(raw)")
    }
    func inRange(_ key: String, _ v: Double, _ ok: (Double) -> Bool) throws {
      if !ok(v) { throw DecodeError.outOfRange(key, v) }
    }
    let cx = try num("cx")
    let cy = try num("cy")
    let fromSpan = try num("fromSpan")
    let toSpan = try num("toSpan")
    let duration = try num("durationMs")
    let panDx = try num("panDx", default: 0)
    let panDy = try num("panDy", default: 0)
    let panMs = try num("panMs", default: 0)
    try inRange("cx", cx) { (0...1).contains($0) }
    try inRange("cy", cy) { (0...1).contains($0) }
    try inRange("fromSpan", fromSpan) { $0 > 0 && $0 <= 1 }
    try inRange("toSpan", toSpan) { $0 > 0 && $0 <= 1 }
    try inRange("durationMs", duration) { durationRange.contains(Int($0)) }
    try inRange("panDx", panDx) { (-1...1).contains($0) }
    try inRange("panDy", panDy) { (-1...1).contains($0) }
    try inRange("panMs", panMs) { panRange.contains(Int($0)) }
    return PinchRequest(
      cx: cx, cy: cy, fromSpan: fromSpan, toSpan: toSpan, durationMs: Int(duration),
      panDx: panDx, panDy: panDy, panMs: Int(panMs))
  }

  /// The finger positions for `req` on a screen of `size`, or the first
  /// finger and moment that would be off it.
  public static func plan(_ req: PinchRequest, size: CGSize) throws -> PinchPlan {
    let w = Double(size.width)
    let h = Double(size.height)
    let short = min(w, h)
    let mid = CGPoint(x: req.cx * w, y: req.cy * h)
    func pair(_ centre: CGPoint, _ span: Double) -> Fingers {
      let half = CGFloat(span * short / 2)
      return Fingers(
        CGPoint(x: centre.x - half, y: centre.y),
        CGPoint(x: centre.x + half, y: centre.y)
      )
    }
    let start = pair(mid, req.fromSpan)
    let pinched = pair(mid, req.toSpan)
    let moved = CGPoint(x: mid.x + CGFloat(req.panDx * w), y: mid.y + CGFloat(req.panDy * h))
    let end = req.panMs > 0 ? pair(moved, req.toSpan) : pinched
    func shortOf(_ from: CGPoint, towards to: CGPoint) -> CGPoint {
      let dx = to.x - from.x
      let dy = to.y - from.y
      let len = max(1, (dx * dx + dy * dy).squareRoot())
      let by = CGFloat(prerollPoints)
      return CGPoint(x: from.x - dx / len * by, y: from.y - dy / len * by)
    }
    let down = Fingers(
      shortOf(start.first, towards: pinched.first), shortOf(start.second, towards: pinched.second))
    for (moment, fingers) in [
      ("as it comes down", down), ("at the start", start), ("after the pinch", pinched),
      ("after the pan", end),
    ] {
      for (i, p) in [fingers.first, fingers.second].enumerated() {
        if p.x < 0 || p.x > CGFloat(w) || p.y < 0 || p.y > CGFloat(h) {
          throw DecodeError.offScreen(finger: i + 1, moment: moment, x: Double(p.x), y: Double(p.y))
        }
      }
    }
    return PinchPlan(down: down, start: start, pinched: pinched, end: end)
  }

  public static func success(ok: Bool, plan: PinchPlan) -> HTTPResponse {
    func pt(_ p: CGPoint) -> String { #"{"x":\#(Int(p.x.rounded())),"y":\#(Int(p.y.rounded()))}"# }
    func two(_ f: Fingers) -> String { "[\(pt(f.first)),\(pt(f.second))]" }
    let fingers = #"{"from":\#(two(plan.start)),"to":\#(two(plan.end))}"#
    let body = Data(#"{"ok":\#(ok),"fingers":\#(fingers)}"#.utf8)
    return envelope(.ok, body)
  }

  public static func failure() -> HTTPResponse {
    envelope(.ok, Data(#"{"ok":false}"#.utf8))
  }

  public static func badRequest(reason: String) -> HTTPResponse {
    let r = reason
      .replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
      .replacingOccurrences(of: "\n", with: "\\n")
    let body = Data(#"{"ok":false,"error":"bad_request","reason":"\#(r)"}"#.utf8)
    return envelope(.badRequest, body)
  }

  private static func envelope(_ status: HTTPStatusCode, _ body: Data) -> HTTPResponse {
    HTTPResponse(statusCode: status, headers: [.contentType: "application/json"], body: body)
  }
}

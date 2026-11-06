import CoreGraphics
import FlyingFox
import Foundation

// POST /drag-at-norm-coord
// {"fromNx","fromNy","toNx","toNy","holdMs","durationMs","releaseMs"}
// → 200 {ok} | 400 bad_request.
//
// One finger down at (fromNx, fromNy), still for `holdMs`, moved to
// (toNx, toNy) over `durationMs`, still there for `releaseMs`, lifted.
// The hold is what lets a home-screen icon or a launcher widget come
// loose before the finger moves; the rest at the end is what lets a drop
// target notice the finger before it lets go. A swipe has neither.
//
// Every field is required: the host fills in the defaults, so the two
// runners never disagree about what an absent one means.
public enum DragRoute {
  public struct DragRequest: Equatable, Sendable {
    public let fromNx: Double
    public let fromNy: Double
    public let toNx: Double
    public let toNy: Double
    public let holdMs: Int
    public let durationMs: Int
    public let releaseMs: Int
    public init(
      fromNx: Double, fromNy: Double, toNx: Double, toNy: Double,
      holdMs: Int, durationMs: Int, releaseMs: Int
    ) {
      self.fromNx = fromNx
      self.fromNy = fromNy
      self.toNx = toNx
      self.toNy = toNy
      self.holdMs = holdMs
      self.durationMs = durationMs
      self.releaseMs = releaseMs
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingField(String)
    case invalidField(String)
    case outOfRange(String, Double)
  }

  /// The ranges the host checks too (`smix_input::Drag::check`).
  static let holdRange = 0...10_000
  static let durationRange = 50...10_000
  static let releaseRange = 0...5_000

  public static func decode(_ body: Data) throws -> DragRequest {
    guard let root = try? JSONSerialization.jsonObject(with: body) as? [String: Any] else {
      throw DecodeError.invalidJSON
    }
    func num(_ key: String) throws -> Double {
      guard let raw = root[key] else { throw DecodeError.missingField(key) }
      guard let n = raw as? NSNumber, !(raw is Bool) else { throw DecodeError.invalidField(key) }
      return n.doubleValue
    }
    func share(_ key: String) throws -> Double {
      let v = try num(key)
      if v < 0 || v > 1 { throw DecodeError.outOfRange(key, v) }
      return v
    }
    func ms(_ key: String, _ range: ClosedRange<Int>) throws -> Int {
      let v = try num(key)
      guard v == v.rounded(), range.contains(Int(v)) else { throw DecodeError.outOfRange(key, v) }
      return Int(v)
    }
    return DragRequest(
      fromNx: try share("fromNx"), fromNy: try share("fromNy"),
      toNx: try share("toNx"), toNy: try share("toNy"),
      holdMs: try ms("holdMs", holdRange),
      durationMs: try ms("durationMs", durationRange),
      releaseMs: try ms("releaseMs", releaseRange))
  }

  /// One point of the finger's path: where it is, and when, in seconds
  /// after it went down.
  public struct Waypoint: Equatable, Sendable {
    public let point: CGPoint
    public let offset: TimeInterval
    public init(_ point: CGPoint, _ offset: TimeInterval) {
      self.point = point
      self.offset = offset
    }
  }

  /// The finger's path in `size` units, down first and lift last. A
  /// still stretch is two waypoints at the same point: the event path
  /// moves in a straight line between consecutive waypoints, so without
  /// the second one the finger would start drifting the moment it landed.
  public static func path(_ r: DragRequest, size: CGSize) -> [Waypoint] {
    let from = CGPoint(x: size.width * r.fromNx, y: size.height * r.fromNy)
    let to = CGPoint(x: size.width * r.toNx, y: size.height * r.toNy)
    let held = TimeInterval(r.holdMs) / 1000
    let arrived = held + TimeInterval(r.durationMs) / 1000
    let lifted = arrived + TimeInterval(r.releaseMs) / 1000
    var out = [Waypoint(from, 0)]
    if r.holdMs > 0 { out.append(Waypoint(from, held)) }
    out.append(Waypoint(to, arrived))
    if r.releaseMs > 0 { out.append(Waypoint(to, lifted)) }
    return out
  }

  public static func success(ok: Bool) -> HTTPResponse {
    envelope(.ok, Data(#"{"ok":\#(ok)}"#.utf8))
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

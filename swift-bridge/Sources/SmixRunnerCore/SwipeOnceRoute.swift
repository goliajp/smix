import FlyingFox
import Foundation

// POST /swipe-once {"direction":"up"|"down"} → 200 {ok:<bool>}.
// Single-swipe gesture, no probe, no selector. The driver-side host loop calls
// this between host-side dict tree probes (via driver.tree() + resolveSelector)
// to bypass the runner-side XCUIElement query.firstMatch stall on dict-only RN
// elements, which otherwise trips the FlyingFox 15s handler timeout and turns
// /scroll into a 500.
//
// Mirrors BackRoute / ScrollRoute envelope shape. Direction "up" / "down"
// follows the same semantic as ScrollRoute (the SDK / driver maps content
// scroll direction to swipe gesture direction inside the handler).
public enum SwipeOnceRoute {
  // left/right follow the XCUIElement primitives (swipeLeft/swipeRight) and
  // the Kotlin runner's left/right impl: the value names the FINGER direction
  // (swipeLeft = finger travels right to left, content follows the finger
  // left, revealing right-side hidden content).
  //
  // NOTE: the up/down convention on this axis is INVERTED relative to that —
  // "down" maps to swipeUp(), treating the value as the scroll/navigation
  // direction ("scroll DOWN through content = swipe finger UP"). The two axes
  // are therefore inconsistent, and so is up/down against the Kotlin runner,
  // which reads up/down as a finger direction too. This reaches users through
  // `App::swipe_once`. Aligning up/down is a behavior break for existing
  // flows and is an open v2 decision, not a settled contract.
  public enum Direction: String, Sendable {
    case up
    case down
    case left
    case right
  }

  public struct SwipeOnceRequest: Equatable, Sendable {
    public let direction: Direction
    public init(direction: Direction) {
      self.direction = direction
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingDirection
    case invalidDirection(String)
  }

  public static func decode(_ body: Data) throws -> SwipeOnceRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.invalidJSON }
    guard let raw = root["direction"] else { throw DecodeError.missingDirection }
    guard let str = raw as? String else { throw DecodeError.invalidDirection("\(raw)") }
    guard let dir = Direction(rawValue: str) else { throw DecodeError.invalidDirection(str) }
    return SwipeOnceRequest(direction: dir)
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

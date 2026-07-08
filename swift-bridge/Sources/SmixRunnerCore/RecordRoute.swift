import FlyingFox
import Foundation

// v2.0 c2 — POST /record/start, POST /record/stop, GET /record/poll.
// AX-event capture route. Decoupled from XCUITest target — Core owns wire
// shape + JSON encoding; UI test target's EventRecorder produces the
// RecordedEvent stream via `XCAXClient_iOS.handleAccessibilityNotification:
// withPayload:` swizzle.
//
// c1 dig (`.scratch/v2.0-c1-design.md` §3) bet 4 named AX notification
// constants + a `_XCGetWrappedAXUIElementFromNotificationPayload` C
// function for payload decoding. Header verification (maestro cli-2.2.0's
// copied `XCAXClient_iOS.h`) shows the real selector signature is
// `_registerForAXNotification:(int)code error:(NSError**)` /
// `handleAccessibilityNotification:(int)code withPayload:(id)payload`
// — int codes, no named string constants, no payload-decode C function
// in the header set. So RecordedEvent's discriminator stays as the raw
// `int` code + best-effort kind classification done in the UI target;
// payload decoding (selector hint extraction, element-type) is deferred
// to c3 IR layer with all hint fields optional in c2.
public enum RecordRoute {

  public struct StartRequest: Equatable, Sendable {
    public init() {}
  }

  public struct StopRequest: Equatable, Sendable {
    public init() {}
  }

  public struct PollRequest: Equatable, Sendable {
    public init() {}
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
  }

  // Empty body OR `{}` object — both legal. Body parity with BackRoute /
  // HideKeyboardRoute. Forward-compatible for future opts (e.g.
  // filter-by-kind) via additive JSON keys.
  public static func decodeStart(_ body: Data) throws -> StartRequest {
    try requireEmptyOrObjectBody(body)
    return StartRequest()
  }

  public static func decodeStop(_ body: Data) throws -> StopRequest {
    try requireEmptyOrObjectBody(body)
    return StopRequest()
  }

  public static func decodePoll(_ body: Data) throws -> PollRequest {
    try requireEmptyOrObjectBody(body)
    return PollRequest()
  }

  private static func requireEmptyOrObjectBody(_ body: Data) throws {
    if body.isEmpty { return }
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard json is [String: Any] else { throw DecodeError.invalidJSON }
  }

  public static func startSuccess() -> HTTPResponse {
    let body = Data(#"{"ok":true}"#.utf8)
    return envelope(.ok, body)
  }

  public static func stopSuccess(events: [RecordedEvent]) -> HTTPResponse {
    let body = Data(#"{"ok":true,"events":\#(encodeEvents(events))}"#.utf8)
    return envelope(.ok, body)
  }

  public static func pollSuccess(events: [RecordedEvent]) -> HTTPResponse {
    let body = Data(#"{"events":\#(encodeEvents(events))}"#.utf8)
    return envelope(.ok, body)
  }

  public static func badRequest(reason: String) -> HTTPResponse {
    let r = jsonEscape(reason)
    let body = Data(#"{"ok":false,"error":"bad_request","reason":"\#(r)"}"#.utf8)
    return envelope(.badRequest, body)
  }

  // JSONEncoder writes RecordedEvent → JSON array. Stable key order across
  // platforms with `.sortedKeys` so wire bytes stay deterministic for any
  // future snapshot tests.
  public static func encodeEvents(_ events: [RecordedEvent]) -> String {
    let encoder = JSONEncoder()
    encoder.outputFormatting = [.sortedKeys]
    encoder.dateEncodingStrategy = .millisecondsSince1970
    do {
      let data = try encoder.encode(events)
      return String(data: data, encoding: .utf8) ?? "[]"
    } catch {
      return "[]"
    }
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

// Wire-shape Codable. UI target (XCUITest) produces; host TS (runner-client)
// consumes. selectorHints + frame + elementType all optional — the c2 swift
// capture only fills `timestampMs` + `rawCode` reliably; richer fields (kind
// classification, selector hints from `XCUIRecorderUtilities.
// nodeToFindElementForSnapshots:`, element-type, frame) are progressive
// — populated where the payload's `id` type permits without depending on
// header-absent C function `_XCGetWrappedAXUIElementFromNotificationPayload`.
public struct RecordedEvent: Codable, Equatable, Sendable {
  public let timestampMs: Int64
  public let rawCode: Int
  public let kind: String?
  public let location: Point?
  public let endLocation: Point?
  public let durationMs: Int?
  public let text: String?
  public let selectorHints: SelectorHints?
  public let elementType: Int?
  public let frame: Rect?
  public let appBundleId: String?
  public let payloadClassName: String?
  public let payloadDescription: String?

  public init(
    timestampMs: Int64,
    rawCode: Int,
    kind: String? = nil,
    location: Point? = nil,
    endLocation: Point? = nil,
    durationMs: Int? = nil,
    text: String? = nil,
    selectorHints: SelectorHints? = nil,
    elementType: Int? = nil,
    frame: Rect? = nil,
    appBundleId: String? = nil,
    payloadClassName: String? = nil,
    payloadDescription: String? = nil
  ) {
    self.timestampMs = timestampMs
    self.rawCode = rawCode
    self.kind = kind
    self.location = location
    self.endLocation = endLocation
    self.durationMs = durationMs
    self.text = text
    self.selectorHints = selectorHints
    self.elementType = elementType
    self.frame = frame
    self.appBundleId = appBundleId
    self.payloadClassName = payloadClassName
    self.payloadDescription = payloadDescription
  }
}

// 5-field OR-match candidates, mirrors smix `find` selector hint priority
// (id > label > title > placeholder > value). c3 IR layer picks the most
// unique one; c2 capture populates as many as the payload permits.
public struct SelectorHints: Codable, Equatable, Sendable {
  public let identifier: String?
  public let label: String?
  public let title: String?
  public let placeholderValue: String?
  public let valueString: String?

  public init(
    identifier: String? = nil,
    label: String? = nil,
    title: String? = nil,
    placeholderValue: String? = nil,
    valueString: String? = nil
  ) {
    self.identifier = identifier
    self.label = label
    self.title = title
    self.placeholderValue = placeholderValue
    self.valueString = valueString
  }
}

// CGPoint / CGRect not Codable across all Darwin platforms uniformly +
// SmixRunnerCore intentionally avoids importing QuartzCore (Core stays
// pure POCO, free of XCUI/CoreGraphics). Mirror the two shapes as plain
// structs.
public struct Point: Codable, Equatable, Sendable {
  public let x: Double
  public let y: Double
  public init(x: Double, y: Double) { self.x = x; self.y = y }
}

public struct Rect: Codable, Equatable, Sendable {
  public let x: Double
  public let y: Double
  public let width: Double
  public let height: Double
  public init(x: Double, y: Double, width: Double, height: Double) {
    self.x = x; self.y = y; self.width = width; self.height = height
  }
}

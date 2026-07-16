import FlyingFox
import Foundation

// POST /foreground {bundleId:"<id>"} → 200 {ok:<bool>}.
// Mirrors FindRoute / ScrollRoute / TapRoute envelope shape. Runner-side
// handler instantiates XCUIApplication(bundleIdentifier:) and calls
// .activate() (Apple synchronous fire-and-forget API; documented
// idempotent — repeated activate on already-frontmost app is no-op).
// Route owns request decoding + response serialization only.
//
// `bundleId` is required + non-empty (empty string == programmer error;
// driver-layer caller passes a real bundle identifier from SDK). No
// `?include=` query — foreground is an app-level act, not an element-level
// one, so the see-through scope question does not arise; contrast /scroll,
// which resolves an element and therefore does need a scope.
public enum ForegroundRoute {
  public struct ForegroundRequest: Equatable, Sendable {
    public let bundleId: String
    public init(bundleId: String) {
      self.bundleId = bundleId
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingBundleId
    case emptyBundleId
    case wrongType(String)
  }

  public static func decode(_ body: Data) throws -> ForegroundRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else {
      throw DecodeError.wrongType("root not object")
    }
    guard let rawId = root["bundleId"] else { throw DecodeError.missingBundleId }
    guard let bundleId = rawId as? String else {
      throw DecodeError.wrongType("bundleId not string")
    }
    if bundleId.isEmpty {
      throw DecodeError.emptyBundleId
    }
    return ForegroundRequest(bundleId: bundleId)
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
    // Same minimal-escape policy as ScrollRoute / FindRoute (control +
    // quote + backslash). Local copy to keep route modules self-contained.
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

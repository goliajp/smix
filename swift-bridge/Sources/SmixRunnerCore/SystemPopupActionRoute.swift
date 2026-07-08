import FlyingFox
import Foundation

// Act side of the system-popup surface — POST /system-popup-action
// {popupId, buttonId}. Paired with `SystemPopupsRoute` (enumerate).
//
// id derivation mirrors the enumerate side (popup.id ← container
// .identifier, fallback "popup-N" by scan order; button.id ← b.identifier,
// fallback "b-N" by intra-popup index) so enumerate → action round-trips
// without an out-of-band id map. UITests dispatch
// (`SmixRunnerUITests.swift systemPopupActionHandler`) walks the same
// scan order and taps via the `EventSynthesizer` +
// `daemonProxySynthesize` dlsym chain.
//
// This file owns Core-layer wire only: decode the request body and
// encode 200 / 404 / 400 envelopes.
public enum SystemPopupActionRoute {
  public struct Request: Equatable, Sendable {
    public let popupId: String
    public let buttonId: String
    public init(popupId: String, buttonId: String) {
      self.popupId = popupId
      self.buttonId = buttonId
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingPopupId
    case missingButtonId
    case wrongType(String)
  }

  public static func decode(_ body: Data) throws -> Request {
    let json: Any
    do {
      json = try JSONSerialization.jsonObject(with: body, options: [])
    } catch {
      throw DecodeError.invalidJSON
    }
    guard let root = json as? [String: Any] else {
      throw DecodeError.wrongType("root not object")
    }
    guard let rawPopupId = root["popupId"] else { throw DecodeError.missingPopupId }
    guard let popupId = rawPopupId as? String else {
      throw DecodeError.wrongType("popupId not string")
    }
    guard let rawButtonId = root["buttonId"] else { throw DecodeError.missingButtonId }
    guard let buttonId = rawButtonId as? String else {
      throw DecodeError.wrongType("buttonId not string")
    }
    return Request(popupId: popupId, buttonId: buttonId)
  }

  public static func success() -> HTTPResponse {
    return envelope(.ok, Data(#"{"ok":true}"#.utf8))
  }

  public static func notFound(popupId: String, buttonId: String) -> HTTPResponse {
    let p = jsonEscape(popupId)
    let b = jsonEscape(buttonId)
    let body = Data(
      #"{"ok":false,"error":"not_found","popupId":"\#(p)","buttonId":"\#(b)"}"#.utf8)
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

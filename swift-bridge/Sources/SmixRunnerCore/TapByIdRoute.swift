import FlyingFox
import Foundation

// v5.3 c4 — POST /tap-by-id {"id":"<a11y-id>"} → 200 {ok:<bool>}.
//
// Dedicated XCUIElement.tap() path for accessibility-id selectors. Lets the
// SDK opt into the XCTest gesture-recognizer chain when the default
// host-HID-at-coord tap doesn't trigger SwiftUI bindings (notably .sheet /
// .alert / .confirmationDialog / .fullScreenCover dismiss buttons in
// iOS 17+ — see v5.3 c3 (b) findings). Parallel to /tap-at-norm-coord;
// the existing /tap route stays untouched so v1 41-段 baseline is zero-risk.
//
// Wire shape (purposefully minimal):
//   request:  {"id": "v2-modal-sheet-dismiss-btn"}
//   response: 200 {"ok": true}            — element tapped
//             200 {"ok": false}           — element not found
//             400 {"ok": false, "error": "bad_request", "reason": "..."}
public enum TapByIdRoute {
  public struct TapByIdRequest: Equatable, Sendable {
    public let id: String
    public init(id: String) { self.id = id }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingId
    case invalidField(String, String)
  }

  public static func decode(_ body: Data) throws -> TapByIdRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.invalidJSON }
    guard let raw = root["id"] else { throw DecodeError.missingId }
    guard let s = raw as? String else {
      throw DecodeError.invalidField("id", "\(raw)")
    }
    guard !s.isEmpty else {
      throw DecodeError.invalidField("id", "empty")
    }
    return TapByIdRequest(id: s)
  }

  public static func success(ok: Bool) -> HTTPResponse {
    let body = Data(#"{"ok":\#(ok)}"#.utf8)
    return envelope(.ok, body)
  }

  public static func badRequest(reason: String) -> HTTPResponse {
    let escaped = reason.replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
    let body = Data(#"{"ok":false,"error":"bad_request","reason":"\#(escaped)"}"#.utf8)
    return envelope(.badRequest, body)
  }

  private static func envelope(_ status: HTTPStatusCode, _ body: Data) -> HTTPResponse {
    HTTPResponse(
      statusCode: status,
      headers: [.contentType: "application/json"],
      body: body
    )
  }
}

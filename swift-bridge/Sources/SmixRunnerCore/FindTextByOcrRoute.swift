import FlyingFox
import Foundation

// v5.19 c1a — POST /find-text-by-ocr {"text":"...","locales":["en"]}
//   → 200 {"found": bool, "frame": [nx, ny, w, h]?}
//
// L5 sense layer for the a11y-less + i18n initiative (per docs/plan-cold/
// v5.17-v5.22-a11y-i18n-master.md). Apple Vision OCR over the current
// XCUIScreen.main screenshot. Returns the first matching text observation's
// bounding box normalized to [0,1] in UIKit coord space (top-left origin,
// y-down — Vision's native [0,1] is bottom-left, y-up; handler converts).
//
// Locales default to ["en"] when empty/missing. Pass BCP-47 language
// subtags (e.g. "en", "ja", "es"). Apple Vision uses these to pick the
// recognition language model — accuracy lifts ~15-30% when correct
// locale is provided per Apple docs.
//
// Wire shape:
//   request:  {"text": "Submit", "locales": ["en"], "recognition_level": "accurate"}
//             locales / recognition_level optional, defaults shown.
//   response: 200 {"found": true,  "frame": [0.32, 0.85, 0.12, 0.04]}
//             200 {"found": false, "frame": null}
//             400 {"found": false, "error": "bad_request", "reason": "..."}
public enum FindTextByOcrRoute {
  public struct FindTextByOcrRequest: Equatable, Sendable {
    public let text: String
    public let locales: [String]
    /// "accurate" (default, slower + higher accuracy) or "fast".
    public let recognitionLevel: String
    public init(text: String, locales: [String], recognitionLevel: String) {
      self.text = text
      self.locales = locales
      self.recognitionLevel = recognitionLevel
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingText
    case invalidField(String, String)
  }

  public static func decode(_ body: Data) throws -> FindTextByOcrRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.invalidJSON }

    guard let raw = root["text"] else { throw DecodeError.missingText }
    guard let s = raw as? String else {
      throw DecodeError.invalidField("text", "\(raw)")
    }
    guard !s.isEmpty else {
      throw DecodeError.invalidField("text", "empty")
    }

    var locales: [String] = ["en"]
    if let l = root["locales"] {
      guard let arr = l as? [String] else {
        throw DecodeError.invalidField("locales", "\(l)")
      }
      if !arr.isEmpty { locales = arr }
    }

    var level = "accurate"
    if let r = root["recognition_level"] {
      guard let lvl = r as? String else {
        throw DecodeError.invalidField("recognition_level", "\(r)")
      }
      guard lvl == "accurate" || lvl == "fast" else {
        throw DecodeError.invalidField("recognition_level", lvl)
      }
      level = lvl
    }

    return FindTextByOcrRequest(text: s, locales: locales, recognitionLevel: level)
  }

  public static func success(found: Bool, frame: (Double, Double, Double, Double)?)
    -> HTTPResponse
  {
    let body: Data
    if let f = frame {
      let s = #"{"found":\#(found),"frame":[\#(f.0),\#(f.1),\#(f.2),\#(f.3)]}"#
      body = Data(s.utf8)
    } else {
      body = Data(#"{"found":\#(found),"frame":null}"#.utf8)
    }
    return envelope(.ok, body)
  }

  public static func badRequest(reason: String) -> HTTPResponse {
    let escaped = reason.replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
    let body = Data(#"{"found":false,"error":"bad_request","reason":"\#(escaped)"}"#.utf8)
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

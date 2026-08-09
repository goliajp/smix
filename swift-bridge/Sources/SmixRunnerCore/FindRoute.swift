import FlyingFox
import Foundation

// POST /find {selector:{text}} -> {ok:true,found:<bool>}.
//
// `expect.toBeVisible()` for a simple text selector pays a full /tree
// fetch + serialization + SDK-side JS predicate walk. /find lets the
// runner short-circuit with a direct XCUIElement query, returning a
// boolean without snapshotting the whole tree.
//
// Wire shape: `{"selector":{"text":"Foo"}}` request, `{"ok":true,"found":true|false}`
// response. Selector model mirrors KeyboardRoute (single plain-text
// selector); the SDK side only dispatches simple selectors to /find,
// falling back to /tree for complex selectors (inside / below / regex /
// multi-modifier). 404 is not used — element-not-found = `found:false`,
// transport/validation errors use the standard 400/500 envelopes.
public enum FindRoute {
  public struct Selector: Equatable, Sendable {
    public let text: String
    public init(text: String) { self.text = text }
  }

  public struct FindRequest: Equatable, Sendable {
    public let selector: Selector
    /// When true, `found` additionally requires the LIVE element frame
    /// to intersect the app frame ("on screen"), not just `.exists`.
    /// iOS 26.5 + RN Fabric snapshots report drifted in-viewport frames
    /// for below-the-fold elements, so the tree tier's frame∩viewport
    /// proxy can false-green; the live XCUI query re-resolves current
    /// layout and tells the truth. Optional on the wire; absent = false,
    /// i.e. exists-only behaviour.
    public let requireOnScreen: Bool
    public init(selector: Selector, requireOnScreen: Bool = false) {
      self.selector = selector
      self.requireOnScreen = requireOnScreen
    }
  }

  public enum DecodeError: Error, Equatable {
    case invalidJSON
    case missingSelector
    case missingText
    case wrongType(String)
  }

  public static func decode(_ body: Data) throws -> FindRequest {
    let json: Any
    do { json = try JSONSerialization.jsonObject(with: body, options: []) }
    catch { throw DecodeError.invalidJSON }
    guard let root = json as? [String: Any] else { throw DecodeError.wrongType("root not object") }
    guard let sel = root["selector"] else { throw DecodeError.missingSelector }
    guard let selObj = sel as? [String: Any] else { throw DecodeError.wrongType("selector not object") }
    guard let rawText = selObj["text"] else { throw DecodeError.missingText }
    guard let text = rawText as? String else { throw DecodeError.wrongType("selector.text not string") }
    let requireOnScreen = (root["requireOnScreen"] as? Bool) ?? false
    return FindRequest(
      selector: Selector(text: text),
      requireOnScreen: requireOnScreen
    )
  }

  /// What the query saw, for the refusals that explain nothing.
  ///
  /// A flow whose `appId` differed from the runner's `--bundle` got
  /// `found:false` here for every selector, while `/tree` — same
  /// request, same header — returned that app's nodes in full. The
  /// suspicion was that the two routes reach elements differently and
  /// only `/tree`'s snapshot honoured the rebind.
  ///
  /// These fields answered it, and the answer was neither route's
  /// element access: `rebound:false` with and without the header, and
  /// the identical `candidates` count both ways. This route was
  /// registered without `contextGuardedResponse`, so `currentContext`
  /// was never set and `resolveApp()` returned the boot-time app every
  /// time. `/fill` and `/clear` were the same. Fixed 2026-08-09; a
  /// `found:false` carrying nothing is what kept it unknowable for as
  /// long as it did.
  ///
  /// Diagnostic only. Nothing here changes what `found` says.
  public struct Diagnostics: Equatable, Sendable {
    /// `XCUIApplication.State` raw value for the app the query ran
    /// against — 1 notRunning, 2 runningBackgroundSuspended,
    /// 3 runningBackground, 4 runningForeground.
    public let appState: Int
    /// How many elements the query had before the predicate. Zero
    /// separates "the app has no elements here" from "it has them and
    /// none matched", which look identical from `found:false`.
    public let candidates: Int
    /// Whether this request named a bundle other than the one the
    /// runner was started with — the case under suspicion.
    public let rebound: Bool
    public init(appState: Int, candidates: Int, rebound: Bool) {
      self.appState = appState
      self.candidates = candidates
      self.rebound = rebound
    }
  }

  /// `found:true` keeps the two-field shape it has always had — there
  /// is nothing to explain about a query that worked, and a client
  /// parsing the old shape should not meet a new field on the happy
  /// path. A runner that cannot tell passes `nil` and says nothing,
  /// rather than inventing a zero that would read as "not running".
  public static func success(found: Bool, diagnostics: Diagnostics? = nil) -> HTTPResponse {
    let body: Data
    switch diagnostics {
    case let d? where !found:
      body = Data(
        #"{"ok":true,"found":false,"diagnostics":{"appState":\#(d.appState),"candidates":\#(d.candidates),"rebound":\#(d.rebound)}}"#
          .utf8
      )
    default:
      body = Data(#"{"ok":true,"found":\#(found)}"#.utf8)
    }
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

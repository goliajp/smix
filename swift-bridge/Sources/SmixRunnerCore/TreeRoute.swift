import FlyingFox
import Foundation

#if canImport(CoreGraphics)
import CoreGraphics
#endif

// XCUI snapshot → JSON tree serializer (v0.3 C1).
// Pure functions on a POCO (`A11ySnapshotData`) so SmixRunnerCore stays free
// of XCTest/XCUI imports — the UITest target converts XCUIElementSnapshot
// into A11ySnapshotData inside the snapshotHandler closure.
public enum TreeRoute {
  /// POCO mirror of the XCUIElementSnapshot fields C1 consumes.
  /// Sendable so it can flow across the actor boundary back into the server.
  public struct A11ySnapshotData: Sendable {
    public let elementTypeRawValue: UInt
    public let identifier: String
    public let label: String
    // v1.5 c5i-a S2 — `title` + `placeholderValue` 跟 maestro IOSDriver.kt:192-210
    // 同源 attribute. RN `<Text>` 渲染常落 title 而非 label
    // (UIKit `accessibilityTitle` vs `accessibilityLabel` 不同字段).
    // TextInput placeholder 落 placeholderValue (XCUIElementSnapshot 公开
    // API). host-side TS resolveSelector OR-match 从 5 字段拓到 6 候选 (5
    // maestro + 既有 smix node.text 保 backward-compat).
    public let title: String?
    public let placeholderValue: String?
    public let value: String?
    public let frame: CGRect
    public let isEnabled: Bool
    public let isSelected: Bool
    /// v5.1 c1 — keyboard focus (first responder)。iOS 26.5 sim 公开
    /// `XCUIElementSnapshot.dictionaryRepresentation` 把 `hasKeyboardFocus`
    /// key filter 掉(自 iOS 15 起的 Apple regression / deprecation,maestro
    /// #2842 OPEN 也撞同墙)。底层 `XCElementSnapshot._hasKeyboardFocus` ivar
    /// 仍承载真值;UITest 侧 `FocusedIdentifier.find` 走 Foundation KVC
    /// `value(forKey: "hasKeyboardFocus")` 透过 filter 直读,深度优先找到
    /// first responder 的 identifier,作为 focusHint 沿 convertSnapshotDict
    /// 子树下传,匹配节点 set 此字段 true。
    public let hasFocus: Bool
    public let children: [A11ySnapshotData]

    public init(
      elementTypeRawValue: UInt,
      identifier: String,
      label: String,
      value: String?,
      frame: CGRect,
      isEnabled: Bool,
      isSelected: Bool,
      hasFocus: Bool = false,
      children: [A11ySnapshotData],
      title: String? = nil,
      placeholderValue: String? = nil
    ) {
      self.elementTypeRawValue = elementTypeRawValue
      self.identifier = identifier
      self.label = label
      self.title = title
      self.placeholderValue = placeholderValue
      self.value = value
      self.frame = frame
      self.isEnabled = isEnabled
      self.isSelected = isSelected
      self.hasFocus = hasFocus
      self.children = children
    }
  }

  /// XCUI accessibility snapshot tree depth cap.
  ///
  /// Apple's XCUI framework does not itself truncate — `maxDepth` defaults
  /// to `Int32.max`. This cap is smix-imposed to bound JSONSerialization
  /// recursion depth. 200 is empirically sufficient for deep React Native
  /// view hierarchies (80–200 levels) that flatten many layout wrappers.
  public static let MAX_DEPTH: Int = 200

  // MARK: - elementType name table

  /// Mirror of `src/core/role.ts:43-104` XCUIElementType numeric → name.
  /// Apple does not publish a public string description for
  /// XCUIElement.ElementType, so we hand-maintain this table. Unknown
  /// values (and rawValue == 1) map to "other".
  public static func elementTypeName(_ rawValue: UInt) -> String {
    switch rawValue {
    case 0: return "any"
    case 1: return "other"
    case 2: return "application"
    case 3: return "group"
    case 4: return "window"
    case 5: return "sheet"
    case 6: return "drawer"
    case 7: return "alert"
    case 8: return "dialog"
    case 9: return "button"
    case 10: return "radioButton"
    case 11: return "radioGroup"
    case 12: return "checkBox"
    case 13: return "disclosureTriangle"
    case 14: return "popUpButton"
    case 15: return "comboBox"
    case 16: return "menuButton"
    case 17: return "toolbarButton"
    case 18: return "popover"
    case 19: return "keyboard"
    case 20: return "key"
    case 21: return "navigationBar"
    case 22: return "tabBar"
    case 23: return "tabGroup"
    case 24: return "toolbar"
    case 25: return "statusBar"
    case 26: return "table"
    case 27: return "tableRow"
    case 28: return "tableColumn"
    case 29: return "outline"
    case 30: return "outlineRow"
    case 31: return "browser"
    case 32: return "collectionView"
    case 33: return "slider"
    case 34: return "pageIndicator"
    case 35: return "progressIndicator"
    case 36: return "activityIndicator"
    case 37: return "segmentedControl"
    case 38: return "picker"
    case 39: return "pickerWheel"
    case 40: return "switch"
    case 41: return "toggle"
    case 42: return "link"
    case 43: return "image"
    case 44: return "icon"
    case 45: return "searchField"
    case 46: return "scrollView"
    case 47: return "scrollBar"
    case 48: return "staticText"
    case 49: return "textField"
    case 50: return "secureTextField"
    case 51: return "datePicker"
    case 52: return "textView"
    case 53: return "menu"
    case 54: return "menuItem"
    case 55: return "menuBar"
    case 56: return "menuBarItem"
    case 57: return "map"
    case 58: return "webView"
    case 75: return "cell"
    default: return "other"
    }
  }

  // MARK: - serialize

  /// Serialize a snapshot tree into A11yNode-shaped JSON. The serializer is
  /// pure: no global state, no live XCUI queries. `appFrame` is used solely
  /// to compute the `visible` heuristic; `logSink`, when non-nil, receives
  /// one warning line if truncation occurs.
  public static func serialize(
    _ root: A11ySnapshotData,
    appFrame: CGRect,
    logSink: ((String) -> Void)?
  ) -> Data {
    var truncated = false
    let dict = nodeToDict(
      root,
      appFrame: appFrame,
      depth: 0,
      truncated: &truncated,
      logSink: logSink
    )
    if truncated {
      logSink?("tree: truncated at depth \(MAX_DEPTH)")
    }
    // Stable key ordering helps host-side `jq` smoke output diff.
    let opts: JSONSerialization.WritingOptions = [.sortedKeys]
    return (try? JSONSerialization.data(withJSONObject: dict, options: opts)) ?? Data("{}".utf8)
  }

  /// v1.2 C2 — recursive node count. Used by the route handler to populate
  /// the `X-Tree-Node-Count` response header alongside `X-Tree-Size-Bytes`,
  /// giving SDK-side instrumentation a true measure of tree complexity.
  public static func countNodes(_ root: A11ySnapshotData) -> Int {
    return 1 + root.children.reduce(0) { $0 + countNodes($1) }
  }

  private static func nodeToDict(
    _ d: A11ySnapshotData,
    appFrame: CGRect,
    depth: Int,
    truncated: inout Bool,
    logSink: ((String) -> Void)?
  ) -> [String: Any] {
    var out: [String: Any] = [:]
    out["rawType"] = elementTypeName(d.elementTypeRawValue)
    if !d.identifier.isEmpty { out["identifier"] = d.identifier }
    if !d.label.isEmpty { out["label"] = d.label }
    if let t = d.title, !t.isEmpty { out["title"] = t }
    if let p = d.placeholderValue, !p.isEmpty { out["placeholderValue"] = p }
    if let v = d.value, !v.isEmpty { out["value"] = v }
    out["bounds"] = [
      "x": d.frame.origin.x,
      "y": d.frame.origin.y,
      "w": d.frame.size.width,
      "h": d.frame.size.height,
    ]
    out["enabled"] = d.isEnabled
    out["selected"] = d.isSelected
    // v5.1 c1 — `focused` 解冻:POCO `hasFocus` 由 UITest snapshotHandler
    // 通过 KVC walk `_hasKeyboardFocus` ivar + focusHint identifier 匹配
    // 填好。详 POCO 字段 doc 注释。
    out["hasFocus"] = d.hasFocus
    out["visible"] = isVisible(d.frame, appFrame: appFrame)

    if depth >= MAX_DEPTH {
      truncated = true
      out["children"] = [[String: Any]]()
    } else {
      out["children"] = d.children.map {
        nodeToDict($0, appFrame: appFrame, depth: depth + 1, truncated: &truncated, logSink: logSink)
      }
    }
    return out
  }

  /// `visible` heuristic. Snapshots are dead frames so no live `isHittable`
  /// is available. Use frame ∩ appFrame as a cheap (~µs/node) proxy:
  /// empty frames or frames entirely outside the app rect map to false.
  static func isVisible(_ frame: CGRect, appFrame: CGRect) -> Bool {
    guard !frame.isEmpty else { return false }
    let inter = frame.intersection(appFrame)
    return !inter.isNull && !inter.isEmpty
  }

  // MARK: - response builders

  public static func success(_ payload: Data) -> HTTPResponse {
    return envelope(.ok, payload)
  }

  /// v1.2 C2 — `success` variant emitting tree meta as HTTP response
  /// headers. Body is byte-identical to `success(payload)` (no JSON wire
  /// shape change), so legacy SDK consumers ignore the headers and still
  /// parse the A11yNode root. New SDK reads `X-Tree-Size-Bytes` /
  /// `X-Tree-Node-Count` for hot-spot instrumentation.
  public static func successWithMeta(
    _ payload: Data,
    sizeBytes: Int,
    nodeCount: Int
  ) -> HTTPResponse {
    HTTPResponse(
      statusCode: .ok,
      headers: [
        .contentType: "application/json",
        HTTPHeader("X-Tree-Size-Bytes"): String(sizeBytes),
        HTTPHeader("X-Tree-Node-Count"): String(nodeCount),
      ],
      body: payload
    )
  }

  public static func unavailable() -> HTTPResponse {
    let body = Data(#"{"ok":false,"error":"snapshot_unavailable"}"#.utf8)
    return envelope(.internalServerError, body)
  }

  /// v1.0.15 Cluster C D2 — extended snapshot_unavailable envelope
  /// with `reason` (categorized enum) and `hint` (actionable text)
  /// so downstream tooling can steer to the right next step instead
  /// of guessing from a single generic error string. Insight ask #4
  /// in `smix-feedback-2026-07-11-post-native-fix.md`.
  ///
  /// Wire shape:
  /// ```
  /// {"ok":false,"error":"snapshot_unavailable",
  ///  "reason":"alive-but-tree-empty",
  ///  "hint":"Process foreground but no named a11y descendants — ..."}
  /// ```
  ///
  /// Backward-compat: pre-v1.0.15 clients that don't know about
  /// `reason` / `hint` see the same `error` key and same 500 status;
  /// only the body is enriched.
  public static func unavailable(
    reason: AppUnavailableReason,
    hint: String
  ) -> HTTPResponse {
    let escapedHint = hint
      .replacingOccurrences(of: "\\", with: "\\\\")
      .replacingOccurrences(of: "\"", with: "\\\"")
    let body = Data(
      #"{"ok":false,"error":"snapshot_unavailable","reason":"\#(reason.rawValue)","hint":"\#(escapedHint)"}"#
        .utf8)
    return envelope(.internalServerError, body)
  }

  private static func envelope(_ status: HTTPStatusCode, _ body: Data) -> HTTPResponse {
    HTTPResponse(
      statusCode: status,
      headers: [.contentType: "application/json"],
      body: body
    )
  }
}

/// v1.0.15 Cluster C D2 — categorization of why a tree probe failed.
/// Emitted as the `reason` field on `unavailable(reason:hint:)`.
/// String values are stable kebab-case identifiers that consumers can
/// match on without needing an enum decoder — the wire is JSON-string,
/// not a numeric tag.
public enum AppUnavailableReason: String, Sendable {
  /// Process exited between launch and the tree probe. Often paired
  /// with a `.ips` file appearing in
  /// `~/Library/Logs/DiagnosticReports/` within 30 s.
  case crashedDuringInit = "crashed-during-init"
  /// Process alive but a11y tree returned zero descendants with a
  /// non-empty `accessibilityIdentifier`. Usual causes: splash screen
  /// ceremony still running, or the consumer's app hasn't populated
  /// accessibility identifiers on its top-level components.
  case aliveButTreeEmpty = "alive-but-tree-empty"
  /// Tree content hash matches a previous session's snapshot. Runner
  /// may be returning cached content; try `POST /session/renew-activation`.
  case aliveButTreeStale = "alive-but-tree-stale"
  /// XCUITest driver-side query threw or timed out. Restart the
  /// runner (`smix runner cycle`) if this persists.
  case driverDisconnected = "driver-disconnected"
  /// Fallback for the "we know something's wrong but not why" case.
  /// Pre-v1.0.15 semantics.
  case unknown = "unknown"
}

extension AppUnavailableReason {
  /// v1.0.15 Cluster C D2 — actionable text for each reason. Kept
  /// alongside the enum so `TreeRoute.unavailable(reason:)` can
  /// compute a default hint without the caller supplying one.
  public var defaultHint: String {
    switch self {
    case .crashedDuringInit:
      return "Process exited during initialization — look at ~/Library/Logs/DiagnosticReports/ for the .ips file."
    case .aliveButTreeEmpty:
      return "Process foreground but no named a11y descendants — likely splash-screen ceremony still running, or your app's accessibility tree lacks accessibilityIdentifier coverage."
    case .aliveButTreeStale:
      return "Tree hash matches a previous session's snapshot — driver may be returning cached content. Try POST /session/renew-activation."
    case .driverDisconnected:
      return "XCUITest driver query failed — try `smix runner cycle` to restart the runner."
    case .unknown:
      return "Tree probe failed for an uncategorized reason — inspect the runner log."
    }
  }
}

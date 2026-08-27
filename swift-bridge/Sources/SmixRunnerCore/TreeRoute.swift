import FlyingFox
import Foundation

#if canImport(CoreGraphics)
import CoreGraphics
#endif

// XCUI snapshot → JSON tree serializer.
// Pure functions on a POCO (`A11ySnapshotData`) so SmixRunnerCore stays free
// of XCTest/XCUI imports — the UITest target converts XCUIElementSnapshot
// into A11ySnapshotData inside the snapshotHandler closure.
public enum TreeRoute {
  /// POCO mirror of the XCUIElementSnapshot fields the serializer consumes.
  /// Sendable so it can flow across the actor boundary back into the server.
  public struct A11ySnapshotData: Sendable {
    public let elementTypeRawValue: UInt
    public let identifier: String
    public let label: String
    // `title` and `placeholderValue` come from the same XCUIElementSnapshot
    // attributes maestro's IOSDriver reads. RN `<Text>` frequently renders
    // into `title` rather than `label` — UIKit's `accessibilityTitle` and
    // `accessibilityLabel` are distinct fields — and a TextInput's
    // placeholder lands in `placeholderValue`. Both must be on the wire so
    // host-side selector resolution can OR-match across all of them.
    public let title: String?
    public let placeholderValue: String?
    public let value: String?
    public let frame: CGRect
    public let isEnabled: Bool
    public let isSelected: Bool
    /// Keyboard focus (first responder).
    ///
    /// On the iOS 26.5 simulator the public
    /// `XCUIElementSnapshot.dictionaryRepresentation` filters the
    /// `hasKeyboardFocus` key out — an Apple regression/deprecation
    /// present since iOS 15 (maestro #2842 is open against the same
    /// wall). The underlying `XCElementSnapshot._hasKeyboardFocus` ivar
    /// still carries the true value, so the UITest side's
    /// `FocusedIdentifier.find` reads it through Foundation KVC
    /// (`value(forKey: "hasKeyboardFocus")`), bypassing the filter. It
    /// walks depth-first for the first responder's identifier and passes
    /// that down the `convertSnapshotDict` subtree as a focus hint; the
    /// matching node sets this field to true.
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

  /// XCUIElementType numeric → name, as carried on the wire.
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
  /// pure: no global state, no live XCUI queries. Visibility is judged
  /// against the root's own frame; `logSink`, when non-nil, receives
  /// one warning line if truncation occurs.
  public static func serialize(
    _ root: A11ySnapshotData,
    logSink: ((String) -> Void)?
  ) -> Data {
    var truncated = false
    // Judge visibility against the snapshot's OWN root frame, taken from
    // the same live snapshot as the node frames — not a value cached at
    // startup. A portrait-startup cache went stale the moment the app
    // locked to landscape, and every node past the portrait width was
    // wrongly called off-screen (C6c). This mirrors the Rust side's
    // `is_visible_enough(node, tree)`, which already ∩'s `tree.bounds`.
    let rootFrame = root.frame
    let modalPresent = containsModal(root)
    let dict = nodeToDict(
      root,
      rootFrame: rootFrame,
      depth: 0,
      truncated: &truncated,
      logSink: logSink,
      inActionContainer: false,
      modalPresent: modalPresent
    )
    if truncated {
      logSink?("tree: truncated at depth \(MAX_DEPTH)")
    }
    // Stable key ordering helps host-side `jq` smoke output diff.
    let opts: JSONSerialization.WritingOptions = [.sortedKeys]
    return (try? JSONSerialization.data(withJSONObject: dict, options: opts)) ?? Data("{}".utf8)
  }

  /// Recursive node count. Used by the route handler to populate
  /// the `X-Tree-Node-Count` response header alongside `X-Tree-Size-Bytes`,
  /// giving SDK-side instrumentation a true measure of tree complexity.
  public static func countNodes(_ root: A11ySnapshotData) -> Int {
    return 1 + root.children.reduce(0) { $0 + countNodes($1) }
  }

  /// Whether a modal is anywhere in this tree.
  ///
  /// Needed before the walk starts, because a node is unreachable by
  /// virtue of something ELSE being present, and the walk meets nodes
  /// before it meets the alert that covers them.
  ///
  /// Structural rather than per-element: `XCUIElement.isHittable` is a
  /// live query, and under a modal those cost about a second each — the
  /// reason this file reads a snapshot in the first place. What the OS
  /// does is exactly this rule, so asking it once about the shape beats
  /// asking it once per node about the same fact.
  static func containsModal(_ d: A11ySnapshotData) -> Bool {
    let t = elementTypeName(d.elementTypeRawValue)
    if t == "alert" || t == "dialog" || t == "sheet" { return true }
    return d.children.contains(where: containsModal)
  }

  /// Test seam for the node serialiser, which is otherwise private.
  static func nodeToDictForTesting(
    _ d: A11ySnapshotData, rootFrame: CGRect, truncated: inout Bool
  ) -> [String: Any] {
    nodeToDict(
      d, rootFrame: rootFrame, depth: 0, truncated: &truncated,
      logSink: nil, inActionContainer: false, modalPresent: containsModal(d))
  }

  private static func nodeToDict(
    _ d: A11ySnapshotData,
    rootFrame: CGRect,
    depth: Int,
    truncated: inout Bool,
    logSink: ((String) -> Void)?,
    inActionContainer: Bool = false,
    modalPresent: Bool = false
  ) -> [String: Any] {
    var out: [String: Any] = [:]
    let rawRawType = elementTypeName(d.elementTypeRawValue)
    // iOS 26.5 XCUITest exposes UIAlertController / .confirmationDialog
    // action buttons as `.other` (rawValue 1) or `.staticText` (rawValue
    // 48) instead of `.button` (rawValue 9). A `tapOn: { role: button,
    // name: 'Reload' }` selector therefore finds nothing there: the
    // resolver walks nodes matching `rawType == "button"` and iOS 26
    // alert buttons no longer report as such.
    //
    // Enrich at the perception layer instead. When we're inside an
    // action-container ancestor (alert / dialog / sheet), any descendant
    // that has a non-empty label AND is currently `other` or
    // `staticText` gets promoted to `button` on the wire. This preserves
    // `role: button` semantics across iOS versions without requiring
    // per-consumer patches. Nested containers (a sheet inside an alert)
    // don't loop — we only lift, never demote.
    let hasLabel = !d.label.isEmpty || (d.title.map { !$0.isEmpty } ?? false)
    let promotable = inActionContainer && hasLabel
      && (rawRawType == "other" || rawRawType == "staticText")
    let rawType = promotable ? "button" : rawRawType
    out["rawType"] = rawType
    // Always emit the raw elementType number. Consumers debugging a
    // degraded a11y tree (RN Fabric on iOS 26.5 is the motivating case)
    // can then distinguish "iOS types this as a button (9) but
    // identifier / label are empty" (a bridge issue on the app side —
    // likely an RN → UIAccessibility drop) from "iOS types this as other
    // (1)" (a plain custom-view wrapper that never had a probe-friendly
    // type). Client-side triage: `elementTypeRaw != 1 && identifier ==
    // "" && label == ""` is the signal to check the app's accessibility
    // bridge, not smix.
    out["elementTypeRaw"] = d.elementTypeRawValue
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
    // The POCO's `hasFocus` is filled in by the UITest snapshotHandler,
    // which KVC-walks the `_hasKeyboardFocus` ivar and matches the
    // focusHint identifier. See the POCO field's doc comment.
    out["hasFocus"] = d.hasFocus
    out["visible"] = isVisible(d.frame, rootFrame)
    // Whether a touch aimed here would reach it.
    //
    // Only stated when a modal is up: with nothing covering the screen the
    // question has no interesting answer, and saying `true` everywhere
    // would turn a field that means "asked and no" into one that means
    // "asked" — the reader downstream distinguishes absence from a no, and
    // filling the absence with noise costs it that.
    //
    // A consumer measured the defect this closes: with a SwiftUI alert
    // open, the button behind it is still in the tree, a tap at it is
    // swallowed by the presentation, and smix exited 0.
    if modalPresent {
      out["hittable"] = inActionContainer
    }

    // Mark child recursion "in action container" once we hit an alert /
    // dialog / sheet at any depth. Use the ORIGINAL
    // rawType (rawRawType) not the promoted one — a promoted button
    // isn't itself an action container.
    let childInActionContainer = inActionContainer
      || rawRawType == "alert"
      || rawRawType == "dialog"
      || rawRawType == "sheet"

    if depth >= MAX_DEPTH {
      truncated = true
      out["children"] = [[String: Any]]()
    } else {
      out["children"] = d.children.map {
        nodeToDict(
          $0,
          rootFrame: rootFrame,
          depth: depth + 1,
          truncated: &truncated,
          logSink: logSink,
          inActionContainer: childInActionContainer,
          modalPresent: modalPresent
        )
      }
    }
    return out
  }

  /// `visible` heuristic. Snapshots are dead frames so no live `isHittable`
  /// is available. Use frame ∩ the snapshot's own root frame as a cheap
  /// (~µs/node) proxy: empty frames map to false; a node entirely outside
  /// the root maps to false. When the root frame is itself empty/null (a
  /// synthesized all-windows root can be a zero or union rect), pass
  /// conservatively rather than hide everything — mirrors the Rust
  /// `is_visible_enough` "unknown root → conservative pass".
  static func isVisible(_ frame: CGRect, _ rootFrame: CGRect) -> Bool {
    guard !frame.isEmpty else { return false }
    guard !rootFrame.isNull && !rootFrame.isEmpty else { return true }
    let inter = frame.intersection(rootFrame)
    return !inter.isNull && !inter.isEmpty
  }

  // MARK: - response builders

  public static func success(_ payload: Data) -> HTTPResponse {
    return envelope(.ok, payload)
  }

  /// `success` variant emitting tree meta as HTTP response headers. Body
  /// is byte-identical to `success(payload)` (no JSON wire shape
  /// change), so SDK consumers that don't know the headers still parse
  /// the A11yNode root. `X-Tree-Size-Bytes` / `X-Tree-Node-Count` drive
  /// hot-spot instrumentation.
  ///
  /// Consumers hitting batch snapshot drift need a signal for whether
  /// the runner is keeping up. Two further additive headers surface it,
  /// again without changing the JSON body wire shape:
  /// - `X-Tree-Snapshot-Refresh-Count` — cumulative /tree successful
  ///   serves since runner boot. Consumers can subtract the value
  ///   between calls to know how many refreshes happened; if the
  ///   sequence stalls while /tree is being polled, XCUITest is
  ///   returning cached snapshots.
  /// - `X-Tree-Snapshot-Wall-Ms` — how long THIS `snapshotHandler`
  ///   invocation took end-to-end. Trending upward across a batch =
  ///   XCUITest bogging down; the underlying a11y tree is under
  ///   sustained pressure (RN 0.86 Fabric on iOS 26.5 under a
  ///   whole-suite sweep hits this).
  public static func successWithMeta(
    _ payload: Data,
    sizeBytes: Int,
    nodeCount: Int,
    snapshotRefreshCount: UInt64 = 0,
    snapshotWallMs: UInt64 = 0
  ) -> HTTPResponse {
    HTTPResponse(
      statusCode: .ok,
      headers: [
        .contentType: "application/json",
        HTTPHeader("X-Tree-Size-Bytes"): String(sizeBytes),
        HTTPHeader("X-Tree-Node-Count"): String(nodeCount),
        HTTPHeader("X-Tree-Snapshot-Refresh-Count"): String(snapshotRefreshCount),
        HTTPHeader("X-Tree-Snapshot-Wall-Ms"): String(snapshotWallMs),
      ],
      body: payload
    )
  }

  public static func unavailable() -> HTTPResponse {
    let body = Data(#"{"ok":false,"error":"snapshot_unavailable"}"#.utf8)
    return envelope(.internalServerError, body)
  }

  /// Extended snapshot_unavailable envelope with `reason` (categorized
  /// enum) and `hint` (actionable text) so downstream tooling can steer
  /// to the right next step instead of guessing from a single generic
  /// error string.
  ///
  /// Wire shape:
  /// ```
  /// {"ok":false,"error":"snapshot_unavailable",
  ///  "reason":"alive-but-tree-empty",
  ///  "hint":"Process foreground but no named a11y descendants — ..."}
  /// ```
  ///
  /// Backward-compat: clients that don't know about `reason` / `hint`
  /// see the same `error` key and same 500 status; only the body is
  /// enriched.
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

/// Categorization of why a tree probe failed.
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
  /// The app is not running. Somebody terminated it, or it was
  /// reinstalled out from under the runner.
  ///
  /// This used to be reported as `crashed-during-init`, which sent the
  /// reader to look for a crash report that does not exist — the two
  /// are the same `XCUIApplication.state` and different situations.
  /// Splitting them costs nothing and saves the search.
  case notRunning = "not-running"
  /// Fallback for the "we know something's wrong but not why" case.
  case unknown = "unknown"
}

extension AppUnavailableReason {
  /// Actionable text for each reason. Kept
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
    case .notRunning:
      return "The app is not running — it was terminated, or reinstalled out from under the runner. Launch it again (`smix sim launch <device> <bundle-id>`, or `smix runner cycle` to rebind), then retry. If nothing terminated it deliberately, it exited on its own: look in ~/Library/Logs/DiagnosticReports/ on the host for a recent .ips."
    case .unknown:
      return "Tree probe failed for an uncategorized reason — inspect the runner log."
    }
  }
}

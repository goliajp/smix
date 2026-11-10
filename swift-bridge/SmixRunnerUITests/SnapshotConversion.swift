import SmixRunnerCore
import XCTest

/// On the iOS 26.5 sim, `XCUIElementSnapshot.dictionaryRepresentation`
/// filters out the `hasKeyboardFocus` key (an Apple regression /
/// deprecation dating to iOS 15; maestro #2842 hits the same wall and is
/// still open). The underlying `XCElementSnapshot._hasKeyboardFocus` ivar
/// still carries the true value, and Foundation KVC
/// `value(forKey: "hasKeyboardFocus")` reads the ivar directly, straight
/// through the filter. This walks the snapshot subtree depth-first and
/// returns the identifier of the first node that hits.
///
/// Private-symbol compliance: private symbols must never be hard-linked —
/// they are reached only via dlsym or Foundation KVC. This is a pure
/// Foundation KVC selector chain: no hard-linked private symbols, no
/// dlsym'd private constants, no `_XCT_*` private selectors. It relies
/// only on Apple's public `value(forKey:)` entry point plus the naming
/// stability of a private ivar (Apple has kept the same
/// `_hasKeyboardFocus` name across iOS 17 / 18 / 26; if that ever
/// changes, keyboard-focus detection breaks wholesale).
///
/// Verified behaviour: fill input-email → focused identifier =
/// input-email; fill input-password → focused identifier =
/// input-password. `_focused_` is read without re-tapping, so this is a
/// genuine first-responder read rather than an echo of our own tap.
enum FocusedIdentifier {
  /// Return the identifier of the first descendant of `snap` whose
  /// `_hasKeyboardFocus` ivar is true, or nil if none is focused —
  /// typically because the keyboard is not up, focus is on a
  /// non-typable element, or the app has just cold-launched.
  static func find(in snap: XCUIElementSnapshot) -> String? {
    var hit: String? = nil
    func walk(_ node: AnyObject) {
      if hit != nil { return }
      if (node.value(forKey: "hasKeyboardFocus") as? Bool) == true {
        let id = (node.value(forKey: "identifier") as? String) ?? ""
        if !id.isEmpty { hit = id }
        return
      }
      if let kids = node.value(forKey: "children") as? [AnyObject] {
        for k in kids {
          walk(k)
          if hit != nil { return }
        }
      }
    }
    walk(snap as AnyObject)
    return hit
  }
}

/// Bridge XCUIElementSnapshot (XCUI / XCTest type) → A11ySnapshotData POCO
/// (SmixRunnerCore type, no XCUI dependency). Maintains the invariant that
/// SmixRunnerCore never imports XCTest/XCUI.
///
/// Uses the `dictionaryRepresentation` path (the same one maestro
/// `cli-2.2.0` takes via
/// `AXElement(_ dict: [XCUIElement.AttributeName: Any])`, which parses the
/// a11y server's raw attribute dict).
///
/// The public `s.children` Swift API is NOT equivalent: it returns an
/// Apple-curated subset. Child elements that are accessibility-rendered
/// but filtered out by the Swift API — an RN drawer item, for instance —
/// simply vanish from the `.children` array. The raw dict's `children` key
/// contains ALL a11y server children.
///
/// `rootIdentifierOverride` is applied only at the top level call (children
/// recurse with nil) and only when the snapshot's own identifier is empty.
/// This compensates for `XCUIApplication.snapshot()` returning a root with
/// an empty identifier even though the caller knows the bundle id.
///
/// `focusHint` is the live first responder's identifier, computed once by
/// snapshotHandler via `FocusedIdentifier.find`. It is threaded down the
/// subtree, and the matching POCO node gets `hasFocus=true`. `nil` ⇒ no
/// node is focused (false across the whole tree; typically the keyboard is
/// not up).
func convertSnapshot(
  _ s: XCUIElementSnapshot,
  rootIdentifierOverride: String? = nil,
  focusHint: String? = nil
) -> TreeRoute.A11ySnapshotData {
  return convertSnapshotDict(
    s.dictionaryRepresentation,
    rootIdentifierOverride: rootIdentifierOverride,
    focusHint: focusHint
  )
}

/// Apple a11y server raw dict → A11ySnapshotData. The dict comes from
/// `XCUIElementSnapshot.dictionaryRepresentation`; its key type
/// `XCUIElement.AttributeName` is a public Swift type. Field extraction
/// mirrors maestro's `AXElement.swift`.
func convertSnapshotDict(
  _ dict: [XCUIElement.AttributeName: Any],
  rootIdentifierOverride: String? = nil,
  focusHint: String? = nil
) -> TreeRoute.A11ySnapshotData {
  func valueFor(_ name: String) -> Any? {
    dict[XCUIElement.AttributeName(rawValue: name)]
  }
  let label = (valueFor("label") as? String) ?? ""
  let elementTypeRaw = (valueFor("elementType") as? Int) ?? 1  // 1 = "other"
  let rawIdentifier = (valueFor("identifier") as? String) ?? ""
  let identifier: String = rawIdentifier.isEmpty ? (rootIdentifierOverride ?? "") : rawIdentifier
  let titleStr: String? = (valueFor("title") as? String).flatMap { $0.isEmpty ? nil : $0 }
  let placeholderStr: String? = (valueFor("placeholderValue") as? String).flatMap { $0.isEmpty ? nil : $0 }
  let valueStr: String? = (valueFor("value") as? String).flatMap { $0.isEmpty ? nil : $0 }
  let enabled = (valueFor("enabled") as? Bool) ?? true
  let selected = (valueFor("selected") as? Bool) ?? false
  let hasFocus = smixHasFocus(valueFor("hasFocus"), identifier: identifier, hint: focusHint)
  let frame: CGRect
  if let frameDict = valueFor("frame") as? [String: Double] {
    frame = CGRect(
      x: frameDict["X"] ?? 0,
      y: frameDict["Y"] ?? 0,
      width: frameDict["Width"] ?? 0,
      height: frameDict["Height"] ?? 0
    )
  } else if let cgRect = valueFor("frame") as? CGRect {
    frame = cgRect
  } else {
    frame = .zero
  }
  let kids: [TreeRoute.A11ySnapshotData]
  if let childDicts = valueFor("children") as? [[XCUIElement.AttributeName: Any]] {
    kids = childDicts.map { convertSnapshotDict($0, focusHint: focusHint) }
  } else {
    kids = []
  }
  return TreeRoute.A11ySnapshotData(
    elementTypeRawValue: UInt(elementTypeRaw),
    identifier: identifier,
    label: label,
    value: valueStr,
    frame: frame,
    isEnabled: enabled,
    isSelected: selected,
    hasFocus: hasFocus,
    children: kids,
    title: titleStr,
    placeholderValue: placeholderStr
  )
}

/// See-through tree for `GET /tree?include=all-windows`.
///
/// Root cause: `app.snapshot()` is rooted at a single app element, and any
/// opaque native modal makes iOS accessibility mask the content beneath it
/// out of that snapshot — yet XCUITest's own lower flat enumeration can
/// still reach every element. Measured in one case: `app.snapshot()`
/// returned a 96-node overlay while a direct `label ==` query still hit
/// 2550 elements underneath it.
///
/// Strategy (defense in depth — trust no single point not to mask):
///   1. Per-window: snapshot each `app.windows` element. A modal often
///      occupies a window of its own, and sibling windows still expose
///      the underlying content.
///   2. Flat fallback: `app.descendants(.any)
///      .allElementsBoundByAccessibilityElement` — snapshot every element
///      XCUITest can enumerate. This is the only layer that reaches
///      masked content present in NEITHER the app snapshot NOR the
///      per-window snapshots.
/// Both paths merge into the children of ONE synthetic application root
/// and go through the UNCHANGED `convertSnapshot` / `TreeRoute.serialize`,
/// so the serialized shape does not change — the wire contract holds. The
/// synthetic root carries `bundleId` so a host smoke check of
/// `.identifier == bundle` still passes.
///
/// Duplicates are deliberately NOT removed: a superset tree is the goal.
/// The SDK-side resolver does its own DFS collection, and the driver only
/// asks whether a content marker is reachable — a question a superset
/// answers soundly.
func buildAllWindowsSnapshot(
  app: XCUIApplication,
  bundleId: String,
  appFrame: CGRect
) -> SmixRunnerServer.SnapshotResult? {
  var children: [TreeRoute.A11ySnapshotData] = []

  let windows = app.windows.allElementsBoundByIndex
  for window in windows {
    guard window.exists else { continue }
    if let snap = try? window.snapshot() {
      children.append(convertSnapshot(snap, focusHint: FocusedIdentifier.find(in: snap)))
    }
  }

  // Flat fallback: every accessibility element XCUITest can enumerate.
  // Each is snapshotted individually so masked-but-enumerable content
  // (the AX-reachable content underneath any opaque modal) is captured as
  // leaf nodes even when no window snapshot surfaced it.
  //
  // Perf cap. Measured: enumerating the RN expo-dev-menu's 50+ native
  // elements costs ~1-2 s per `el.snapshot()`, totalling 60-120 s, which
  // blows both the SDK's drainPopups hard cap and FlyingFox's 15 s socket
  // timeout. Hence structural limits: maxElements=80 + budgetMs=8s,
  // breaking out when either is hit.
  //
  // Bailing out is safe: the superset tree still contains the window
  // snapshot and plain app snapshot main paths, which keep content
  // markers reachable. 80 elements is ample for an RN dev-mode main
  // screen, a SpringBoard system alert, or an ordinary modal; only a
  // dev-menu or heavy native overlay hits the cap, and those are already
  // captured by the windows snapshot above. Skipping the flat fallback
  // there loses no marker reachability — only redundant leaf nodes — so
  // the "superset tree" behavioural contract still holds.
  let flat = app.descendants(matching: .any).allElementsBoundByAccessibilityElement
  let flatStart = Date()
  let flatMaxElements = 80
  let flatBudgetSeconds: TimeInterval = 8.0
  var flatProcessed = 0
  for el in flat {
    if flatProcessed >= flatMaxElements { break }
    if Date().timeIntervalSince(flatStart) >= flatBudgetSeconds { break }
    flatProcessed += 1
    guard el.exists else { continue }
    if let snap = try? el.snapshot() {
      children.append(convertSnapshot(snap, focusHint: FocusedIdentifier.find(in: snap)))
    }
  }

  // SpringBoard alert / sheet / dialog
  // out-of-process windows. A `simctl openurl` confirm alert lives in
  // com.apple.springboard, not the runner-bound app — neither `app.windows`
  // nor `app.descendants` reach it. Merge active SpringBoard popup
  // snapshots into the synthetic root so `/tree?include=all-windows`
  // exposes the same nodes that `/find?include=all-windows` and
  // `/tap?include=all-windows` will need.
  for s in collectSpringBoardWindows() {
    children.append(s)
  }

  // Always also include the plain app snapshot (it is a subset on the
  // happy path; under masking it is the dev-menu overlay — harmless as
  // an extra child, and guarantees the all-windows tree is a strict
  // superset of the legacy tree).
  if let appSnap = try? app.snapshot() {
    children.append(
      convertSnapshot(appSnap, focusHint: FocusedIdentifier.find(in: appSnap))
        .inInterfaceShape(landscape: smixInterfaceIsLandscape(app)))
  }

  let synthetic = TreeRoute.A11ySnapshotData(
    elementTypeRawValue: 2, // application
    identifier: SmixRunnerServer.currentContext.bundleId ?? bundleId,
    label: "",
    value: nil,
    frame: appFrame,
    isEnabled: true,
    isSelected: false,
    children: children
  )
  return (root: synthetic, appFrame: appFrame)
}

func collectSpringBoardWindows() -> [TreeRoute.A11ySnapshotData] {
  var out: [TreeRoute.A11ySnapshotData] = []
  let springboard = smixSystemUI()
  let alerts = springboard.alerts.allElementsBoundByAccessibilityElement
  for el in alerts {
    guard el.exists else { continue }
    if let snap = try? el.snapshot() {
      out.append(convertSnapshot(snap, focusHint: FocusedIdentifier.find(in: snap)))
    }
  }
  let sheets = springboard.sheets.allElementsBoundByAccessibilityElement
  for el in sheets {
    guard el.exists else { continue }
    if let snap = try? el.snapshot() {
      out.append(convertSnapshot(snap, focusHint: FocusedIdentifier.find(in: snap)))
    }
  }
  let dialogs = springboard.dialogs.allElementsBoundByAccessibilityElement
  for el in dialogs {
    guard el.exists else { continue }
    if let snap = try? el.snapshot() {
      out.append(convertSnapshot(snap, focusHint: FocusedIdentifier.find(in: snap)))
    }
  }
  return out
}

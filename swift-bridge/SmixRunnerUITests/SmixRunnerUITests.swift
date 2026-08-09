import XCTest
import SmixRunnerCore
import ObjectiveC.runtime
import Vision
import UIKit

// The Swift swizzler is only the maxDepth fallback entry point. The modal
// overlay (`snapshotKeyHonorModalViews=0`) is installed permanently by the
// ObjC `SmixA11ySwizzle.m` `+load`, which runs at the dyld stage — earlier
// than the XCTest framework. This mirrors maestro `cli-2.2.0`, which runs
// the same two mechanisms in parallel:
//   - ObjC `+load` `XCAXClient_iOS+FBSnapshotReqParams.m` (modal overlay)
//   - Swift `AXClientSwizzler.swift` (maxDepth fallback, fires only on the
//     ViewHierarchyHandler IllegalArgumentError path; never on the happy path)
//
// The IMP-direct call below is what avoids an iOS 26.5 `unrecognized
// selector` crash. Naming the literal `Standin.self` is not enough on its
// own: dispatching through ObjC msgSend still means calling a Standin
// selector on an XCAXClient_iOS instance, which the runtime rejects.
// `class_getMethodImplementation(Standin.self, swizzledSel)` +
// `unsafeBitCast` calls the IMP directly over the C ABI, bypassing msgSend
// and its selector/class registration check entirely.

// Interactive-probe config.
//
// Runner reads `SMIX_INTERACTIVE_PROBE_JSON` at boot; consumer sets it
// via `.smix/config.yaml interactiveProbe: { minIdentifierCount, ignore }`
// which the CLI JSON-encodes and forwards as
// `TEST_RUNNER_SMIX_INTERACTIVE_PROBE_JSON` (Xcode strips the
// `TEST_RUNNER_` prefix; runner sees `SMIX_INTERACTIVE_PROBE_JSON`).
//
// When missing / malformed, falls back to bundled defaults:
// `minIdentifierCount: 3`, `ignore: [SplashScreenLogo]`.
// "SplashScreenLogo" is the generic Expo splash-screen a11y id —
// present in every Expo app during the pre-JS-mount window, never
// evidence of interactivity.
//
// The target app's own bundle id is ALWAYS ignored dynamically at probe
// time rather than being listed here: every app's root node carries
// identifier == bundleId, so counting it toward minIdentifierCount is a
// semantic bug, not something to configure per app.
struct InteractiveProbeConfig {
  let minIdentifierCount: Int
  let ignore: Set<String>

  static let bundledDefault = InteractiveProbeConfig(
    minIdentifierCount: 3,
    ignore: ["SplashScreenLogo"]
  )

  static func fromEnv() -> InteractiveProbeConfig {
    guard let raw = ProcessInfo.processInfo.environment["SMIX_INTERACTIVE_PROBE_JSON"],
          !raw.isEmpty,
          let data = raw.data(using: .utf8)
    else {
      return bundledDefault
    }
    struct Wire: Decodable {
      let minIdentifierCount: Int?
      let ignore: [String]?
    }
    guard let parsed = try? JSONDecoder().decode(Wire.self, from: data) else {
      return bundledDefault
    }
    return InteractiveProbeConfig(
      minIdentifierCount: max(1, parsed.minIdentifierCount ?? 3),
      ignore: Set(parsed.ignore ?? ["SplashScreenLogo"])
    )
  }
}

private var _overwriteDefaultParameters: [String: Int] = [:]

private final class AXClientStandin: NSObject {
  // IMP-direct lookup. Must use the literal `AXClientStandin.self`, NOT
  // `type(of: self)`: on a swizzled call `self` is an XCAXClient_iOS
  // instance, and the Standin selector does not exist on that class.
  // `class_getMethodImplementation` yields the IMP, and `unsafeBitCast`
  // calls it over the C ABI, skipping ObjC msgSend's check that the
  // selector is registered on `self.class`.
  func originalDefaultParameters() -> NSDictionary {
    let selector = NSSelectorFromString("defaultParameters")
    let swizzledSelector = #selector(swizzledDefaultParameters)
    let imp = class_getMethodImplementation(AXClientStandin.self, swizzledSelector)
    typealias DefaultParametersIMP = @convention(c) (NSObject, Selector) -> NSDictionary
    let method = unsafeBitCast(imp, to: DefaultParametersIMP.self)
    return method(self, selector)
  }

  @objc func swizzledDefaultParameters() -> NSDictionary {
    let defaults = originalDefaultParameters().mutableCopy() as! NSMutableDictionary
    for (k, v) in _overwriteDefaultParameters { defaults[k] = v }
    return defaults
  }
}

enum AXClientSwizzler {
  // Force Standin into the ObjC runtime table early, so that when the
  // lazy `setupOnce` fires, `class_getMethodImplementation(Standin.self,
  // ...)` cannot miss the lookup.
  fileprivate static let proxy = AXClientStandin()

  /// maxDepth fallback injection point, in the ViewHierarchyHandler style
  /// (same as maestro's `AXClientSwizzler.swift`). First access to the
  /// setter triggers the lazy `setupOnce`. Never fires on the happy path —
  /// the modal overlay is already installed permanently by the ObjC `+load`.
  static var overwriteDefaultParameters: [String: Int] {
    get { _overwriteDefaultParameters }
    set { _ = setupOnce; _overwriteDefaultParameters = newValue }
  }

  private static let setupOnce: Void = {
    guard let target = NSClassFromString("XCAXClient_iOS") else {
      FileHandle.standardError.write(
        Data("smix-runner: AXClientSwizzler: XCAXClient_iOS not found\n".utf8))
      return
    }
    let origSel = NSSelectorFromString("defaultParameters")
    let replaceSel = #selector(AXClientStandin.swizzledDefaultParameters)
    guard
      let origMethod = class_getInstanceMethod(target, origSel),
      let replaceMethod = class_getInstanceMethod(AXClientStandin.self, replaceSel)
    else {
      FileHandle.standardError.write(
        Data("smix-runner: AXClientSwizzler: method lookup failed\n".utf8))
      return
    }
    method_exchangeImplementations(origMethod, replaceMethod)
    FileHandle.standardError.write(
      Data("smix-runner: AXClientSwizzler: maxDepth-fallback swizzle installed\n".utf8))
  }()
}

// Fast keyboard path via the XCTest private daemon proxy.
// `XCTRunnerDaemonSession.sharedSession.daemonProxy` exposes the
// `_XCT_sendString:maximumFrequency:completion:` selector, which submits a
// string to the on-device test daemon and types it into the currently
// focused element at the requested typing frequency. This is 10-100×
// faster than `XCUIElement.typeText` (which forces a separate XCUITest
// query + isHittable + per-char keyboard event roundtrip), because the
// daemon talks directly to the IOHIDEvent layer inside the sim.
//
// Cross-tool reference: maestro uses the same selector at
// `typingFrequency=10`; smix defaults to 200, since (a) the daemon
// throttles internally if events overflow, and (b) our flow is
// non-redactable QA scripting (`shouldRedact: false`). 200 was verified
// against a complex bench (3 iter × 30 step + login-tap +
// tap-text-selector) with no regression.
//
// This path patches no Apple binary and injects into no other process:
// the daemon proxy is XCTest's own private API, already loaded inside
// our test target.
private final class DaemonKeyboard: @unchecked Sendable {
  static let shared = DaemonKeyboard()
  private let proxy: NSObject?

  private init() {
    guard let clazz = NSClassFromString("XCTRunnerDaemonSession") else {
      self.proxy = nil; return
    }
    let sharedSel = NSSelectorFromString("sharedSession")
    let imp = clazz.method(for: sharedSel)
    typealias SharedFn = @convention(c) (AnyClass, Selector) -> NSObject
    let session = unsafeBitCast(imp, to: SharedFn.self)(clazz, sharedSel)
    self.proxy = session.perform(NSSelectorFromString("daemonProxy"))?
      .takeUnretainedValue() as? NSObject
  }

  /// Type `text` via the test daemon at up to `typingFrequency` chars/sec.
  /// Throws on missing proxy / daemon error.
  func sendString(_ text: String, typingFrequency: Int = 100) async throws {
    guard let proxy = self.proxy else {
      throw NSError(
        domain: "DaemonKeyboard", code: 1,
        userInfo: [NSLocalizedDescriptionKey: "XCTRunnerDaemonSession daemonProxy unavailable"])
    }
    let selector = NSSelectorFromString("_XCT_sendString:maximumFrequency:completion:")
    let imp = proxy.method(for: selector)
    typealias Method = @convention(c) (
      NSObject, Selector, NSString, Int, @escaping (Error?) -> Void
    ) -> Void
    let method = unsafeBitCast(imp, to: Method.self)
    return try await withCheckedThrowingContinuation { continuation in
      method(proxy, selector, text as NSString, typingFrequency) { error in
        if let error = error {
          continuation.resume(throwing: error)
        } else {
          continuation.resume(returning: ())
        }
      }
    }
  }
}

// Cache of the currently-focused keyboard field's text length so
// the /clear handler can skip the snapshot-triggering `focused.value` read
// on the hot path. State is mutated only by handler closures (synchronous
// after the action returns) so contention is rare; NSLock keeps it sound
// across the async boundaries. Invalidation is conservative:
//   fill(text)       → length += text.count   (typeText APPENDS to existing)
//   pressKey('delete') → length = max(0, length-1)
//   pressKey(other)  → length = nil (return/tab/escape may change focus)
//   clear            → length = 0 (after deletes are sent)
//   tap (any)        → length = nil (navigation may dismiss/change field)
// Read at clear time: if length is nil OR selector isn't `_focused_`, the
// clear handler falls back to the snapshot read path. So an invalidated
// cache is always safe — at worst we lose the optimization.
private final class KeyboardCache: @unchecked Sendable {
  static let shared = KeyboardCache()
  private let lock = NSLock()
  private var _length: Int? = nil

  var length: Int? {
    lock.lock(); defer { lock.unlock() }
    return _length
  }

  func appendFill(_ text: String) {
    lock.lock(); defer { lock.unlock() }
    _length = (_length ?? 0) + text.count
  }

  func recordPressKey(_ key: String) {
    lock.lock(); defer { lock.unlock() }
    if key == "delete" {
      if let v = _length { _length = max(0, v - 1) }
    } else {
      // return / tab / escape / space / unknown may change focus
      _length = nil
    }
  }

  func recordClear() {
    lock.lock(); defer { lock.unlock() }
    _length = 0
  }

  func invalidate() {
    lock.lock(); defer { lock.unlock() }
    _length = nil
  }
}

// Runner-lifetime resilience, layered.
//
// Root cause: when a resolved element vanishes mid-interaction, XCUITest's
// engine fails the running test ("Failed to get matching snapshot: No
// matches found …"). It surfaces two ways, and which one lands first
// cannot be decided a priori — that unknown is exactly why the defense is
// layered rather than betting on one: (a) a raw ObjC exception from
// `_XCTFailureHandler`, and/or (b) an `XCTIssue` recorded via
// `XCTestCase.record(_:)` that fails the test.
//
//   Trampoline (load-bearing): `SmixRunCatching` (ObjC @try/@catch,
//        UITest target only) converts (a) into a Swift Error so the
//        handler maps it to its EXISTING wire shape — `.notFound` (tap),
//        `nil` (snapshot → 500 snapshot_unavailable), `false` (find).
//        Wire shape unchanged.
//   Record backstop: `record(_:)` is overridden. A handler sets
//        `inHandlerSpan` only around its XCUITest calls; while set, a
//        recorded issue is written to stderr in an AI-readable form and
//        `super.record` is NOT called, so the issue does not fail
//        `test_runForever`. OUTSIDE that span (real setUp / launch
//        failures) `super.record` runs unchanged — genuine startup
//        failures still fail loudly.
//
// `HandlerSpanFlag` is process-wide (the runner serves one request at a
// time on the FlyingFox actor; the flag is set/cleared synchronously
// around each handler's XCUITest span). NSLock keeps it sound across the
// async route boundary.
private final class HandlerSpanFlag: @unchecked Sendable {
  static let shared = HandlerSpanFlag()
  private let lock = NSLock()
  private var _inSpan = false

  var inSpan: Bool {
    lock.lock(); defer { lock.unlock() }
    return _inSpan
  }
  func enter() { lock.lock(); _inSpan = true; lock.unlock() }
  func leave() { lock.lock(); _inSpan = false; lock.unlock() }
}

/// Run `body` inside the ObjC exception trampoline AND the record-backstop
/// handler span. Returns `nil` when the XCUITest engine failed (vanished element
/// → caught NSException); the caller maps `nil` to its existing
/// element-not-found wire shape. On success returns the body's value.
/// The thrown NSException reason is written to stderr (AI-readable).
///
/// `SmixRunCatching` marshals `body` onto the main thread before running
/// it. This is required, not incidental. FlyingFox route closures run
/// off-main on the cooperative pool; an unwrapped `element.tap()`
/// works because XCUITest internally marshals the touch dispatch, but
/// the ObjC `@try/@catch` frame disrupts that marshaling and trips
/// `XCUIElementBaseEventTarget.m:391` (`Must be called on the main
/// thread`). Running the whole guarded XCUITest span on main restores
/// correct dispatch while keeping the NSException → Error trampoline.
private func smixGuarded<T>(_ label: String, _ body: () -> T) -> T? {
  HandlerSpanFlag.shared.enter()
  defer { HandlerSpanFlag.shared.leave() }
  var result: T?
  var caught: NSError?
  let ok = SmixRunCatching({ result = body() }, &caught)
  if !ok {
    let msg = caught?.localizedDescription ?? "unknown XCUITest failure"
    FileHandle.standardError.write(
      Data("smix-runner: guarded(\(label)) caught: \(msg)\n".utf8))
    return nil
  }
  return result
}

// Element-resolution source selector (`?include=all-windows`).
//
// When a native modal overlay sits in front of the bound app, iOS
// accessibility MASKS the content beneath it out of the single-app element
// tree that `app.descendants(matching: .any).matching(predicate)` walks,
// so /find and /tap return 404. That masked content is still reachable at
// XCUITest's lower flat-enumeration layer
// (`descendants(.any).allElementsBoundByAccessibilityElement` plus
// per-window descendants) — which is exactly the element set
// `buildAllWindowsSnapshot` collects. This resolver enumerates that
// see-through set and returns the first element satisfying
// `label == text OR identifier == text`, giving /find and /tap the same
// reach through an overlay that `/tree?include=all-windows` already has.
//
// nil scope (no `?include=`) ⇒ the caller keeps its default
// `app.descendants(.any)` query. The SDK runner-client posts /find & /tap
// WITHOUT a query, so every existing flow resolves through that path.
//
// Why a flat probe and not `.matching(predicate)` over the see-through
// set: an XCUIElementQuery's `.matching` re-runs against the live single
// app-element tree (the very tree the modal masks). The see-through
// reach only exists at the per-element `allElementsBoundByAccessibility
// Element` / per-window-`descendants` enumeration layer, so we must
// enumerate THERE and test each element's `label` / `identifier`
// in-process. Each `.label` / `.identifier` read is wrapped by the
// caller's `smixGuarded` span (the main-thread ObjC trampoline) so a
// vanished element during enumeration maps to the existing not-found wire
// shape, never a runner kill.
/// `selector` rather than a bare string: the flat enumeration has to
/// apply the same form-to-match rule as the NSPredicate path, or the
/// all-windows scope would answer a different question than the default
/// one for the same request.
private func firstSeeThroughMatch(
  app: XCUIApplication, selector: RouteSelector
) -> XCUIElement? {
  func matches(_ el: XCUIElement) -> Bool {
    // `.exists` is required before `.label`/`.identifier`; a stale handle
    // from a prior enumeration frame can otherwise throw. Mirrors the
    // `guard el.exists` in buildAllWindowsSnapshot.
    guard el.exists else { return false }
    switch selector {
    case .text(let v): return el.label == v || el.identifier == v
    case .id(let v): return el.identifier == v
    case .label(let v): return el.label == v
    }
  }
  // 1. Flat fallback first: the layer that reaches content masked out of
  //    BOTH the app snapshot AND the per-window snapshots (content behind
  //    any opaque native modal overlay). Same enumeration as
  //    buildAllWindowsSnapshot's `flat`.
  let flat = app.descendants(matching: .any)
    .allElementsBoundByAccessibilityElement
  for el in flat where matches(el) { return el }
  // 2. Per-window descendants: a modal often lives in its own window;
  //    sibling windows still expose the underlying content tree. Same
  //    windows enumeration as buildAllWindowsSnapshot.
  for window in app.windows.allElementsBoundByIndex where window.exists {
    let wdesc = window.descendants(matching: .any)
      .allElementsBoundByAccessibilityElement
    for el in wdesc where matches(el) { return el }
  }
  // 3. SpringBoard: an `xcrun simctl openurl` confirm alert ("Open in…?")
  //    lives in com.apple.springboard, NOT the runner-bound app — neither
  //    `app.descendants` nor `app.windows` reach it. The see-through path
  //    here lets `/tap?include=all-windows` resolve and hit a SpringBoard
  //    alert button — acting on a system popup is a core flat capability,
  //    not driver-specific. Same `label == text OR identifier == text` matcher; same
  //    `smixGuarded` span at the caller guards mid-enumeration vanish.
  let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
  let sbDesc = springboard.descendants(matching: .any)
    .allElementsBoundByAccessibilityElement
  for el in sbDesc where matches(el) { return el }
  return nil
}

// SpringBoard popup enumeration. Popup sense is a core flat capability,
// not driver-specific; this helper returns the active SpringBoard alerts /
// sheets / dialogs as `SystemPopupsRoute.Popup` POCOs the route serializer
// wraps in the `{"ok":true,"popups":[…]}` envelope. Role classification is
// locale-invariant: only `userTestingAttributes` predicates are consulted
// (the same Apple-internal AX test attribute the setUp interruption
// monitor uses to find the cancel button). Dangerous flag =
// role==destructive OR a DANGEROUS_LABEL_TOKENS hit — the token list is
// a structural fallback for buttons that expose no destructive role.
private let DANGEROUS_LABEL_TOKENS: [String] = [
  "delete", "erase", "remove", "wipe", "destroy", "format", "reset",
]

private func isLabelDangerousSpringBoard(_ label: String) -> Bool {
  let lo = label.lowercased()
  for t in DANGEROUS_LABEL_TOKENS {
    if lo.contains(t) { return true }
  }
  return false
}

// /system-popups core sense — three tiers of enumeration, locale-invariant
// and runtime-agnostic:
//
//   1. SpringBoard's (`com.apple.springboard`) `.alerts/.sheets/.dialogs/
//      .popovers` — any system-level modal (custom scheme confirm, local
//      network, tracking transparency, system warnings, etc).
//   2. The bound app's `.alerts/.sheets/.dialogs/.popovers` — the four
//      iOS standard modal types (XCUIElement.ElementType raw 7/5/8/18).
//      Any standard app-side modal container matches here.
//   3. The bound app's NON-main windows (`app.windows[i≥1]`) — the
//      fallback for non-standard modals. Any self-drawn overlay, custom
//      portal, or third-party toast container appears as
//      XCUIElement.ElementType.window (raw 9) at binding index ≥ 1; the
//      main app window is always index 0 and is never a popup. This is
//      XCUITest's own structural (locale-invariant) main-vs-overlay
//      discriminator.
//
// The `source` field carries each popup's process bundleId so the upper
// layer can tell system popups from in-app ones.
//
// `outcomeHint` is auto-marked ONLY for the SpringBoard scheme-confirm
// pattern, via the locale-invariant userTestingAttributes path. Every
// other popup leaves it null and hands the decision back to the upper
// layer. Label-keyed patterns are forbidden here: recognizing a new
// pattern structurally must go through userTestingAttributes, an SF
// Symbol identifier, or container topology — never a literal English
// label, which would silently break under any other locale.
private func collectSystemPopups(
  app: XCUIApplication,
  bundleId: String,
  includeAllWindows: Bool = false
) -> [SystemPopupsRoute.Popup] {
  let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
  var out: [SystemPopupsRoute.Popup] = []
  // Enumeration caps for buttons/staticTexts inside a bound-app window.
  // Heavy native overlays like the RN expo-dev-menu carry 50+ buttons, and
  // `.allElementsBoundByAccessibilityElement` plus per-element `.exists` /
  // `.label` / `.identifier` calls each cost ~1-2 s — 60-120 s in total,
  // which blows past both the SDK's drainPopups budget and FlyingFox's 15 s
  // socket timeout, hanging the whole chain. Hence a structural cap: at
  // most 40 buttons + 20 staticTexts per container. Alerts, sheets,
  // dialogs and popovers are naturally far below this; only a dev-menu or
  // window gets truncated. Decide policy still works after truncation:
  // the default affirmative-by-hint is findable within buttons[0..N], and
  // RN dismiss-by-xmark is the same shape — the xmark is usually
  // buttons[0] or close to it. Truncation only drops redundant button
  // metadata, never a signal popup-handling needs.
  let consumeMaxButtons = 40
  let consumeMaxStaticTexts = 20
  // Overall wall-clock budget across containers. consume() has its own
  // local 5 s buttons / 3 s texts budgets, but those lack a cross-container
  // deadline: several containers (springboard.alerts/sheets +
  // app.alerts/sheets/dialogs/popovers) can sum past FlyingFox's 15 s
  // socket timeout and hang the runner's main thread. 11 s leaves margin
  // under that 15 s. Every container checks the deadline on entry to
  // consume; once it passes, remaining containers are skipped and a
  // partial result is returned.
  let collectSystemPopupsBudgetSeconds: TimeInterval = 11.0
  let collectStart = Date()
  func budgetExceeded() -> Bool {
    Date().timeIntervalSince(collectStart) >= collectSystemPopupsBudgetSeconds
  }
  func consume(
    _ container: XCUIElement, fallbackType: String, source: String
  ) {
    if budgetExceeded() { return }
    // Snapshot-based enumeration: one `container.snapshot()` grabs the
    // whole subtree, and everything after that is a pure in-memory walk of
    // dictionaryRepresentation, reading each button/staticText's
    // label/identifier/userTestingAttributes plus the container's own
    // elementType/identifier.
    //
    // Root cause this avoids: while a modal alert is up, EVERY live
    // XCUITest element-property access (.label / .exists / .identifier /
    // .elementType / a predicate query) triggers a full a11y snapshot
    // costing ~1.2 s. A per-element walk therefore costs N × 1.2 s for one
    // container, blows past FlyingFox's 15 s socket timeout, and hangs the
    // runner's main thread. Reading only matched elements is not enough
    // either — a live `.label` / `.exists` on each match is still a
    // per-element access.
    //
    // So: exactly ONE live access per container. The snapshot doubles as
    // the existence check — a failing `try?` means the container vanished,
    // so return — which is why there is no `guard container.exists` here.
    // Everything else is memory, making per-container cost independent of
    // the element count N.
    guard let snap = try? container.snapshot() else { return }
    let dict = snap.dictionaryRepresentation
    func dictVal(_ k: String) -> Any? { dict[XCUIElement.AttributeName(rawValue: k)] }

    var buttonNodes: [(label: String, id: String, frame: CGRect)] = []
    var textLabels: [String] = []
    collectPopupNodes(dict, buttons: &buttonNodes, texts: &textLabels)

    // Role classification. `userTestingAttributes` is NOT present in
    // `XCUIElementSnapshot.dictionaryRepresentation` (measured: it never
    // appears), so it can only be obtained via a live predicate query.
    //
    // Done for SpringBoard native alerts only: their a11y tree is simple,
    // so 2 container-level attribute queries are fast — as opposed to 2N
    // per-button ones — and do not trip the hang. In-app RN modals are
    // SKIPPED: RN buttons never set userTestingAttributes (a live query
    // against them always classifies "default" anyway, so there is nothing
    // to gain), and their slow live-element access is exactly the hang
    // root cause the single-snapshot path above eliminates.
    var cancelLabels: Set<String> = []
    var destructiveLabels: Set<String> = []
    if source == "com.apple.springboard" {
      let cancelPred = NSPredicate(format: "userTestingAttributes CONTAINS %@", "cancel-button")
      let destructivePred = NSPredicate(format: "userTestingAttributes CONTAINS %@", "destructive")
      cancelLabels = Set(
        container.buttons.matching(cancelPred).allElementsBoundByAccessibilityElement
          .compactMap { $0.exists ? $0.label : nil })
      destructiveLabels = Set(
        container.buttons.matching(destructivePred).allElementsBoundByAccessibilityElement
          .compactMap { $0.exists ? $0.label : nil })
    }

    var buttons: [SystemPopupsRoute.PopupButton] = []
    var cancelCount = 0
    var defaultCount = 0
    for (i, bn) in buttonNodes.enumerated() {
      if buttons.count >= consumeMaxButtons { break }
      let label = bn.label
      let role = classifyPopupButtonRole(
        label: label, cancelLabels: cancelLabels, destructiveLabels: destructiveLabels)
      if role == "cancel" { cancelCount += 1 }
      if role == "default" { defaultCount += 1 }
      let id = bn.id.isEmpty ? "b-\(i)" : bn.id
      let dangerous = role == "destructive" || isLabelDangerousSpringBoard(label)
      buttons.append(
        SystemPopupsRoute.PopupButton(
          id: id, label: label, role: role,
          dangerous: dangerous,
          outcomeHint: nil
        )
      )
    }
    // Read the container's elementType from the dict root, avoiding a live
    // `container.elementType` access. XCUIElement.ElementType raw values:
    // alert=7, sheet=5, dialog=8, popover=18.
    let etRaw = (dictVal("elementType") as? Int) ?? 0
    let isAlert = etRaw == 7
    // Scheme-confirm pattern: a SpringBoard 2-button alert with exactly
    // one cancel + at least one default. Mark the affirmative button(s)
    // with outcomeHint = "scheme-confirm-affirm" — locale-invariant
    // decision aid. The hint is SpringBoard-specific (the system
    // "Open in 'X'?" deeplink confirm); in-app modals do not get an
    // auto-hint — the upper layer decides which button to press.
    let isSpringboard = source == "com.apple.springboard"
    let schemeConfirm =
      isSpringboard && isAlert && cancelCount == 1 && defaultCount >= 1
      && buttons.count == 2
    if schemeConfirm {
      buttons = buttons.map { b in
        if b.role == "default" {
          return SystemPopupsRoute.PopupButton(
            id: b.id, label: b.label, role: b.role,
            dangerous: b.dangerous,
            outcomeHint: "scheme-confirm-affirm"
          )
        }
        return b
      }
    }
    var title = ""
    var body = ""
    for (i, s) in textLabels.prefix(consumeMaxStaticTexts).enumerated() {
      if i == 0 { title = s } else {
        if !body.isEmpty { body += "\n" }
        body += s
      }
    }
    let rawId = (dictVal("identifier") as? String) ?? ""
    let id = rawId.isEmpty ? "popup-\(out.count)" : rawId
    let typeName: String
    switch etRaw {
    case 7: typeName = "alert"
    case 5: typeName = "sheet"
    case 8: typeName = "dialog"
    case 18: typeName = "popover"
    default: typeName = fallbackType
    }
    out.append(
      SystemPopupsRoute.Popup(
        id: id,
        type: typeName,
        source: source,
        title: title,
        body: body,
        buttons: buttons
      )
    )
  }
  // SpringBoard side first (preserves emitted-order expectation: system
  // popup before in-app popup when both are present).
  for el in springboard.alerts.allElementsBoundByAccessibilityElement {
    consume(el, fallbackType: "alert", source: "com.apple.springboard")
  }
  for el in springboard.sheets.allElementsBoundByAccessibilityElement {
    consume(el, fallbackType: "sheet", source: "com.apple.springboard")
  }
  // Bound-app side: the structural iOS modal types
  // (alert / sheet / dialog / popover). The bound-app's own modal
  // surfaces here; the legacy SpringBoard-only path missed them.
  for el in app.alerts.allElementsBoundByAccessibilityElement {
    consume(el, fallbackType: "alert", source: bundleId)
  }
  for el in app.sheets.allElementsBoundByAccessibilityElement {
    consume(el, fallbackType: "sheet", source: bundleId)
  }
  for el in app.dialogs.allElementsBoundByAccessibilityElement {
    consume(el, fallbackType: "dialog", source: bundleId)
  }
  for el in app.popovers.allElementsBoundByAccessibilityElement {
    consume(el, fallbackType: "popover", source: bundleId)
  }
  // Phase 2: bound-app NON-main window. Enumerate every bound-app window
  // and emit indices ≥ 1 as window-typed popups. Index 0 is the primary
  // app window (root) — never a popup. `consume`'s `typeName` logic already
  // falls through to `fallbackType` for elementType not in
  // alert/.sheet/.dialog/.popover, so passing `fallbackType: "window"`
  // routes the entry to the correct wire type.
  //
  // Keyboard window filter: when an app (RN / UIKit / SwiftUI) presents
  // the on-screen keyboard, UIKit hosts it in a separate UIWindow that
  // lives at `app.windows[i ≥ 1]` alongside any genuine overlay. The
  // keyboard is NOT a popup — it has no dismiss affordance, no
  // outcomeHint surface, and the upper layer (typing flows / fill) wants
  // it on-screen, not drained. `XCUIElement.keyboards` (elementType
  // .keyboard, raw 70) is the locale-invariant structural probe: if a
  // non-main window's subtree contains a keyboard element, skip it.
  // The probe is structural (no label / locale dependency); it stays
  // correct across iOS versions, languages, and input modes (standard,
  // emoji, dictation, third-party keyboards).
  // Enumerating the bound app's non-main windows happens only when
  // `includeAllWindows` is explicitly opted into; it is skipped by default.
  // Heavy native overlays (RN expo-dev-menu, self-drawn modals) live in
  // this branch, and skipping it by default saves the ~60-120 s cost of a
  // full enumeration — inside consume(), each button + staticText
  // `.exists` / `.label` / `.identifier` call runs ~1-2 s, and a dev-menu
  // has 50+ elements.
  //
  // The SDK's default `app.system.drainPopups()` passes no include ⇒ scope
  // nil ⇒ includeAllWindows=false ⇒ the cheap SpringBoard scheme-confirm
  // path. Only an explicit
  // `app.system.drainPopups({include:'all-windows'})` opts into the
  // expensive sense.
  if includeAllWindows {
    let allWindows = app.windows.allElementsBoundByIndex
    for (i, w) in allWindows.enumerated() {
      if i == 0 { continue }
      if w.keyboards.firstMatch.exists { continue }
      consume(w, fallbackType: "window", source: bundleId)
    }
  }
  return out
}

// The act side of system-popup handling. Walks the same SpringBoard /
// bound-app scan
// order `collectSystemPopups` uses (alerts → sheets → bound-app
// alerts/sheets/dialogs/popovers), matches popup by id derivation
// `popup-N` (global out-count) ⇋ container.identifier, then matches
// button by id derivation `b-N` (intra-popup index) ⇋ b.identifier.
// On match, taps via SmixEventRecord + SmixRunnerDaemonProxy.shared
// .synthesize (the daemonProxySynthesize dlsym chain — private symbols
// are reached via dlsym, never hard-linked) at the matched button's
// frame center. Returns .found
// when synthesize completes; .notFound when popup id, button id, or
// synthesize dispatch failed. Does NOT walk includeAllWindows windows —
// the action route is element-level by id, scope was decided at the
// sense path.
private func findAndTapSystemPopupButton(
  app: XCUIApplication, bundleId: String,
  popupId: String, buttonId: String
) -> SmixRunnerServer.SystemPopupActionOutcome {
  let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
  var popupIdx = 0
  let scanSets: [XCUIElementQuery] = [
    springboard.alerts,
    springboard.sheets,
    app.alerts,
    app.sheets,
    app.dialogs,
    app.popovers,
  ]
  for query in scanSets {
    for el in query.allElementsBoundByAccessibilityElement {
      // Snapshot-based: one `el.snapshot()` grabs the container subtree,
      // then identifier + button (id, frame) are read from
      // dictionaryRepresentation in memory, eliminating per-element live
      // `.exists` / `.identifier` / `.frame` access. While a modal is up
      // each live access triggers a ~1.2 s full a11y snapshot, and
      // per-element accumulation hits FlyingFox's 15 s socket timeout —
      // the same hang already fixed in collectSystemPopups.
      //
      // `snapshot()` doubles as the existence check: a failing `try?`
      // means the container vanished. popupIdx still increments in that
      // case, to keep the popup-N id derivation aligned with enumerate's
      // out-count.
      guard let snap = try? el.snapshot() else {
        popupIdx += 1
        continue
      }
      let dict = snap.dictionaryRepresentation
      let rawId = (dict[XCUIElement.AttributeName(rawValue: "identifier")] as? String) ?? ""
      let elId = rawId.isEmpty ? "popup-\(popupIdx)" : rawId
      if elId == popupId {
        var buttonNodes: [(label: String, id: String, frame: CGRect)] = []
        var textLabels: [String] = []
        collectPopupNodes(dict, buttons: &buttonNodes, texts: &textLabels)
        for (i, bn) in buttonNodes.enumerated() {
          let bId = bn.id.isEmpty ? "b-\(i)" : bn.id
          if bId == buttonId {
            let center = CGPoint(x: bn.frame.midX, y: bn.frame.midY)
            guard let record = SmixEventRecord(orientation: .portrait),
                  record.addPointerTouchEvent(at: center) else {
              return .notFound
            }
            let sema = DispatchSemaphore(value: 0)
            var ok = false
            Task {
              do {
                try await SmixRunnerDaemonProxy.shared.synthesize(record: record)
                ok = true
              } catch {
                FileHandle.standardError.write(
                  Data(
                    "smix-runner: system-popup-action daemonProxy error: \(error)\n"
                      .utf8))
              }
              sema.signal()
            }
            sema.wait()
            return ok ? .found : .notFound
          }
        }
        return .notFound
      }
      popupIdx += 1
    }
  }
  return .notFound
}


// Long-running XCTestCase that hosts the in-simulator HTTP server.
// Pattern: Maestro re-building-the-ios-driver.
// The test method `test_runForever` intentionally never returns — `runForever()`
// blocks on FlyingFox `server.run()` until xcodebuild cancels the runner.
final class SmixRunnerUITests: XCTestCase {
  /// The predicate for a selector form.
  ///
  /// `text` keeps the historical `label OR identifier` shape byte for
  /// byte: it is what callers have been getting, and narrowing it here
  /// would break flows that (knowingly or not) rely on a text selector
  /// landing on an identifier. The new forms are exact, which is the
  /// point — an id selector that matched a label would be the same
  /// confusion in the other direction.
  static func predicate(for selector: RouteSelector) -> NSPredicate {
    switch selector {
    case .text(let v):
      return NSPredicate(format: "label == %@ OR identifier == %@", v, v)
    case .id(let v):
      return NSPredicate(format: "identifier == %@", v)
    case .label(let v):
      return NSPredicate(format: "label == %@", v)
    }
  }

  // `continueAfterFailure = true` (which is what maestro sets) was tried
  // and measured as a regression (2/5). XCTest's internal swallow has side
  // effects that destabilize it in combination with the `record(_:)`
  // override below. Leaving it unset keeps XCTest's default behaviour.
  //
  // Issue-recording backstop. While a handler's XCUITest
  // span is active a recorded XCTIssue (the second way a vanished element
  // surfaces — `_XCTFailureHandler` may `record(_:)` instead of `@throw`)
  // is swallowed (logged AI-readable, not propagated) so one failed
  // interaction does not terminate `test_runForever` = does not restart
  // the runner. Outside the span the default behaviour is preserved: a
  // genuine setUp/launch failure still fails the test loudly.
  override func record(_ issue: XCTIssue) {
    if HandlerSpanFlag.shared.inSpan {
      let desc = issue.compactDescription
      FileHandle.standardError.write(
        Data("smix-runner: swallowed in-handler XCTIssue: \(desc)\n".utf8))
      // Sniff "Application X is not running" issues and
      // mark the bundle-id in the app-alive cache. Task-local read
      // works here because record(_:) is called synchronously from
      // the XCUITest span running under the current request's task
      // scope. Parse conservatively (bundle-id-shaped substring
      // between "Application " and " is not running"); anything not
      // matching is a no-op.
      if let range = desc.range(of: "Application "),
         let end = desc.range(of: " is not running"),
         range.upperBound < end.lowerBound,
         let cache = SmixRunnerServer.currentAppAliveCache {
        let bundleId = String(desc[range.upperBound..<end.lowerBound])
          .trimmingCharacters(in: .whitespaces)
        if !bundleId.isEmpty && bundleId.contains(".") {
          Task {
            await cache.markDead(bundleId: bundleId)
            await cache.noteReprobeAttempted()
            // Adaptive re-probe. During the 20 s dead
            // window, poll XCUIApplication.state every 3 s. On the
            // first observation of `.runningForeground` (or any
            // "not notRunning" state), invalidate the cache early
            // so a slow-bootstrap app doesn't sit blocked for the
            // full 20 s while it's actually alive again.
            //
            // Bounded to 6 iterations (18 s) — matches the cache
            // window minus one probe interval for slack. If the
            // app is still `.notRunning` after 6 probes the cache
            // expires naturally.
            //
            // Every observable exit path advances a
            // counter (noteReprobeInvalidatedEarly / Succeeded /
            // ExhaustedWindow) so /diagnostic/dump can prove whether
            // this fired, hit, or timed out. A stderr log line alone
            // cannot: a grep returning zero does not distinguish these
            // states from a dropped log line.
            var iterationsRun = 0
            for _ in 0..<6 {
              try? await Task.sleep(nanoseconds: 3_000_000_000)
              iterationsRun += 1
              // Cache may have been invalidated by /session/open or
              // /sim/launch during the wait; check before probing.
              if !(await cache.isSuppressed(bundleId: bundleId)) {
                await cache.noteReprobeInvalidatedEarly()
                return
              }
              let target = XCUIApplication(bundleIdentifier: bundleId)
              let state = await SmixRunnerServer.onMain { target.state }
              if state != .notRunning && state != .unknown {
                await cache.markAlive(bundleId: bundleId)
                await cache.noteReprobeSucceeded()
                FileHandle.standardError.write(
                  Data("smix-runner: app-alive cache re-probe hit \(bundleId) state=\(state.rawValue); early invalidate\n".utf8))
                return
              }
            }
            if iterationsRun == 6 {
              await cache.noteReprobeExhaustedWindow()
            }
          }
        }
      }
      return
    }
    super.record(issue)
  }

  // There is deliberately no UI-interruption monitor here. An earlier one
  // tapped the affirmative button of every two-button SpringBoard alert
  // exposing a `cancel-button` AX attribute. That bypassed the
  // `/system-popups` decision layer: deciding which button to press
  // belongs to the upper layer, never to the driver.
  //
  // e2e does not depend on such a monitor: `popup_decided =
  // rn-window-dismiss` runs entirely through the core `/system-popups`
  // sense path plus the `/tap` act path, with the AI / e2e author making
  // the decision. Settings and acceptance flows never trigger a
  // two-button alert, so the monitor was inert there anyway. Its absence
  // keeps the runner's sense / decide / act layering free of a hidden
  // actor.

  func test_runForever() async throws {
    // The modal overlay is installed permanently by the ObjC
    // `SmixA11ySwizzle.m` `+load`, which runs at the dyld stage — earlier
    // than this test entry point. The Swift `AXClientSwizzler` is only the
    // maxDepth fallback entry (the ViewHierarchyHandler
    // IllegalArgumentError path) and is never actively installed on the
    // happy path. This mirrors maestro `cli-2.2.0`, which runs the same
    // two mechanisms in parallel.

    // EventRecorder swizzle gated by `SMIX_RECORD_ENABLED` env.
    // SMIX_ prefixed env vars are forwarded by Xcode via the TEST_RUNNER_
    // prefix convention (same forwarding mechanism as TEST_RUNNER_SMIX_RUNNER_
    // LAUNCH_ARGS). Default off → install never runs → v1.x stable runner
    // path is byte-identical (no extra method_setImplementation, no extra
    // memory residence, no buffer accumulation).
    let recordEnabled = ProcessInfo.processInfo.environment["SMIX_RECORD_ENABLED"] == "1"
    if recordEnabled {
      EventRecorder.shared.installSwizzle()
    }

    // C2 target app: Settings (Calculator not preinstalled on iOS 26 sim runtime).
    // R4.d (audit #6) — launchArguments resolved from
    // `SMIX_RUNNER_LAUNCH_ARGS` env (JSON array literal) via
    // LaunchArgsResolver. Empty/missing/malformed env → empty array
    // = app's natural locale. e2e wanting English locale injects
    // `TEST_RUNNER_SMIX_RUNNER_LAUNCH_ARGS=
    // ["-AppleLanguages","(en)","-AppleLocale","en_US"]` (same
    // TEST_RUNNER_ prefix convention as TargetBundleResolver /
    // LaunchModeResolver). Pre-R4 hard-coded the en literal which
    // hijacked locale unconditionally.
    let bundleId = TargetBundleResolver.resolve(env: ProcessInfo.processInfo.environment)
    let app = XCUIApplication(bundleIdentifier: bundleId)
    app.launchArguments = LaunchArgsResolver.resolve(
      env: ProcessInfo.processInfo.environment
    )
    // `test_runForever` is an XCTest test method; XCTest
    // guarantees test methods execute on the main queue, so `.launch()`
    // / `.activate()` here are on-main by construction (no explicit
    // MainActor hop needed). Any XCUITest mutation added inside this
    // setup block inherits the same guarantee. Handlers below use
    // `resolveApp()` (async) with SmixRunnerServer.onMain instead.
    switch LaunchModeResolver.resolve(env: ProcessInfo.processInfo.environment) {
    case .launch: app.launch()
    case .activate: app.activate()
    }

    // Capture app.frame ONCE per runner lifetime. XCUIApplication
    // .frame internally triggers a light snapshot (~50-150ms); for our default
    // tap dispatch (Settings is portrait-only, window doesn't resize across taps)
    // the value is invariant. Cache here so each tapHandler invocation only pays
    // for element resolution + element.frame read, not the global app.frame read.
    //
    // Cache is now per-bundle so a client that switches the target app
    // mid-session via `App-Bundle-Id` header doesn't get the wrong-app frame.
    // The default (boot-time) entry is seeded here; subsequent per-bundle
    // entries are populated lazily inside handlers.
    var cachedAppFrames: [String: CGRect] = [bundleId: app.frame]
    let cachedAppFramesLock = NSLock()

    // Per-request target-app resolver. Reads the task-local
    // `RequestContext` set by `SmixRunnerServer.contextGuardedResponse`
    // from the `App-Bundle-Id` / `App-Activate` request headers. When
    // the client asks to talk to a bundle id different from the
    // runner-bound default, this returns a freshly-bound
    // XCUIApplication for that bundle; when `App-Activate: true` is
    // present, the target is activated first to recover from stale
    // foreground states.
    //
    // Absent headers → default `RequestContext` → returns the cached
    // boot-time `app`.
    //
    // The resolver is `async` because `.activate()` is main-actor
    // isolated on iOS 26+ SDKs; calling it off-main raises
    // `NSInternalInconsistencyException`.
    //
    // Per-bundle-id activation rate limit. Without it,
    // every request carrying `App-Activate: true` triggers an
    // `.activate()` call. Long-running gates (visual / perf
    // regression, ~340 s of continuous requests) accumulated ~1000+
    // activate calls, exhausting XCTest process arbitration on
    // iOS 26.5+ and crashing `test_runForever()` mid-run. The
    // rate-limit records the last activate timestamp per bundle-id
    // and skips redundant activations within a 5 s window. Recovery
    // semantics preserved: after 5 s of silence a subsequent
    // `App-Activate: true` re-issues the call, so a foreground steal
    // by SpringBoard is auto-recovered within the same window.
    var lastActivatedAt: [String: Date] = [:]
    let lastActivatedAtLock = NSLock()
    let activationCooldown: TimeInterval = 5.0
    let maybeActivate: @Sendable (String, XCUIApplication) async -> Bool = { bundleKey, target in
      let now = Date()
      let shouldActivate: Bool = {
        lastActivatedAtLock.lock()
        defer { lastActivatedAtLock.unlock() }
        if let last = lastActivatedAt[bundleKey], now.timeIntervalSince(last) < activationCooldown {
          return false
        }
        lastActivatedAt[bundleKey] = now
        return true
      }()
      if shouldActivate {
        await SmixRunnerServer.onMain {
          target.activate()
        }
      }
      return shouldActivate
    }
    // Session table. Sessions are opened via `POST /session/open`
    // and carry a client-supplied opaque id; every subsequent request that
    // includes the `Session-Id` header short-circuits `resolveApp()` here
    // to return the session's cached XCUIApplication binding — no
    // `.activate()` on any per-request path, ever, unless the client
    // explicitly calls `POST /session/renew-activation`.
    //
    // Compared with the legacy path (rate-limited to 1 activate / 5 s /
    // bundle-id), session-backed clients get:
    //   * zero incidental activations after the initial open
    //   * O(1) session cache lookup
    //   * explicit escape hatch for foreground-drift recovery (renew)
    //
    // The table stores each session's bundle id + XCUIApplication +
    // last-renew timestamp for the 2 s renew rate limit.
    struct SessionEntry: Sendable {
      let bundleId: String
      let app: XCUIApplication
      var lastActivatedAt: Date
      /// Last time this session was touched by any
      /// request (Session-Id header hit in resolveApp). The
      /// idle-close sweep uses this to reap sessions that haven't
      /// been used within `sessionIdleTimeoutSec`.
      var lastAccessedAt: Date
      /// Snapshot of `interactiveNamedIds` observed on
      /// the most-recent successful `launchApp` that opted into
      /// `waitForInteractiveMs`. Empty on session open + on launches
      /// where reachedInteractive was false. Persisted per-session
      /// so `smix diagnostic dump` / `session/list` can surface WHICH
      /// ax-ids fired the interactive gate, not just the count.
      var lastInteractiveNamedIds: [String] = []
    }
    let sessions: NSLock = NSLock()
    var sessionTable: [String: SessionEntry] = [:]
    let renewCooldown: TimeInterval = 2.0

    // Session persistence across XCTest lifecycle.
    //
    // The runner runs INSIDE the sim; its writable Documents dir
    // persists across xcodebuild restarts. Every mutation flushes the
    // session table to `~/Documents/smix-sessions.json`, and boot
    // rehydrates whatever was there. `smix runner cycle` (host side)
    // preserves the file — consumer's `Session-Id` survives the cycle.
    //
    // Persisted schema:
    //   {"schema":1,"sessions":[{"sessionId","bundleId","openedAtMs",
    //                             "lastActivatedAtMs"}]}
    //
    // The XCUIApplication reference is NOT persisted — it's a live
    // binding to XCTest infrastructure. On boot each session is
    // rehydrated with a fresh XCUIApplication(bundleIdentifier:) —
    // NO .activate() call, the client's next request handles that.
    let sessionsFileURL: URL = {
      let docs = FileManager.default.urls(for: .documentDirectory,
                                          in: .userDomainMask)[0]
      return docs.appendingPathComponent("smix-sessions.json")
    }()
    struct PersistedSession: Codable {
      let sessionId: String
      let bundleId: String
      let openedAtMs: UInt64
      let lastActivatedAtMs: UInt64
    }
    struct SessionsFile: Codable {
      let schema: Int
      let sessions: [PersistedSession]
    }
    // Serialize table → file. Caller holds `sessions` lock or is
    // otherwise single-writer. Best-effort: write failures log to
    // stderr and don't affect the running session.
    let persistSessions: @Sendable () -> Void = {
      var records: [PersistedSession] = []
      for (sid, entry) in sessionTable {
        let openedMs = UInt64(entry.lastActivatedAt.timeIntervalSince1970 * 1000)
        records.append(PersistedSession(
          sessionId: sid,
          bundleId: entry.bundleId,
          openedAtMs: openedMs,
          lastActivatedAtMs: openedMs
        ))
      }
      let file = SessionsFile(schema: 1, sessions: records)
      do {
        let data = try JSONEncoder().encode(file)
        // Atomic-rename write via .atomic option (Cocoa spelling of
        // rename(2)-based crash-safe overwrite).
        try data.write(to: sessionsFileURL, options: .atomic)
      } catch {
        FileHandle.standardError.write(
          Data("smix-runner: sessions persist failed: \(error)\n".utf8))
      }
    }
    // Rehydrate the table from disk (called once at boot before the
    // server starts). Returns synchronously; failures start empty.
    let rehydrateSessions: @Sendable () -> Void = {
      guard FileManager.default.fileExists(atPath: sessionsFileURL.path) else {
        return
      }
      do {
        let data = try Data(contentsOf: sessionsFileURL)
        let file = try JSONDecoder().decode(SessionsFile.self, from: data)
        guard file.schema == 1 else {
          FileHandle.standardError.write(
            Data("smix-runner: sessions schema=\(file.schema) unknown; ignoring\n".utf8))
          return
        }
        let now = Date()
        for record in file.sessions {
          let target = (record.bundleId == bundleId)
            ? app
            : XCUIApplication(bundleIdentifier: record.bundleId)
          let entry = SessionEntry(
            bundleId: record.bundleId,
            app: target,
            lastActivatedAt: Date(timeIntervalSince1970: TimeInterval(record.lastActivatedAtMs) / 1000),
            lastAccessedAt: now
          )
          sessionTable[record.sessionId] = entry
        }
        FileHandle.standardError.write(
          Data("smix-runner: rehydrated \(file.sessions.count) session(s) from \(sessionsFileURL.path)\n".utf8))
      } catch {
        FileHandle.standardError.write(
          Data("smix-runner: sessions rehydrate failed: \(error)\n".utf8))
      }
    }
    // Rehydrate BEFORE server starts.
    sessions.lock()
    rehydrateSessions()
    sessions.unlock()
    /// Session idle timeout (seconds). Tightened from
    /// the aspirational 120 s pre-implementation window down to 60 s
    /// so SIGKILL-orphaned client sessions vanish within a minute.
    let sessionIdleTimeoutSec: TimeInterval = 60.0
    /// How often the idle-close sweep runs.
    let sessionIdleSweepIntervalSec: TimeInterval = 15.0
    let resolveApp: @Sendable () async -> XCUIApplication = {
      let ctx = SmixRunnerServer.currentContext
      // Session-Id header path — hit the session table, no activation.
      if let sid = ctx.sessionId {
        sessions.lock()
        var entry = sessionTable[sid]
        // Every hit refreshes the last-access clock so
        // the sweep only reaps genuinely-idle sessions.
        if entry != nil {
          entry!.lastAccessedAt = Date()
          sessionTable[sid] = entry!
        }
        sessions.unlock()
        if let entry = entry {
          return entry.app
        }
        // Unknown session id → fall through to legacy path with the
        // provided bundleId (best-effort recovery).
      }
      if let b = ctx.bundleId, b != bundleId {
        let target = XCUIApplication(bundleIdentifier: b)
        if ctx.activate {
          _ = await maybeActivate(b, target)
        }
        return target
      }
      if ctx.activate {
        _ = await maybeActivate(bundleId, app)
      }
      return app
    }

    // Per-target frame lookup. Callers that used the module-scope
    // `cachedAppFrame` pass their resolved app; this returns the
    // memoized frame or captures it lazily.
    let frameFor: @Sendable (XCUIApplication) -> CGRect = { target in
      // Bundle id keys the cache. XCUIApplication doesn't expose bundleId as
      // a public property, so fall back to `.description` (rare-lock case).
      let key = target === app
        ? bundleId
        : (SmixRunnerServer.currentContext.bundleId ?? String(describing: target))
      cachedAppFramesLock.lock()
      defer { cachedAppFramesLock.unlock() }
      if let f = cachedAppFrames[key] {
        return f
      }
      let f = target.frame
      cachedAppFrames[key] = f
      return f
    }

    // Compat shim: the tapHandler path below reads
    // `cachedAppFrame` in a couple of places; keep that name pointing at the
    // default-bundle frame until every call site is migrated to `frameFor`.
    let cachedAppFrame = cachedAppFrames[bundleId]!

    // R5.b (audit Med-11) — runner bind port resolved from
    // `SMIX_RUNNER_PORT` env (Xcode strips `TEST_RUNNER_` prefix) via
    // `RunnerPortResolver`. Empty/missing → default 22087. cell-pool
    // N>1 was structurally broken pre-R5.b because every cell shared
    // port 22087; now each cell ships its own TEST_RUNNER_SMIX_RUNNER_PORT
    // through `buildRunnerLaunchSpec`.
    let resolvedPort = RunnerPortResolver.resolve(
      env: ProcessInfo.processInfo.environment
    )
    // Mark boot time so `/diagnostic/dump` can report
    // runner uptime.
    let bootAt: Date = Date()
    let server = SmixRunnerServer()

    // Background idle-close sweep. Detached task lives
    // for the lifetime of the runner. Every `sessionIdleSweepIntervalSec`
    // seconds it enumerates the session table and closes any entry
    // whose `lastAccessedAt` is older than `sessionIdleTimeoutSec`.
    // SIGKILL-orphaned client sessions (client vanished without POST
    // /session/close) vanish within 60-75 s instead of lingering
    // until runner restart.
    Task.detached { @Sendable in
      while !Task.isCancelled {
        try? await Task.sleep(nanoseconds: UInt64(sessionIdleSweepIntervalSec * 1_000_000_000))
        let now = Date()
        sessions.lock()
        var reaped: [String] = []
        for (sid, entry) in sessionTable
          where now.timeIntervalSince(entry.lastAccessedAt) >= sessionIdleTimeoutSec {
          reaped.append(sid)
        }
        for sid in reaped {
          sessionTable.removeValue(forKey: sid)
        }
        if !reaped.isEmpty {
          persistSessions()
        }
        sessions.unlock()
        if !reaped.isEmpty {
          FileHandle.standardError.write(
            Data("smix-runner: idle-close sweep reaped \(reaped.count) session(s) (idle > \(Int(sessionIdleTimeoutSec))s)\n".utf8))
        }
      }
    }

    // Extract the app-alive cache so the diagnostic handler can
    // direct-capture it rather than read the task-local. Reading the
    // task-local reports `aliveCache: null`: FlyingFox's per-request task
    // spawn does not inherit the `withValue` scope wrapping
    // `server.run()`, so `SmixRunnerServer.currentAppAliveCache` is nil at
    // request time. Capturing the reference in the closure sidesteps that.
    let localAppAliveCache = AppAliveCache(ttlMs: 20_000)

    // Cumulative session lifecycle counters.
    // Advance on every mutation; survive session close (unlike the
    // instantaneous `sessionTable.count` view that returns 0 at
    // end-of-batch and hides "was anything opened during this run").
    // A final class + NSLock rather than an actor so we can access
    // synchronously from inside sync handlers without stray suspend
    // points, and so the counter increment is a single-line inline
    // operation not an `await`.
    final class LifecycleCounters: @unchecked Sendable {
      private let lock = NSLock()
      private var counters = SessionRoute.SessionLifecycleCounters()
      func advance(_ change: (inout LifecycleCountersInner) -> Void) {
        lock.lock(); defer { lock.unlock() }
        var inner = LifecycleCountersInner(from: counters)
        change(&inner)
        counters = inner.toWire()
      }
      func snapshot() -> SessionRoute.SessionLifecycleCounters {
        lock.lock(); defer { lock.unlock() }
        return counters
      }
    }
    // Mutable inner mirror so we can advance individual fields inside
    // the `advance` closure (the wire type is immutable-let-based).
    struct LifecycleCountersInner {
      var openedTotal: UInt64
      var closedTotal: UInt64
      var relaunchAppTotal: UInt64
      var terminateAppTotal: UInt64
      var terminateAppViaXCUIApplication: UInt64
      var terminateAppViaFallback: UInt64
      var launchAppTotal: UInt64
      var launchAppReachedForeground: UInt64
      var launchAppTimedOutBeforeForeground: UInt64
      // Interactive fingerprint counters.
      var launchAppReachedInteractive: UInt64
      var launchAppTimedOutBeforeInteractive: UInt64
      init(from c: SessionRoute.SessionLifecycleCounters) {
        openedTotal = c.openedTotal
        closedTotal = c.closedTotal
        relaunchAppTotal = c.relaunchAppTotal
        terminateAppTotal = c.terminateAppTotal
        terminateAppViaXCUIApplication = c.terminateAppViaXCUIApplication
        terminateAppViaFallback = c.terminateAppViaFallback
        launchAppTotal = c.launchAppTotal
        launchAppReachedForeground = c.launchAppReachedForeground
        launchAppTimedOutBeforeForeground = c.launchAppTimedOutBeforeForeground
        launchAppReachedInteractive = c.launchAppReachedInteractive
        launchAppTimedOutBeforeInteractive = c.launchAppTimedOutBeforeInteractive
      }
      func toWire() -> SessionRoute.SessionLifecycleCounters {
        SessionRoute.SessionLifecycleCounters(
          openedTotal: openedTotal,
          closedTotal: closedTotal,
          relaunchAppTotal: relaunchAppTotal,
          terminateAppTotal: terminateAppTotal,
          terminateAppViaXCUIApplication: terminateAppViaXCUIApplication,
          terminateAppViaFallback: terminateAppViaFallback,
          launchAppTotal: launchAppTotal,
          launchAppReachedForeground: launchAppReachedForeground,
          launchAppTimedOutBeforeForeground: launchAppTimedOutBeforeForeground,
          launchAppReachedInteractive: launchAppReachedInteractive,
          launchAppTimedOutBeforeInteractive: launchAppTimedOutBeforeInteractive
        )
      }
    }
    let lifecycleCounters = LifecycleCounters()
    // Top-level `lastInteractiveNamedIds` on the diagnostic dump.
    // Per-session `interactiveNamedIds` goes with the session at
    // teardown, but post-mortem
    // triage often runs AFTER the batch closes every session — so the
    // WHICH-ids sample vanishes right when we want it. This box
    // survives session-close: last non-empty snapshot across all
    // launchApp completions since runner boot. Empty when no launch
    // has completed with a non-empty sample yet.
    final class LastInteractiveIdsBox: @unchecked Sendable {
      private let lock = NSLock()
      private var ids: [String] = []
      func update(_ next: [String]) {
        guard !next.isEmpty else { return }
        lock.lock(); defer { lock.unlock() }
        ids = next
      }
      func snapshot() -> [String] {
        lock.lock(); defer { lock.unlock() }
        return ids
      }
    }
    let lastInteractiveIdsBox = LastInteractiveIdsBox()

    try await server.runForever(
      port: resolvedPort,
      tapHandler: { req, scope in
        // Per-request target-app rebind.
        let app = await resolveApp()
        // Tap navigates / changes focus; invalidate keyboard cache.
        KeyboardCache.shared.invalidate()
        // C2 selector subset: text → match by label OR identifier.
        // Settings rows are XCUIElementTypeCell, not button; using `.any` covers both.
        // 3 stage timers around resolve / tap call / total. SMIX_STAGE_LOG
        // env on the SDK side decides whether stages reach disk; runner always returns
        // them (cheap) so default opt-in path collects data.
        // `.resolve` mode returns the element frame + cached app frame
        // (so SDK can host-HID-inject at coord) and skips element.tap() AND the
        // isHittable check. Rationale: HID injection at coord doesn't need
        // XCUITest's hittable semantic (which forces a redundant snapshot per
        // call); waitForExistence already proves the element is in the AX tree.
        // If the element is in-tree but visually offscreen, the tap will simply
        // miss and the caller's next expect/waitFor will surface it.
        // `.resolveAndTap` keeps isHittable + the legacy synchronous tap.
        // Sub-stage timers within resolve (wait_existence_ms /
        // frame_read_ms) emitted only for .resolve mode to attribute the
        // measured P50 gap from the theoretical floor.
        let t0 = Date()
        let predicate = Self.predicate(for: req.selector)
        // nil scope: default resolution (`query.firstMatch`, resolved
        // OUTSIDE the guard; the SDK posts /tap with no `?include=`).
        // "all-windows": defer resolution INTO the guard via
        // `firstSeeThroughMatch` — the flat enumeration reads
        // `.label`/`.identifier` on possibly-vanishing handles, so it must
        // run inside the main-thread ObjC trampoline span.
        let seeThrough = (scope == "all-windows")
        let query = app.descendants(matching: .any).matching(predicate)
        let element0: XCUIElement? = seeThrough ? nil : query.firstMatch
        let tBeforeWait = Date()
        // Short-circuit on cached-snapshot exists. Instrumentation
        // attributed ~1072 ms (99.98%) of resolve_ms to
        // waitForExistence — XCUITest's polling cycle has a
        // ~1s minimum even when the element is already in the live tree
        // (Settings → "General" row). Try the synchronous `.exists`
        // (single snapshot read) first; only fall back to the slow
        // waitForExistence path when the element is genuinely not yet
        // rendered (e.g., mid-navigation, animation still landing).
        //
        // Every XCUITest access is wrapped in the ObjC trampoline span
        // (`smixGuarded`). Any state transition — an overlay rendering or
        // dismissing, a reload, an animation settling — can make an
        // already-resolved element vanish mid-flight, at which point
        // XCUITest throws an NSException from inside `.tap()` / `.frame` /
        // `.isHittable`. The trampoline maps that throw to `.notFound`,
        // the existing not-found wire shape, so the runner survives.
        let outcome: SmixRunnerServer.TapOutcome? = smixGuarded("tap") {
          let element: XCUIElement
          if seeThrough {
            // See-through: resolve from the masked-content-reaching flat /
            // per-window enumeration (same set buildAllWindowsSnapshot
            // uses). Already proven existing during enumeration, so no
            // waitForExistence cycle is needed (and none would help — the
            // masked element never enters the live single-app tree
            // waitForExistence polls).
            guard let m = firstSeeThroughMatch(app: app, selector: req.selector)
            else { return SmixRunnerServer.TapOutcome.notFound }
            element = m
          } else {
            // Legacy: byte-identical to before this fix.
            guard let legacy = element0 else {
              return SmixRunnerServer.TapOutcome.notFound
            }
            element = legacy
            if !element.exists {
              guard element.waitForExistence(timeout: 3) else {
                return SmixRunnerServer.TapOutcome.notFound
              }
            }
          }
          let tAfterWait = Date()
          // `.isHittable` does not gate the see-through path. For an
          // element resolved through a native modal — reachable in the AX
          // tree but visually covered — it always reports false. The
          // read-through semantics are explicit ("AX reachable, and the
          // intent is THROUGH the overlay"), so hittability is skipped in
          // favour of a `coordinate.tap()` at the element's AX centre; a
          // coordinate tap is not hittability-gated.
          //
          // Note the limit: iOS hit-testing still routes the actual touch
          // by z-order, so if the overlay intercepts the touch the
          // underlying onPress will NOT fire. Reading through is a sense
          // capability; acting is still bound by iOS hit-testing.
          //
          // nil scope keeps the `.isHittable` + `element.tap()` path. The
          // main-thread trampoline backstops a vanished element on both.
          if req.mode == .resolveAndTap && !seeThrough {
            guard element.isHittable else {
              return SmixRunnerServer.TapOutcome.notFound
            }
          }
          let tResolveEnd = Date()
          let elementFrame = element.frame
          let tAfterFrameRead = Date()
          let tapCallMs: Double
          switch req.mode {
          case .resolve:
            tapCallMs = 0
          case .resolveAndTap:
            // The intervening
            // synchronous `element.exists` recheck that used to sit here
            // disrupted XCUITest's internal main-thread marshaling of the
            // synthesized touch dispatch (it added a second snapshot read
            // between resolve and tap, off-main). It is removed: the
            // entire `smixGuarded` body now runs ON the main thread (see
            // SmixExceptionTrampoline) and the ObjC trampoline already
            // catches the vanished-element NSException, mapping it to
            // `.notFound` — the recheck added no coverage the trampoline
            // does not already provide, and was the regression's trigger.
            if seeThrough {
              // Occlusion-tolerant: tap the resolved element's AX center
              // coordinate (not hittability-gated). nil scope is
              // unaffected — this branch only runs for ?include=
              // all-windows, the SDK posts /tap with no query.
              element.coordinate(
                withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5)
              ).tap()
            } else {
              element.tap()
            }
            tapCallMs = Date().timeIntervalSince(tAfterFrameRead) * 1000
          case .daemonProxySynthesize:
            // RN Pressable JS-thread onPress unreliable
            // through XCUIElement.tap()'s Apple gesture recognizer chain
            // (RN `RCTTouchHandler` UIGestureRecognizer gets that synthesised
            // gesture cancelled or routed past it). Alternative dispatch
            // emits a raw IOKit-level touch event in the resolved element
            // frame centre via `XCTRunnerDaemonSession.daemonProxy._XCT_
            // synthesizeEvent:completion:` (no XCUIElement-owner metadata),
            // which UIKit's standard hit-test routes through RN's gesture
            // chain → Pressable onPress fires. Same daemonProxy path as
            // tapAtCoordHandler and EventSynthesizer.swift.
            let center = CGPoint(
              x: elementFrame.origin.x + elementFrame.size.width / 2,
              y: elementFrame.origin.y + elementFrame.size.height / 2
            )
            guard let record = SmixEventRecord(orientation: .portrait),
                  record.addPointerTouchEvent(at: center) else {
              return SmixRunnerServer.TapOutcome.notFound
            }
            // smixGuarded body is sync (ObjC trampoline contract);
            // daemonProxy.synthesize is async. Bridge with a sema, same
            // pattern as DaemonKeyboard sync wrappers elsewhere in the
            // runner.
            let sema = DispatchSemaphore(value: 0)
            var synthesizeOk = false
            Task {
              do {
                try await SmixRunnerDaemonProxy.shared.synthesize(record: record)
                synthesizeOk = true
              } catch {
                FileHandle.standardError.write(
                  Data("smix-runner: tap daemonProxySynthesize error: \(error)\n".utf8))
              }
              sema.signal()
            }
            sema.wait()
            tapCallMs = Date().timeIntervalSince(tAfterFrameRead) * 1000
            guard synthesizeOk else { return SmixRunnerServer.TapOutcome.notFound }
          }
          let tEnd = Date()
          let label = element.label
          let stages = TapRoute.TapStages(
            resolveMs: tResolveEnd.timeIntervalSince(t0) * 1000,
            tapCallMs: tapCallMs,
            totalMs: tEnd.timeIntervalSince(t0) * 1000,
            waitExistenceMs: tAfterWait.timeIntervalSince(tBeforeWait) * 1000,
            frameReadMs: tAfterFrameRead.timeIntervalSince(tResolveEnd) * 1000
          )
          return SmixRunnerServer.TapOutcome.matched(
            label: label.isEmpty ? req.selector.raw : label,
            stages: stages,
            frame: elementFrame,
            appFrame: cachedAppFrame
          )
        }
        return outcome ?? .notFound
      },
      snapshotHandler: { scope in
        // Per-request target-app rebind: previously `app` captured
        // whichever process was foreground at runner boot (often
        // Settings / Preferences), so every `/tree` returned
        // snapshot_unavailable once that process backgrounded. The
        // client's `App-Bundle-Id` header rebinds per request; missing
        // header falls back to the cached boot-time app.
        let app = await resolveApp()
        // XCUIElement.snapshot() is a throwing, blocking call
        // (~50-100 ms on Settings). Returning nil here causes the server to
        // respond `500 {"ok":false,"error":"snapshot_unavailable"}` — used
        // when the target app has terminated mid-test.
        //
        // `scope` carries the `?include=` query value.
        //
        //   nil / anything but "all-windows": LEGACY path. A single
        //     `app.snapshot()` root, byte-identical to before this
        //     checkpoint. This is the zero-regression anchor: no param ⇒
        //     the exact same bytes the SDK / host smoke gates already
        //     parse. Guarded so a mid-test crash → nil → 500
        //     snapshot_unavailable (the existing wire shape), never a
        //     thrown error that kills test_runForever.
        //
        //   "all-windows": SEE-THROUGH path. Any opaque native modal makes
        //     iOS accessibility mask the content beneath it out of the
        //     single-app snapshot, even though XCUITest's lower
        //     flat-enumeration layer can still reach it.
        //     Walk every window's `.snapshot()` AND a flat
        //     `descendants(.any)` fallback, merge into ONE synthetic
        //     root, then run the UNCHANGED `convertSnapshot` /
        //     `TreeRoute.serialize` over it. The synthetic root keeps
        //     `rootIdentifierOverride: bundleId` so host smoke
        //     `.identifier == bundle` still passes.
        if scope == "all-windows" {
          return smixGuarded("tree-all-windows") {
            buildAllWindowsSnapshot(app: app, bundleId: bundleId,
                                    appFrame: cachedAppFrame)
          } ?? nil
        }
        let snap: XCUIElementSnapshot? = smixGuarded("tree") {
          try? app.snapshot()
        } ?? nil
        guard let snap else { return nil }
        // One-shot KVC walk on the snapshot tree to find the
        // first responder identifier. Threaded into convertSnapshot so the
        // matching POCO node gets `hasFocus=true`. Public
        // `dictionaryRepresentation` filters out `hasKeyboardFocus`, so
        // this KVC ivar read is the only reliable path in iOS 26.5 sim.
        let focusHint = FocusedIdentifier.find(in: snap)
        // `XCUIElementSnapshot.identifier` on the application root is
        // typically empty (XCUITest only auto-populates identifier for
        // accessibility-identified subviews). C1 surfaces the bundle id at
        // the root by overriding the converted POCO's identifier when the
        // snapshot doesn't carry one — keeps `TreeRoute.serialize` purely
        // mechanical and host-side smoke gates can assert on
        // `.identifier == "com.apple.Preferences"`.
        // The bundle THIS request resolved to, not the one the runner
        // booted with. `resolveApp()` rebinds per request off the
        // App-Bundle-Id header, so a client driving a second app got a
        // snapshot of that app carrying the launch-time id at its root
        // — right nearly always, wrong exactly when it matters.
        //
        // Same fallback shape as `frameFor` above, and for the same
        // reason: XCUIApplication does not expose its bundle id, so the
        // request context is the only place to read it from.
        let resolvedBundle = SmixRunnerServer.currentContext.bundleId ?? bundleId
        let root = convertSnapshot(snap, rootIdentifierOverride: resolvedBundle, focusHint: focusHint)
        // Reuse cachedAppFrame (the same invariance the tapHandler
        // optimization relies on). app.frame access internally triggers a
        // light snapshot (~50-150ms); Settings is portrait-only and the
        // window doesn't resize across taps so the value is invariant for
        // this runner's lifetime. Saved ~50-100ms per /tree call.
        return (root: root, appFrame: cachedAppFrame)
      },
      // Resolve text-only selectors via the same
      // predicate as tapHandler, then call XCUIElement.typeText /
      // .clearAndEnterText / app.typeKey for pressKey. All three are
      // best-effort: false return means element not found (404) or
      // unsupported key (400); the SDK surfaces these via the same
      // ExpectationFailure shape as tap.
      // Keyboard ops via XCTest daemon fast path
      // (XCTRunnerDaemonSession.sharedSession.daemonProxy._XCT_sendString...).
      // Daemon proxy types into whatever element is currently focused — much
      // faster than XCUIElement.typeText (which does per-char roundtrips
      // through the test target's main thread). For a non-`_focused_`
      // selector, the handler first taps the matching typable element to
      // give it focus, then submits the string to the daemon.
      fillHandler: { selectorText, text, scope, dispatch, clearFirst in
        let app = await resolveApp()  // Per-request target-app rebind.
        let t0 = DispatchTime.now()
        // `key-events` skips focus resolution and types into whatever
        // holds focus. That is the whole point of the mode: it is for
        // fields the a11y tree cannot address, so resolving through the
        // a11y tree first would fail for exactly the callers who asked
        // for it.
        let resolveFocus = dispatch != .keyEvents
        if resolveFocus && !selectorText.isEmpty && selectorText != "_focused_" {
          // Focus-tap can hit a vanished element (same
          // root cause as tapHandler). Guard the resolve+tap span; a
          // caught failure leaves the field unfocused and the daemon
          // send below simply targets whatever is focused (best-effort,
          // unchanged contract) instead of killing the runner.
          //
          // nil scope: default predicate resolution (the SDK posts /fill
          // with no `?include=`). "all-windows": when the default
          // predicate (which runs against the modal-masked single
          // app tree) finds nothing, fall back to the see-through set so
          // a typable field behind a native modal is still focus-tapped.
          _ = smixGuarded("fill-focus") { () -> Bool in
            let typablePredicate = NSPredicate(format:
              "(label == %@ OR identifier == %@) AND " +
              "(elementType == 49 OR elementType == 50 OR elementType == 52 OR elementType == 45)",
              selectorText, selectorText)
            var element = app.descendants(matching: .any).matching(typablePredicate).firstMatch
            if !element.exists {
              let anyText = NSPredicate(format:
                "elementType == 49 OR elementType == 50 OR elementType == 52 OR elementType == 45")
              element = app.descendants(matching: .any).matching(anyText).firstMatch
            }
            if element.exists {
              if !element.hasFocus { element.tap() }
            } else if scope == "all-windows",
                      let m = firstSeeThroughMatch(app: app, selector: .text(selectorText)) {
              if !m.hasFocus { m.tap() }
            }
            return true
          }
        }
        let t1 = DispatchTime.now()
        do {
          // typeText appends, so a field with something in it ends up
          // holding the old value and the new one concatenated. In a
          // secure field that is invisible — the dots look right and
          // the login fails. Empty it first unless the caller asked to
          // append, using the same proportional delete count /clear
          // uses so this costs a field's length, not a fixed 64.
          if clearFirst {
            let count: Int
            if let cached = KeyboardCache.shared.length {
              count = max(cached + 4, 4)
            } else {
              let raw: String = smixGuarded("fill-clear-read") { () -> String in
                let focusedPred = NSPredicate(format: "hasKeyboardFocus == true")
                let focused = app.descendants(matching: .any).matching(focusedPred).firstMatch
                return (focused.exists ? focused.value as? String : nil) ?? ""
              } ?? ""
              count = max(raw.count + 4, 4)
            }
            let deletes = String(repeating: XCUIKeyboardKey.delete.rawValue, count: count)
            try await DaemonKeyboard.shared.sendString(deletes, typingFrequency: 200)
            KeyboardCache.shared.recordClear()
          }
          try await DaemonKeyboard.shared.sendString(text, typingFrequency: 200)
          let t2 = DispatchTime.now()
          // Track typed text length. clear's hot path reads this to
          // skip the snapshot-triggering focused.value read.
          KeyboardCache.shared.appendFill(text)
          return .success(
            focusMs: SmixRunnerServer.msBetween(t0, t1),
            daemonSendMs: SmixRunnerServer.msBetween(t1, t2)
          )
        } catch { return .notFound }
      },
      clearHandler: { selectorText, scope in
        let app = await resolveApp()  // Per-request target-app rebind.
        let t0 = DispatchTime.now()
        if !selectorText.isEmpty && selectorText != "_focused_" {
          // Guard the focus-tap span (vanished-element safe).
          // nil scope: default resolution (the SDK posts /clear with no
          // `?include=`).
          // "all-windows": see-through fallback when the masked single
          // app tree yields nothing (mirrors fillHandler).
          _ = smixGuarded("clear-focus") { () -> Bool in
            let typablePredicate = NSPredicate(format:
              "(label == %@ OR identifier == %@) AND " +
              "(elementType == 49 OR elementType == 50 OR elementType == 52 OR elementType == 45)",
              selectorText, selectorText)
            var element = app.descendants(matching: .any).matching(typablePredicate).firstMatch
            if !element.exists {
              let anyText = NSPredicate(format:
                "elementType == 49 OR elementType == 50 OR elementType == 52 OR elementType == 45")
              element = app.descendants(matching: .any).matching(anyText).firstMatch
            }
            if element.exists && !element.hasFocus { element.tap() }
            else if !element.exists, scope == "all-windows",
                    let m = firstSeeThroughMatch(app: app, selector: .text(selectorText)) {
              if !m.hasFocus { m.tap() }
            }
            return true
          }
        }
        // Proportional delete count. Previously we always
        // sent max(value.count, 64) deletes = ~640ms minimum at
        // typingFrequency=100. Now scale to actual field content +
        // small overshoot, dropping clear of a 5-char field from
        // ~640ms → ~90ms (7× speedup on this operation).
        //
        // Hot path: when caller uses `_focused_` AND we have a
        // tracked text length from a prior fill/pressKey, skip the
        // snapshot-triggering `focused.value` read (~50-80ms). Cache is
        // invalidated by tap / non-delete pressKey so an out-of-sync
        // cache resolves to nil → falls back to the safe snapshot path.
        let count: Int
        if selectorText == "_focused_", let cached = KeyboardCache.shared.length {
          count = max(cached + 4, 4)
        } else {
          // The focused.value read triggers a snapshot
          // that can fail under modal masking. Guard it; a caught
          // failure falls back to the minimum delete count (4) — the
          // same conservative default as an empty field.
          let raw: String = smixGuarded("clear-read") { () -> String in
            let focusedPred = NSPredicate(format: "hasKeyboardFocus == true")
            let focused = app.descendants(matching: .any).matching(focusedPred).firstMatch
            return (focused.exists ? focused.value as? String : nil) ?? ""
          } ?? ""
          count = max(raw.count + 4, 4)
        }
        let deletes = String(repeating: XCUIKeyboardKey.delete.rawValue, count: count)
        let t1 = DispatchTime.now()
        do {
          try await DaemonKeyboard.shared.sendString(deletes, typingFrequency: 200)
          let t2 = DispatchTime.now()
          // Field is now empty.
          KeyboardCache.shared.recordClear()
          return .success(
            focusMs: SmixRunnerServer.msBetween(t0, t1),
            daemonSendMs: SmixRunnerServer.msBetween(t1, t2)
          )
        } catch { return .notFound }
      },
      pressKeyHandler: { key in
        let t0 = DispatchTime.now()
        // iOS hardware buttons (home / lock / volumeUp / volumeDown) are
        // not keyboard events: they go through the XCUIDevice public API
        // and never enter the mapping dict. Supported:
        //   home          → XCUIDevice.shared.press(.home, forDuration: 0)
        //   volumeUp/Down → XCUIDevice.shared.press(.volumeUp/.volumeDown, ...)
        // On the iOS simulator, XCUIDevice.Button exposes no public enum
        // case for lock, and simctl has no lock interface either — so lock
        // explicitly returns .notFound and reports unsupported on stderr,
        // rather than silently no-op'ing.
        switch key {
        case "home":
          XCUIDevice.shared.press(XCUIDevice.Button.home)
          let t2 = DispatchTime.now()
          KeyboardCache.shared.invalidate()
          return .success(focusMs: 0, daemonSendMs: SmixRunnerServer.msBetween(t0, t2))
        case "lock":
          FileHandle.standardError.write(
            Data("smix-runner: pressKey lock: unsupported on iOS Simulator (no XCUIDevice.Button.lock, and simctl has no lock verb); maestro has the same limitation\n".utf8))
          return .notFound
        case "volumeUp", "volumeDown":
          // Apple documents XCUIDevice.Button.volumeUp/.volumeDown as
          // unavailable in the iOS Simulator (physical iOS device only).
          // The adapter is expected to gracefully skip these at the
          // runtime layer so they never reach the wire; if one does
          // arrive, the adapter missed the check — report on stderr and
          // return .notFound so the runner survives.
          FileHandle.standardError.write(
            Data("smix-runner: pressKey \(key): unavailable in iOS Simulator (Apple XCUIDevice.Button restriction); the adapter should have skipped this before it reached the wire\n".utf8))
          return .notFound
        default: break
        }
        // keyboard-event path (return / delete / tab / space / escape /
        // arrows). Arrow keys ride the same XCUIKeyboardKey rawValue →
        // `_XCT_sendString` mechanism as the original five; wire names
        // match the Rust KeyName camelCase serialization (smix-input).
        let mapping: [String: String] = [
          "return": XCUIKeyboardKey.return.rawValue,
          "delete": XCUIKeyboardKey.delete.rawValue,
          "tab":    XCUIKeyboardKey.tab.rawValue,
          "space":  XCUIKeyboardKey.space.rawValue,
          "escape": XCUIKeyboardKey.escape.rawValue,
          "arrowUp":    XCUIKeyboardKey.upArrow.rawValue,
          "arrowDown":  XCUIKeyboardKey.downArrow.rawValue,
          "arrowLeft":  XCUIKeyboardKey.leftArrow.rawValue,
          "arrowRight": XCUIKeyboardKey.rightArrow.rawValue,
        ]
        guard let raw = mapping[key] else { return .notFound }
        let t1 = DispatchTime.now()
        do {
          try await DaemonKeyboard.shared.sendString(raw, typingFrequency: 200)
          let t2 = DispatchTime.now()
          // Track keyboard cache: 'delete' decrements; others
          // (return/tab/escape/space) may change focus → invalidate.
          KeyboardCache.shared.recordPressKey(key)
          return .success(
            focusMs: SmixRunnerServer.msBetween(t0, t1),
            daemonSendMs: SmixRunnerServer.msBetween(t1, t2)
          )
        } catch { return .notFound }
      },
      // /find: XCUIElement query for "does this label/identifier
      // exist", without paying the cost of XCUIApplication.snapshot() +
      // serialization. Used by SDK `expect.toBeVisible()` for simple
      // text selectors. Returns boolean.
      findHandler: { selectorText, scope, requireOnScreen in
        let app = await resolveApp()  // Per-request target-app rebind.
        // `.exists` triggers a snapshot that can fail
        // under modal masking. Guard it; a caught failure maps to the
        // existing not-found wire shape (`false`), never a runner kill.
        //
        // nil scope: the default
        // `app.descendants(.any).matching(predicate).firstMatch.exists`
        // (the SDK posts /find with no `?include=`).
        // "all-windows": resolve from the same masked-content-reaching
        // see-through set /tree?include=all-windows uses, so
        // `expect.toBeVisible()` of content behind a native modal returns
        // the truthful answer instead of a masked false.
        if scope == "all-windows" {
          let hit = smixGuarded("find-all-windows") { () -> Bool in
            firstSeeThroughMatch(app: app, selector: .text(selectorText)) != nil
          } ?? false
          return SmixRunnerServer.FindOutcome(found: hit)
        }
        let hit = smixGuarded("find") { () -> Bool in
          let predicate = NSPredicate(
            format: "label == %@ OR identifier == %@",
            selectorText, selectorText
          )
          let el = app.descendants(matching: .any)
            .matching(predicate)
            .firstMatch
          guard el.exists else { return false }
          // Live on-screen confirmation. iOS 26.5 + RN
          // Fabric SNAPSHOT frames drift for below-the-fold elements
          // (report stale in-viewport coords with visible=true), so
          // the tree tier can false-green a wait while the same
          // element honestly fails a tap. The LIVE query here
          // re-resolves current layout: `el.frame` is the truth. We
          // check frame ∩ app.frame (on-screen) rather than
          // `isHittable` deliberately — hittability is false for
          // elements under floating overlays (e.g. a QA bubble),
          // which are genuinely visible and assertable.
          if requireOnScreen {
            return el.frame.intersects(app.frame)
          }
          return true
        } ?? false
        if hit {
          return SmixRunnerServer.FindOutcome(found: true)
        }
        // The refusal explains itself, and only the refusal: the second
        // query below is not free, and a match has nothing to explain.
        //
        // `candidates` separates "this app exposes no elements to the
        // query at all" from "it exposes them and none matched" — from
        // `found:false` alone those look the same, which is how a
        // cross-app flow could fail every assertion with no way in.
        let diagnostics = smixGuarded("find-diagnostics") {
          () -> SmixRunnerServer.FindOutcome in
          SmixRunnerServer.FindOutcome(
            found: false,
            diagnostics: FindRoute.Diagnostics(
              appState: Int(app.state.rawValue),
              candidates: Int(app.descendants(matching: .any).count),
              rebound: SmixRunnerServer.currentContext.bundleId.map { $0 != bundleId } ?? false
            )
          )
        }
        return diagnostics ?? SmixRunnerServer.FindOutcome(found: false)
      },
      // System popup sense. Popup sensing is a core flat capability, not
      // driver-specific; the handler enumerates SpringBoard alerts /
      // sheets / dialogs AND the bound app's own alert / sheet / dialog /
      // popover (the structural iOS modal types) into Popup POCOs.
      // The route owns envelope serialization + guarded fallback (sense
      // failure ⇒ empty popups, NOT 5xx). `scope` is the `?include=`
      // query value; both nil and "all-windows" reach the same
      // enumeration today (SpringBoard popups and bound-app modals are
      // each enumerated from their own XCUIApplication query path, so
      // the see-through flag does not change the source set — the
      // parameter is plumbed for forward compatibility mirroring /tree /
      // /find / /tap).
      systemPopupsHandler: { scope in
        return smixGuarded("system-popups") {
          collectSystemPopups(
            app: app, bundleId: bundleId,
            includeAllWindows: scope == "all-windows"
          )
        } ?? []
      },
      // POST /system-popup-action handler — the act side. Walks
      // the same SpringBoard / bound-app scan order collectSystemPopups
      // uses (alerts → sheets → bound-app alerts/sheets/dialogs/popovers)
      // so popup.id derivation (container.identifier fallback "popup-N"
      // by global out-count) and button.id derivation (b.identifier
      // fallback "b-N" by intra-popup index) round-trip from enumerate.
      // Matched button frame center → SmixEventRecord pointer touch →
      // SmixRunnerDaemonProxy.shared.synthesize (the daemonProxy dlsym
      // chain — private symbols are reached via dlsym, never
      // hard-linked) so SpringBoard alert handlers
      // and RN onPress both fire reliably. Match miss ⇒ .notFound;
      // synthesize raise inside smixGuarded ⇒ .notFound (caller can
      // re-enumerate + retry).
      systemPopupActionHandler: { popupId, buttonId in
        let outcome: SmixRunnerServer.SystemPopupActionOutcome? =
          smixGuarded("system-popup-action") {
            findAndTapSystemPopupButton(
              app: app, bundleId: bundleId,
              popupId: popupId, buttonId: buttonId
            )
          }
        return outcome ?? .notFound
      },
      // Scroll-until-visible. Resolves the target selector
      // (text or id) via the same label==% / identifier==% predicate as
      // findHandler / tapHandler (no xpath, no regex — those are
      // deliberately kept off the selector surface). Each loop iteration:
      // probe existence on the
      // XCUIElementQuery's firstMatch (cheap, single-snapshot read); if
      // present + isHittable, return (true, swipes); otherwise call
      // XCUIApplication.swipeUp() (direction=='down' ⇒ content moves up
      // = scroll DOWN through content) or swipeDown(). XCUITest's
      // swipeUp/Down are frame-aware, synchronous (return after animation
      // settle), and Apple-supported — no host-HID sidecar required for
      // this checkpoint. Bound by maxSwipes + a wall-clock budget.
      //
      // Returns (false, maxSwipes) on exhaustion; the wire layer (200 OK
      // + matched:false) is the route's normal "not visible" response —
      // same discipline as /find (the result is in the body, not the
      // HTTP status). Mid-interaction vanished-element NSExceptions are
      // caught by smixGuarded → wire as matched:false.
      scrollHandler: { selectorJSON, direction, maxSwipes, timeoutMs, scope in
        let app = await resolveApp()  // Per-request target-app rebind.
        // Parse selectorJSON → text? / id? (mirrors the route's wire shape).
        var sText: String? = nil
        var sId: String? = nil
        if let data = selectorJSON.data(using: .utf8),
           let obj = (try? JSONSerialization.jsonObject(with: data, options: []))
             as? [String: Any]
        {
          sText = obj["text"] as? String
          sId = obj["id"] as? String
        }
        // No usable selector field → caller misuse; return immediately
        // (route layer wraps as matched:false). Only label/id equality is
        // accepted here — xpath and regex selectors are deliberately kept
        // off the selector surface.
        guard sText != nil || sId != nil else {
          return (matched: false, swipes: 0)
        }
        let predicate: NSPredicate
        if let t = sText {
          predicate = NSPredicate(
            format: "label == %@ OR identifier == %@", t, t
          )
        } else if let id = sId {
          predicate = NSPredicate(format: "identifier == %@", id)
        } else {
          return (matched: false, swipes: 0)
        }
        let seeThrough = (scope == "all-windows")
        let startTime = Date()
        let timeoutSeconds = TimeInterval(timeoutMs) / 1000.0
        let outcome: (matched: Bool, swipes: Int)? = smixGuarded("scroll") {
          var swipes = 0
          // Probe first — element might already be on-screen at swipe=0.
          if seeThrough, let _ = sText,
             firstSeeThroughMatch(app: app, selector: .text(sText!)) != nil
          {
            return (matched: true, swipes: 0)
          }
          if !seeThrough {
            let query = app.descendants(matching: .any).matching(predicate)
            if query.firstMatch.exists {
              return (matched: true, swipes: 0)
            }
          }
          while swipes < maxSwipes {
            if Date().timeIntervalSince(startTime) >= timeoutSeconds {
              break
            }
            // XCUIApplication.swipeUp() = swipe gesture moving finger UP =
            // content scrolls DOWN to expose elements below the fold; the
            // SDK / driver maps direction='down' to this (scroll DOWN through
            // content). Symmetric for 'up' → swipeDown().
            // Maestro navigation convention (wire = "what to
            // see"); finger gesture is inverse. See swipeOnceHandler above.
            // Preserve original semantic: invalid direction breaks the while
            // loop (defensive — ScrollRoute.decode already guards).
            if direction == "down" {
              app.swipeUp()        // see below ← finger up
            } else if direction == "up" {
              app.swipeDown()      // see above ← finger down
            } else if direction == "left" {
              app.swipeRight()     // see left ← finger right
            } else if direction == "right" {
              app.swipeLeft()      // see right ← finger left
            } else {
              break
            }
            swipes += 1
            // Post-swipe probe.
            if seeThrough, let t = sText,
               firstSeeThroughMatch(app: app, selector: .text(t)) != nil
            {
              return (matched: true, swipes: swipes)
            }
            if !seeThrough {
              let q = app.descendants(matching: .any).matching(predicate)
              if q.firstMatch.exists {
                return (matched: true, swipes: swipes)
              }
            }
          }
          return (matched: false, swipes: swipes)
        }
        return outcome ?? (matched: false, swipes: 0)
      },
      // POST /foreground handler. Instantiates a fresh
      // XCUIApplication(bundleIdentifier:) — NOT the runner-bound `app` from
      // line 537 (caller-supplied bundleId may differ from runner bound app
      // when test switches apps mid-flow). XCUIApplication.activate is
      // Apple synchronous fire-and-forget; the handler returns true on
      // "activate request dispatched" (XCUITest didn't throw), not on
      // "app truly frontmost" — verifying "really in foreground" is a
      // sense-layer job and belongs to the caller (which runs app.tree() /
      // app.findOne afterwards). smixGuarded wraps the activate call so a
      // vanished-bundle NSException surfaces as `false` instead of crashing
      // the runner.
      foregroundHandler: { bundleId in
        // `.activate()` is main-actor-isolated on iOS 26+ SDKs; hop
        // through `SmixRunnerServer.onMain`. The `smixGuarded`
        // NSException trampoline stays inside the main hop so a
        // vanished-bundle exception surfaces as `false` instead of
        // crashing the runner.
        let outcome: Bool? = await SmixRunnerServer.onMain {
          smixGuarded("foreground") {
            XCUIApplication(bundleIdentifier: bundleId).activate()
            return true
          }
        }
        return outcome ?? false
      },
      // POST /back handler. Queries the runner-bound `app`'s
      // `navigationBars.buttons.firstMatch` and taps it. `firstMatch`
      // is the first (canonically left-edge) navbar button — on iOS
      // RN / UIKit this is the standard back button (auto-generated by
      // `react-navigation` or `UINavigationController`). The path is
      // i18n-safe (positional, not label-based). No back button (e.g.
      // on a root screen) returns `false`; the caller surfaces an
      // `ExpectationFailure` with a hint to verify the screen has a
      // back button.
      backHandler: {
        let app = await resolveApp()  // Per-request target-app rebind.
        // Multi-strategy back-nav fallback chain, mirroring the pattern
        // the keyboard-dismiss path uses.
        // Strategy 1: navigationBars.firstMatch — i18n-safe positional
        //   path; works for nav-stack screens.
        // Strategy 2: swipeRight from the left edge — the iOS
        //   interactive pop gesture; works for RN react-navigation
        //   Modal screens off the nav stack when the modal has
        //   `gestureEnabled: true`.
        let outcome: Bool? = smixGuarded("back") {
          // The navigation bar's identifier is the screen title, so a
          // change in it IS the "did navigate" signal. Both strategies
          // watch it instead of sleeping a fixed 0.5s and reporting
          // success unconditionally: measured on Settings, navigation
          // lands ~100ms after the tap returns, so the fixed sleep was a
          // ~5x overpay AND told the caller nothing about whether the
          // screen actually changed. Same shape as the scroll settle
          // poll below.
          let navBars = app.navigationBars
          let beforeTitle = (try? navBars.firstMatch.snapshot())?.identifier

          // A screen with no navigation bar offers no identity to watch,
          // so that case keeps the old fixed settle and the old
          // optimistic answer rather than inventing a signal. When there
          // IS a bar, only an observed change counts: a snapshot that
          // throws mid-gesture means "no reading", not "navigated" — an
          // earlier version of this returned true on the throw and so
          // reported success on a root screen with nowhere to go.
          func navigated(from previous: String?) -> Bool {
            guard let previous else {
              Thread.sleep(forTimeInterval: 0.5)
              return true
            }
            let deadline = Date().addingTimeInterval(2.0)
            while Date() < deadline {
              Thread.sleep(forTimeInterval: 0.05)
              let bar = app.navigationBars.firstMatch
              if !bar.exists { return true }
              if let now = (try? bar.snapshot())?.identifier, now != previous {
                return true
              }
            }
            return false
          }

          // Strategy 1: navigation bar back button
          let firstButton = navBars.buttons.firstMatch
          if firstButton.exists {
            firstButton.tap()
            if navigated(from: beforeTitle) { return true }
          }
          // Strategy 2: iOS interactive pop gesture (swipe right from left
          // edge). RN screens with `gestureEnabled:true` (default for stack
          // navigator screens, including Modal-presented screens) accept it.
          let leftEdge = app.coordinate(withNormalizedOffset: CGVector(dx: 0.01, dy: 0.5))
          let rightTarget = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
          leftEdge.press(forDuration: 0.1, thenDragTo: rightTarget)
          if navigated(from: beforeTitle) { return true }
          // Neither strategy moved the screen. `false` reaches the caller
          // as a 404-shaped refusal, which is the honest answer — the old
          // code returned true here without looking.
          return false
        }
        return outcome ?? false
      },
      // POST /swipe-once handler. Single XCUITest swipe gesture, no probe,
      // no selector. The driver-side host loop scrollUntilVisible
      // alternates between a host-side dict tree probe (driver.tree +
      // resolveSelector) and calls to this handler. That bypasses the
      // runner-side query.firstMatch stall on dict-only RN elements, which
      // otherwise trips FlyingFox's 15 s handler timeout and a /scroll 500.
      //
      // direction "down" = scroll the content down (revealing what is
      // below) = swipe the finger up (app.swipeUp()); symmetric for "up".
      // Matches the convention in scrollHandler.
      swipeOnceHandler: { direction, _scope in
        let app = await resolveApp()  // Per-request target-app rebind.
        let outcome: Bool? = smixGuarded("swipe-once") {
          // All 4 directions follow maestro navigation
          // convention (the wire string names what content to SEE, not
          // the finger gesture direction). swift `XCUIElement.swipe<X>`
          // primitives are the inverse finger gesture, so e.g. "down"
          // (navigate down = see below) → swipeUp (finger up = content
          // moves up). L/R follow the same navigation convention as U/D
          // and the Kotlin runner — NOT the finger-direction convention.
          switch direction {
          case "down":  app.swipeUp();    return true   // see below ← finger up
          case "up":    app.swipeDown();  return true   // see above ← finger down
          case "left":  app.swipeRight(); return true   // see left  ← finger right
          case "right": app.swipeLeft();  return true   // see right ← finger left
          default:      return false
          }
        }
        return outcome ?? false
      },
      // POST /hide-keyboard handler. Queries the runner-bound `app`'s
      // `keyboards.firstMatch`; if the keyboard is on screen, calls
      // `swipeDown()` to dismiss. Idempotent — keyboard already absent
      // is a no-op success. `firstMatch` + `.swipeDown()` is XCUITest
      // standard portable API (no private symbols). Typical use: an
      // explicit `hideKeyboard` step between a text-input fill and a
      // subsequent tap, when the on-screen keyboard would otherwise
      // mask the next target.
      hideKeyboardHandler: {
        let app = await resolveApp()  // Per-request target-app rebind.
        // Software-keyboard handling is core capability and has to be
        // robust: swipeDown alone sometimes fails to dismiss an RN
        // TextInput keyboard. Hence a multi-strategy chain that verifies
        // the keyboard is actually dismissed after each strategy.
        let outcome: Bool? = smixGuarded("hide-keyboard") {
          guard app.keyboards.firstMatch.exists else { return true }
          // Each strategy already knew how to check its own work — it
          // just slept a flat 0.5s first and then looked exactly once.
          // Polling the same check returns as soon as the keyboard is
          // actually gone (and gives a slow dismissal more than 0.5s,
          // which the old shape would have called a failure).
          func keyboardGone() -> Bool {
            let deadline = Date().addingTimeInterval(1.0)
            while Date() < deadline {
              Thread.sleep(forTimeInterval: 0.05)
              if !app.keyboards.firstMatch.exists { return true }
            }
            return false
          }
          // Strategy 1: tap Return/Done/Continue/Search/Go key on keyboard
          for keyName in ["Return", "Done", "Continue", "Search", "Go", "Next", "Enter"] {
            let key = app.keyboards.buttons[keyName]
            if key.exists {
              key.tap()
              if keyboardGone() { return true }
            }
          }
          // Strategy 2: tap above keyboard (RN Keyboard.dismiss responds to outside touch)
          let above = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15))
          above.tap()
          if keyboardGone() { return true }
          // Strategy 3: swipeDown (fallback)
          app.keyboards.firstMatch.swipeDown()
          return keyboardGone()
        }
        return outcome ?? false
      },
      // POST /input-text handler. Submits the text to whatever element
      // currently has keyboard focus via the daemon fast path — the exact
      // `_XCT_sendString` route fillHandler takes after its focus-tap. No
      // selector / no focus-tap here: the caller focused the field first
      // (FFI App.fill taps the element, then posts /input-text), mirroring
      // the Android runner's /input-text contract.
      inputTextHandler: { text in
        do {
          try await DaemonKeyboard.shared.sendString(text, typingFrequency: 200)
          // typeText APPENDS to existing field content — same cache
          // bookkeeping as fillHandler so clear's hot path stays accurate.
          KeyboardCache.shared.appendFill(text)
          return true
        } catch { return false }
      },
      // POST /tap-at-norm-coord handler. Uses the daemonProxy
      // `_XCT_synthesizeEvent:completion:` private path (the same one
      // maestro `cli-2.2.0` takes): a raw IOKit-level event that goes
      // through UIKit's standard hit-test and so fires RN Pressable
      // onPress.
      //
      // `app.coordinate(withNormalizedOffset:).tap()` does NOT work here.
      // It goes through the XCUI session middle layer, and the resulting
      // event carries XCUIElement-owner metadata that stops RN list
      // rendering from triggering its data fetch — the list sits on a
      // skeleton loader for 30 s instead of populating.
      tapAtCoordHandler: { nx, ny, times, intervalMs, holdMs in
        let app = await resolveApp()  // Per-request target-app rebind.
        // Compute the physical point (nx × app.frame.width + frame.origin)
        // on the main thread.
        var px: CGFloat = 0
        var py: CGFloat = 0
        let setupOk = smixGuarded("tap-at-norm-coord-setup") {
          let frame = app.frame
          px = frame.origin.x + frame.size.width * CGFloat(nx)
          py = frame.origin.y + frame.size.height * CGFloat(ny)
          return true
        }
        guard setupOk == true else { return (ok: false, chain: []) }

        guard let record = SmixEventRecord(orientation: .portrait) else {
          FileHandle.standardError.write(
            Data("smix-runner: tap-at-norm-coord: XCSynthesizedEventRecord unavailable\n".utf8))
          return (ok: false, chain: [])
        }
        // One record, N paths. The interval rides the event timeline
        // rather than being whatever a per-tap round trip cost.
        let pathAdded = record.addPointerTapBurst(
          at: CGPoint(x: px, y: py), times: times, intervalMs: intervalMs,
          holdMs: holdMs)
        guard pathAdded else {
          FileHandle.standardError.write(
            Data("smix-runner: tap-at-norm-coord: XCPointerEventPath unavailable\n".utf8))
          return (ok: false, chain: [])
        }
        // What the point is inside, from a fresh snapshot taken here —
        // immediately before the touch, and from the runner rather than
        // from the host's tree, which is a round trip older and would
        // only confirm the arithmetic the host already did.
        //
        // Not after the touch. A tap that opens a screen has the
        // destination under that point by the time a post-touch
        // snapshot returns, so the successful taps — the ones that
        // navigate — are exactly the ones such a check calls misses.
        // The two things it exists to catch, a target that moved since
        // the host looked and an overlay swallowing the touch, are both
        // visible in the state the touch is about to be delivered to.
        //
        // What it still cannot see: the snapshot takes time, so a screen
        // moving during it can leave the target under the point here and
        // gone by the touch, which reads as confirmed. That window is the
        // snapshot's own duration against the host round trip this
        // replaces, and it errs towards accepting a tap rather than
        // failing a working one — the opposite of the trade the
        // post-touch reading made.
        //
        // A failure to snapshot yields an empty chain rather than a
        // failed tap: the touch still happens, and the host reads an
        // empty chain as "could not be judged" rather than as a pass.
        var chain: [HitChainEntry] = []
        _ = smixGuarded("tap-at-norm-coord-chain") { () -> Bool in
          guard let snap = try? app.snapshot() else { return false }
          chain = HitChain.at(
            point: CGPoint(x: px, y: py), in: convertSnapshot(snap))
          return true
        }
        do {
          try await SmixRunnerDaemonProxy.shared.synthesize(record: record)
        } catch {
          FileHandle.standardError.write(
            Data("smix-runner: tap-at-norm-coord: synthesize error: \(error)\n".utf8))
          return (ok: false, chain: [])
        }
        return (ok: true, chain: chain)
      },
      // POST /tap-by-id handler. XCUIElement.tap() via the XCTest
      // gesture-recognizer chain for SwiftUI .sheet / .alert /
      // .confirmationDialog / .fullScreenCover dismiss buttons. The default
      // /tap-at-norm-coord path injects an IOKit-level touch at the button's
      // frame, but iOS modal-window UIWindow hit-testing routes the event to
      // the wrong target when the modal is owned by a separate window scene
      // — SwiftUI's dismiss-binding closure never fires.
      // XCUIElement.tap() goes through XCTRunnerDaemonSession against the
      // resolved element handle, so the gesture lands on the actual SwiftUI
      // hit-target regardless of window scene topology.
      tapByIdHandler: { identifier in
        let app = await resolveApp()  // Per-request target-app rebind.
        // Resolve element + swipe-scroll into view + compute the
        // post-scroll frame center. All XCUI ops sit inside smixGuarded
        // (main-thread + NSException trampoline). The actual tap dispatch
        // runs outside via the IOHID daemonProxy synthesize path so SwiftUI
        // bindings fire on iOS 17+ (XCUI coordinate-anchored tap dispatches
        // without firing Button onTap closures for non-modal Buttons —
        // observed ground truth: big29 visible after swipe-scroll
        // + Button[0.50,0.50] synthesize logged, but @State lastTapped
        // stays nil).
        var cx: CGFloat = 0
        var cy: CGFloat = 0
        let setupOk: Bool? = smixGuarded("tap-by-id-setup") {
          // Resolve by a11y identifier. Try button-typed first (fast path),
          // then fall back to descendants-any when the id sits on a non-Button
          // (Text, View container, etc).
          let resolved: XCUIElement
          let buttonEl = app.buttons[identifier]
          if buttonEl.exists || buttonEl.waitForExistence(timeout: 2) {
            resolved = buttonEl
          } else {
            let anyEl = app.descendants(matching: .any)
              .matching(NSPredicate(format: "identifier == %@", identifier))
              .firstMatch
            guard anyEl.exists || anyEl.waitForExistence(timeout: 3) else {
              return false
            }
            resolved = anyEl
          }
          // Off-screen scroll-into-view via swipe on ancestor ScrollView.
          // XCUIElement.tap() / coordinate.tap don't auto-scroll SwiftUI
          // ScrollView contents — surface as "Activation point invalid, no
          // suggested hit points". Walk all ScrollViews, swipe in the
          // direction that brings the element's frame inside the
          // scroll-view's frame, retry hittability via a fresh predicate
          // query (subscript-anchored `app.buttons[id]` caches its snapshot
          // across the swipe and reports stale isHittable=false even after
          // the element is visually scrolled into view). Max 8 swipe-rounds.
          var current = resolved
          if !current.isHittable {
            var attempts = 0
            while !current.isHittable && attempts < 8 {
              let elFrame = current.frame
              var scrolledOne = false
              for sv in app.scrollViews.allElementsBoundByIndex {
                guard sv.exists else { continue }
                let svFrame = sv.frame
                if elFrame.midX > svFrame.maxX {
                  sv.swipeLeft()
                } else if elFrame.midX < svFrame.minX {
                  sv.swipeRight()
                } else if elFrame.midY > svFrame.maxY {
                  sv.swipeUp()
                } else if elFrame.midY < svFrame.minY {
                  sv.swipeDown()
                } else {
                  continue
                }
                scrolledOne = true
                // Re-query with a fresh predicate-based descendants query;
                // subscript queries can return stale-snapshotted elements.
                current = app.descendants(matching: .any)
                  .matching(NSPredicate(format: "identifier == %@", identifier))
                  .firstMatch
                if current.isHittable { break }
              }
              if !scrolledOne { break }
              attempts += 1
            }
            guard current.isHittable else { return false }
            // XCUIElement.swipeLeft / .swipeUp etc carry inertial momentum;
            // the scroll continues briefly after the gesture call returns.
            // A fixed `Thread.sleep(1.2)` would cover deceleration before
            // snapshotting, but this uses a responsive snapshot-frame
            // stability poll instead: snapshot midX every 100ms, declare
            // settled after 2 consecutive snapshots with |Δ midX| < 0.5
            // (sub-pixel). 2.0s upper bound — slightly above the 1.2s
            // observed worst case, to absorb slow scrolls.
            // Fast scrolls settle in ~300-500ms; we no longer pay the
            // fixed 1.2s tax on the off-screen path.
            // iOS scrollView deceleration is monotonic ease-out (no bounce
            // outside over-scroll edge cases), so 2 stable polls suffice.
            var prevX: CGFloat? = nil
            var stableTicks = 0
            let settleStart = Date()
            while Date().timeIntervalSince(settleStart) < 2.0 {
              Thread.sleep(forTimeInterval: 0.1)
              guard let tickSnap = try? current.snapshot() else { break }
              let tickX = tickSnap.frame.origin.x + tickSnap.frame.size.width * 0.5
              if let p = prevX, abs(tickX - p) < 0.5 {
                stableTicks += 1
                if stableTicks >= 2 { break }
              } else {
                stableTicks = 0
              }
              prevX = tickX
            }
          }
          // Snapshot the element for a fresh ground-truth frame — `.frame`
          // on a live XCUIElement caches the pre-scroll bounds even after
          // a predicate re-query, and `.coordinate(...).screenPoint` is
          // derived from that cached frame. `snapshot()` forces a fresh
          // accessibility snapshot, so the post-scroll on-screen frame is
          // returned.
          let snap: XCUIElementSnapshot
          do {
            snap = try current.snapshot()
          } catch {
            FileHandle.standardError.write(
              Data("smix-runner: tap-by-id: snapshot error: \(error)\n".utf8))
            return false
          }
          let f = snap.frame
          cx = f.origin.x + f.size.width * 0.5
          cy = f.origin.y + f.size.height * 0.5
          return true
        }
        guard setupOk == true else { return false }
        guard let record = SmixEventRecord(orientation: .portrait) else {
          FileHandle.standardError.write(
            Data("smix-runner: tap-by-id: XCSynthesizedEventRecord unavailable\n".utf8))
          return false
        }
        let pathAdded = record.addPointerTouchEvent(at: CGPoint(x: cx, y: cy))
        guard pathAdded else {
          FileHandle.standardError.write(
            Data("smix-runner: tap-by-id: XCPointerEventPath unavailable\n".utf8))
          return false
        }
        do {
          try await SmixRunnerDaemonProxy.shared.synthesize(record: record)
          return true
        } catch {
          FileHandle.standardError.write(
            Data("smix-runner: tap-by-id: synthesize error: \(error)\n".utf8))
          return false
        }
      },
      // POST /find-text-by-ocr handler. Apple Vision OCR over the
      // current XCUIScreen screenshot. Returns the first matching observation's
      // bounding box normalized to [0,1] in UIKit coord space (top-left
      // origin, y-down). Vision's native bbox is bottom-left origin + y-up,
      // so the handler converts to UIKit. This is the sense-layer
      // fallback for a11y-less and i18n cases.
      //
      // Matching: case-insensitive substring on `topCandidates(1)` of each
      // observation. Returns the first hit (top-left-most by Vision
      // observation order). Caller can re-call with finer keyword for
      // disambiguation.
      findTextByOcrHandler: { text, locales, recognitionLevel in
        let app = await resolveApp()  // Per-request target-app rebind.
        let result: (Double, Double, Double, Double)? = await Task { @MainActor in
          // Screenshot via XCUIScreen.main (already wrapped by smixGuarded
          // pattern in /tap-by-id; OCR path doesn't trigger XCTIssue in
          // normal flow but we still wrap defensively below).
          let xcImage = XCUIScreen.main.screenshot()
          let uiImage = xcImage.image
          guard let cgImage = uiImage.cgImage else {
            FileHandle.standardError.write(
              Data("smix-runner: find-text-by-ocr: cgImage unavailable\n".utf8))
            return nil as (Double, Double, Double, Double)?
          }
          let request = VNRecognizeTextRequest()
          request.recognitionLevel = (recognitionLevel == "fast") ? .fast : .accurate
          request.usesLanguageCorrection = true
          request.recognitionLanguages = locales
          let handler = VNImageRequestHandler(cgImage: cgImage, options: [:])
          do {
            try handler.perform([request])
          } catch {
            FileHandle.standardError.write(
              Data("smix-runner: find-text-by-ocr: perform error: \(error)\n".utf8))
            return nil
          }
          guard let observations = request.results else { return nil }
          let needle = text.lowercased()
          for obs in observations {
            guard let cand = obs.topCandidates(1).first else { continue }
            if cand.string.lowercased().contains(needle) {
              // Vision bbox: bottom-left origin, y-up, normalized [0,1]
              let vbox = obs.boundingBox
              // Convert to UIKit: top-left origin, y-down
              let nx = Double(vbox.origin.x)
              let nyUiKit = 1.0 - Double(vbox.origin.y) - Double(vbox.size.height)
              let w = Double(vbox.size.width)
              let h = Double(vbox.size.height)
              return (nx, nyUiKit, w, h)
            }
          }
          return nil
        }.value
        return result
      },
      // GET /screenshot handler. PNG of the whole screen.
      //
      // Same call the OCR handler above makes — `XCUIScreen.main
      // .screenshot()` — differing only in what it hands back: pixels
      // rather than a verdict about them. That it works on a physical
      // device is the point of the route; simulators have `simctl io
      // screenshot` and phones had nothing.
      //
      // No `resolveApp()`: this photographs the screen, not the target
      // app, so there is no bundle to rebind to.
      //
      // Unit-tested up to the envelope (ScreenshotRouteTests) and no
      // further — XCUIScreen exists only in this host, so the pixels
      // themselves are proven by e2e against a real device, not here.
      // Saying so rather than implying coverage that does not exist.
      screenshotHandler: {
        await Task { @MainActor in
          XCUIScreen.main.screenshot().image.pngData()
        }.value
      },
      // POST /swipe-at-norm-coord handler. From-to coordinate swipe
      // via Apple native event chain (XCSynthesizedEventRecord +
      // XCPointerEventPath initForTouchAtPoint → moveToPoint:atOffset:
      // → liftUpAtOffset:). Normalized-coordinate escape-hatch sibling of
      // tap-at-norm-coord. Same daemonProxy dispatch path; same setup-time
      // app.frame coord conversion as tap.
      swipeAtCoordHandler: { fromNx, fromNy, toNx, toNy in
        let app = await resolveApp()  // Per-request target-app rebind.
        var fromPx: CGFloat = 0
        var fromPy: CGFloat = 0
        var toPx: CGFloat = 0
        var toPy: CGFloat = 0
        let setupOk = smixGuarded("swipe-at-norm-coord-setup") {
          let frame = app.frame
          fromPx = frame.origin.x + frame.size.width * CGFloat(fromNx)
          fromPy = frame.origin.y + frame.size.height * CGFloat(fromNy)
          toPx = frame.origin.x + frame.size.width * CGFloat(toNx)
          toPy = frame.origin.y + frame.size.height * CGFloat(toNy)
          return true
        }
        guard setupOk == true else { return false }

        guard let record = SmixEventRecord(orientation: .portrait) else {
          FileHandle.standardError.write(
            Data("smix-runner: swipe-at-norm-coord: XCSynthesizedEventRecord unavailable\n".utf8))
          return false
        }
        let pathAdded = record.addPointerSwipeEvent(
          from: CGPoint(x: fromPx, y: fromPy),
          to: CGPoint(x: toPx, y: toPy)
        )
        guard pathAdded else {
          FileHandle.standardError.write(
            Data("smix-runner: swipe-at-norm-coord: XCPointerEventPath unavailable\n".utf8))
          return false
        }
        do {
          try await SmixRunnerDaemonProxy.shared.synthesize(record: record)
          return true
        } catch {
          FileHandle.standardError.write(
            Data("smix-runner: swipe-at-norm-coord: synthesize error: \(error)\n".utf8))
          return false
        }
      },
      // POST /double-tap handler. XCUIElement.doubleTap() public API.
      // Selector resolution follows the tap handler (NSPredicate
      // label|identifier), but does NOT accept see-through (modal scope):
      // see-through is a tap-specific path. Double-tap is single-armed —
      // NSPredicate descendants match → first hit. Not found ⇒ stderr +
      // false (notFound).
      doubleTapHandler: { selector in
        let app = await resolveApp()  // Per-request target-app rebind.
        return smixGuarded("double-tap") { () -> Bool in
          let predicate = Self.predicate(for: selector)
          let element = app.descendants(matching: .any)
            .matching(predicate)
            .firstMatch
          if !element.exists {
            FileHandle.standardError.write(
              Data("smix-runner: double-tap: element not found for \(selector.wireKey)=\(selector.raw)\n".utf8))
            return false
          }
          element.doubleTap()
          return true
        } ?? false
      },
      // POST /long-press handler.
      //
      // `XCUIElement.press(forDuration:)` is deliberately NOT used. It
      // was measured on iPhone 17 Pro / iOS 26.5 taking a constant
      // ~2.6s round trip for every requested hold from 500ms to
      // 6000ms, while `/tap` on the same selector took 156ms and
      // `/find` 1.7ms — so the cost is inside the press, and the hold
      // it performs bears no relation to the one asked for. Every
      // `longPressOn: { duration: N }` written against this runner got
      // the same gesture regardless of N.
      //
      // Synthesising the touch puts the timeline in this process:
      // touch down at offset 0, lift at offset `durationMs`. That is
      // the same mechanism `repeatTap` already rides, and it is what
      // makes the reported bounds mean anything.
      longPressHandler: { selector, durationMs in
        let entryMs = Date().timeIntervalSince1970 * 1000.0
        let app = await resolveApp()  // Per-request target-app rebind.
        var centre = CGPoint.zero
        let resolved = smixGuarded("long-press-resolve") { () -> Bool in
          let predicate = Self.predicate(for: selector)
          let element = app.descendants(matching: .any)
            .matching(predicate)
            .firstMatch
          if !element.exists {
            FileHandle.standardError.write(
              Data("smix-runner: long-press: element not found for \(selector.wireKey)=\(selector.raw)\n".utf8))
            return false
          }
          let f = element.frame
          centre = CGPoint(x: f.midX, y: f.midY)
          return true
        }
        guard resolved == true else { return nil }

        guard let record = SmixEventRecord(orientation: .portrait) else {
          FileHandle.standardError.write(
            Data("smix-runner: long-press: XCSynthesizedEventRecord unavailable\n".utf8))
          return nil
        }
        guard record.addPointerTapBurst(
          at: centre, times: 1, intervalMs: 0, holdMs: Int(durationMs)) else {
          FileHandle.standardError.write(
            Data("smix-runner: long-press: XCPointerEventPath unavailable\n".utf8))
          return nil
        }
        let callStartMs = Date().timeIntervalSince1970 * 1000.0
        do {
          try await SmixRunnerDaemonProxy.shared.synthesize(record: record)
        } catch {
          FileHandle.standardError.write(
            Data("smix-runner: long-press: synthesize failed: \(error)\n".utf8))
          return nil
        }
        let callEndMs = Date().timeIntervalSince1970 * 1000.0
        return LongPressRoute.PressTimings.around(
          callStartMs: callStartMs, callEndMs: callEndMs,
          holdMs: durationMs, handlerEntryMs: entryMs)
      },
      // POST /set-orientation handler. XCUIDevice.shared.orientation
      // public XCUI API. orientation literal aligned with iOS UIDeviceOrientation
      // raw values via switch mapping.
      setOrientationHandler: { orientationStr in
        return smixGuarded("set-orientation") { () -> Bool in
          let mapped: UIDeviceOrientation
          switch orientationStr {
          case "portrait": mapped = .portrait
          case "portraitUpsideDown": mapped = .portraitUpsideDown
          case "landscapeLeft": mapped = .landscapeLeft
          case "landscapeRight": mapped = .landscapeRight
          default:
            FileHandle.standardError.write(
              Data("smix-runner: set-orientation: unknown orientation '\(orientationStr)'\n".utf8))
            return false
          }
          XCUIDevice.shared.orientation = mapped
          // settle: orientation change is asynchronous in sim;
          // 200ms aligns with iOS rotation animation budget.
          Thread.sleep(forTimeInterval: 0.2)
          return true
        } ?? false
      },
      recordHandlers: recordEnabled ? SmixRunnerServer.RecordHandlers(
        start: {
          EventRecorder.shared.installSwizzle(appBundleId: bundleId)
          EventRecorder.shared.start()
        },
        stop: {
          return EventRecorder.shared.stop()
        },
        poll: {
          return EventRecorder.shared.drain()
        }
      ) : nil,
      sessionHandlers: SmixRunnerServer.SessionHandlers(
        open: { req in
          // Bind (or rebind) an XCUIApplication for the requested
          // bundle. If the client asked for the runner's boot-time
          // default bundle id, reuse the existing `app` instance so
          // its cached test-driver state carries over; otherwise
          // spin up a fresh XCUIApplication.
          let target: XCUIApplication = (req.bundleId == bundleId)
            ? app
            : XCUIApplication(bundleIdentifier: req.bundleId)
          var activatedOnce = false
          if req.activate {
            activatedOnce = await maybeActivate(req.bundleId, target)
          }
          let sid = UUID().uuidString
          let now = Date()
          let entry = SessionEntry(
            bundleId: req.bundleId,
            app: target,
            lastActivatedAt: now,
            lastAccessedAt: now
          )
          sessions.lock()
          sessionTable[sid] = entry
          persistSessions()
          sessions.unlock()
          lifecycleCounters.advance { $0.openedTotal &+= 1 }
          return SmixRunnerServer.SessionOpenOutcome(
            sessionId: sid,
            activatedOnce: activatedOnce
          )
        },
        close: { req in
          sessions.lock()
          sessionTable.removeValue(forKey: req.sessionId)
          persistSessions()
          sessions.unlock()
          lifecycleCounters.advance { $0.closedTotal &+= 1 }
          // Idempotent — unknown session id → ok=true anyway.
          return SmixRunnerServer.SessionCloseOutcome(ok: true)
        },
        renew: { req in
          sessions.lock()
          let entry = sessionTable[req.sessionId]
          sessions.unlock()
          guard var entry = entry else {
            return SmixRunnerServer.SessionRenewOutcome(
              notFound: true, ok: false, activated: false
            )
          }
          let now = Date()
          if now.timeIntervalSince(entry.lastActivatedAt) < renewCooldown {
            // Rate-limited — session known but no-op.
            return SmixRunnerServer.SessionRenewOutcome(
              notFound: false, ok: true, activated: false
            )
          }
          await SmixRunnerServer.onMain {
            entry.app.activate()
          }
          entry.lastActivatedAt = now
          sessions.lock()
          sessionTable[req.sessionId] = entry
          sessions.unlock()
          return SmixRunnerServer.SessionRenewOutcome(
            notFound: false, ok: true, activated: true
          )
        },
        // Close every open session in the table. Used
        // by `smix runner cycle` and the supervisor auto-restart.
        // Idempotent; returns the count that was cleared.
        closeAll: {
          sessions.lock()
          let count = sessionTable.count
          sessionTable.removeAll()
          persistSessions()
          sessions.unlock()
          return SmixRunnerServer.SessionCloseAllOutcome(closed: count)
        },
        // Terminate+launch the session's cached app in
        // place. Preserves session id and XCUITest binding — no
        // uninstall/install, no cross-session churn.
        relaunchApp: { req in
          let start = Date()
          sessions.lock()
          let entry = sessionTable[req.sessionId]
          sessions.unlock()
          guard let entry = entry else {
            return SmixRunnerServer.SessionRelaunchOutcome(
              notFound: true, ok: false, wallMs: 0
            )
          }
          await SmixRunnerServer.onMain {
            entry.app.terminate()
            entry.app.launch()
          }
          let wallMs = UInt64(Date().timeIntervalSince(start) * 1000)
          lifecycleCounters.advance { $0.relaunchAppTotal &+= 1 }
          return SmixRunnerServer.SessionRelaunchOutcome(
            notFound: false, ok: true, wallMs: wallMs
          )
        },
        // Enumerate every session. Snapshot the table
        // under lock, then map to summaries outside.
        list: {
          sessions.lock()
          let summaries: [SessionRoute.SessionSummary] = sessionTable.map { (sid, entry) in
            let ms = UInt64(entry.lastActivatedAt.timeIntervalSince1970 * 1000)
            return SessionRoute.SessionSummary(
              sessionId: sid,
              bundleId: entry.bundleId,
              openedAtMs: ms,
              lastActivatedAtMs: ms,
              interactiveNamedIds: entry.lastInteractiveNamedIds
            )
          }
          sessions.unlock()
          return SmixRunnerServer.SessionListOutcome(sessions: summaries)
        },
        // Diagnostic snapshot. Runner side does not
        // shell out to `simctl` (that's the CLI process); recent
        // subprocesses stay empty on this side, and the CLI merges
        // its own client-side ring on top when it prints.
        //
        // Also snapshot the app-alive cache counters
        // (when the runner was booted with one). A log line cannot
        // distinguish "re-probe never fired" from "log line got dropped by
        // a cycle"; the counters make that a numeric question, and they
        // survive a runner cycle. `nil` when the runner opted out of
        // app-alive caching.
        // Direct-capture `localAppAliveCache` rather than the
        // task-local, and emit cumulative session lifecycle counters.
        // Always emit `aliveCache` with the
        // `wired: true` sentinel so consumers can distinguish "runner
        // has no cache" from "cache present, workload didn't fire".
        diagnostic: {
          sessions.lock()
          let summaries: [SessionRoute.SessionSummary] = sessionTable.map { (sid, entry) in
            let ms = UInt64(entry.lastActivatedAt.timeIntervalSince1970 * 1000)
            return SessionRoute.SessionSummary(
              sessionId: sid,
              bundleId: entry.bundleId,
              openedAtMs: ms,
              lastActivatedAtMs: ms,
              interactiveNamedIds: entry.lastInteractiveNamedIds
            )
          }
          sessions.unlock()
          let uptimeMs = UInt64(Date().timeIntervalSince(bootAt) * 1000)
          let c = await localAppAliveCache.counterSnapshot()
          let aliveCache = SessionRoute.AliveCacheCounters(
            wired: true,
            markDeadTotal: c.markDeadTotal,
            markAliveTotal: c.markAliveTotal,
            suppressHitTotal: c.suppressHitTotal,
            suppressMissTotal: c.suppressMissTotal,
            reprobeAttemptedTotal: c.reprobeAttemptedTotal,
            reprobeSucceededTotal: c.reprobeSucceededTotal,
            reprobeInvalidatedEarly: c.reprobeInvalidatedEarly,
            reprobeExhaustedWindow: c.reprobeExhaustedWindow
          )
          let sessionCounters = lifecycleCounters.snapshot()
          let lastInteractiveIds = lastInteractiveIdsBox.snapshot()
          return SmixRunnerServer.DiagnosticOutcome(
            snapshot: SessionRoute.DiagnosticSnapshot(
              sessions: summaries,
              simHealth: "healthy",
              supervisorPid: nil,
              uptimeMs: uptimeMs,
              aliveCache: aliveCache,
              sessionCounters: sessionCounters,
              lastInteractiveNamedIds: lastInteractiveIds
            )
          )
        },
        // Cooperative XCUIApplication.terminate() via
        // testmanagerd. Does NOT signal com.apple.ReportCrash — which is
        // what keeps the OS from showing an "<App> quit unexpectedly"
        // crash dialog. `simctl terminate` sends SIGKILL and triggers
        // ReportCrash; XCUIApplication.terminate() is the graceful
        // pathway. Paired with `launchApp` below via SDK-side
        // orchestration + host-side simctl sandbox wipe.
        // Cooperative XCUIApplication.terminate()
        // via testmanagerd. Detect cooperative outcome by checking that
        // `.state == .notRunning` after the call returned within a
        // reasonable window. If it doesn't, XCUIApplication likely
        // timed out and internally fell back to a hard kill — which
        // triggers `bug_type: 309` `.ips` writes.
        terminateApp: { req in
          let start = Date()
          sessions.lock()
          let entry = sessionTable[req.sessionId]
          sessions.unlock()
          guard let entry = entry else {
            return SmixRunnerServer.SessionAppLifecycleOutcome(
              notFound: true, ok: false, wallMs: 0
            )
          }
          let terminatedCooperatively: Bool = await SmixRunnerServer.onMain {
            entry.app.terminate()
            // `.terminate()` is blocking; XCUIApplication waits until
            // the process observed `.notRunning` OR the framework's
            // internal ~30 s timeout. Reading `.state` here tells us
            // which happened.
            return entry.app.state == .notRunning
          }
          let wallMs = UInt64(Date().timeIntervalSince(start) * 1000)
          lifecycleCounters.advance {
            $0.terminateAppTotal &+= 1
            if terminatedCooperatively {
              $0.terminateAppViaXCUIApplication &+= 1
            } else {
              $0.terminateAppViaFallback &+= 1
            }
          }
          let terminalState: UInt8 = await SmixRunnerServer.onMain {
            UInt8(entry.app.state.rawValue)
          }
          return SmixRunnerServer.SessionAppLifecycleOutcome(
            notFound: false,
            ok: true,
            wallMs: wallMs,
            waitedMs: 0,
            terminalState: terminalState,
            terminatedCooperatively: terminatedCooperatively
          )
        },
        // Cooperative `XCUIApplication.launch()`. Applies
        // request-supplied `launchArguments` + `launchEnvironment` before
        // launching so callers can bypass scaffolding like the Expo
        // dev-launcher server picker (SDK 57 stopped auto-navigating on a
        // URL scheme). Polls `.state` for
        // `.runningForeground` up to `waitForForegroundMs` so the
        // caller's next terminate does not hit a not-yet-ready
        // process — the trigger for `bug_type: 309` `.ips` writes.
        launchApp: { req in
          let start = Date()
          sessions.lock()
          let entry = sessionTable[req.sessionId]
          sessions.unlock()
          guard let entry = entry else {
            return SmixRunnerServer.SessionAppLifecycleOutcome(
              notFound: true, ok: false, wallMs: 0
            )
          }
          await SmixRunnerServer.onMain {
            // Empty args / empty env leave whatever the runner had
            // cached, which is initially empty.
            entry.app.launchArguments = req.args
            entry.app.launchEnvironment = req.env
            entry.app.launch()
          }
          let launchDoneWallMs = UInt64(Date().timeIntervalSince(start) * 1000)
          var waitedMs: UInt64 = 0
          var reachedForeground = false
          if let deadlineMs = req.waitForForegroundMs, deadlineMs > 0 {
            let waitStart = Date()
            let pollIntervalNs: UInt64 = 250_000_000  // 250 ms
            while UInt64(Date().timeIntervalSince(waitStart) * 1000) < deadlineMs {
              let state = await SmixRunnerServer.onMain { entry.app.state }
              if state == .runningForeground {
                reachedForeground = true
                break
              }
              try? await Task.sleep(nanoseconds: pollIntervalNs)
            }
            waitedMs = UInt64(Date().timeIntervalSince(waitStart) * 1000)
          }
          let terminalState: UInt8 = await SmixRunnerServer.onMain {
            UInt8(entry.app.state.rawValue)
          }
          // Interactive fingerprint probe.
          // After foreground is observed (or immediately if the
          // caller didn't request a foreground wait), poll the a11y
          // tree at 500 ms cadence looking for ≥ minIdentifierCount
          // descendants with a non-empty accessibilityIdentifier
          // that isn't in the interactive-probe ignore-list.
          //
          // Fires `reachedInteractive = true` on first match; capture
          // up to 8 observed ax-ids for debug attribution. On
          // timeout: reachedInteractive stays false, counter
          // `launchAppTimedOutBeforeInteractive` +1. Per Q8 answer
          // (a): `launchApp` still returns success either way —
          // consumer detects the "up but unusable" state via the
          // counter delta, not via a hard failure.
          //
          // Ignore-list + minIdentifierCount come from
          // `.smix/config.yaml interactiveProbe: {...}` forwarded to
          // the runner via SMIX_INTERACTIVE_PROBE_JSON. Missing env
          // = defaults: minIdentifierCount 3, ignore [SplashScreenLogo].
          var reachedInteractive = false
          var interactiveNamedIds: [String] = []
          if let interactiveDeadlineMs = req.waitForInteractiveMs,
             interactiveDeadlineMs > 0 {
            let probeConfig = InteractiveProbeConfig.fromEnv()
            // The target app's own bundle id never counts as
            // interactivity evidence: the application root node carries
            // identifier == bundleId on EVERY app, so leaving it in the
            // sample inflates the count by one for free. Merge it into
            // the ignore set dynamically instead of asking each
            // consumer to configure their own bundle id away.
            let effectiveIgnore = probeConfig.ignore.union([entry.bundleId])
            let interactiveStart = Date()
            let interactivePollNs: UInt64 = 500_000_000  // 500 ms
            while UInt64(Date().timeIntervalSince(interactiveStart) * 1000) < interactiveDeadlineMs {
              let observed: [String] = await SmixRunnerServer.onMain {
                // Enumerating via
                // `descendants(matching:).element(boundBy: i)` does NOT
                // work here, even with a fresh snapshot each iteration:
                // that call is XCTest-lazy, so the element resolves at
                // access time against the CURRENT (possibly
                // re-snapshotted) tree. When the tree shrinks
                // mid-iteration — a stopApp followed by an openLink into
                // the dev-launcher will do it — XCTest raises the
                // unrecoverable assertion "No matches found for Element
                // at index N", killing test_runForever.
                //
                // Walking the frozen `XCUIElementSnapshot` instead is
                // safe: snap is an in-memory object, so there is no
                // XCUITest re-resolution during the walk and a shrinking
                // tree between iterations cannot crash it. This is the
                // same pattern the runner uses for modal popup collection
                // (see collectPopupNodes below) and
                // FocusedIdentifier.find. Taking the snapshot still
                // forces XCUITest to re-scrape the a11y hierarchy from
                // scratch, which is what keeps each iteration fresh.
                guard let snap = try? entry.app.snapshot() else {
                  return []
                }
                var ids: [String] = []
                var enumerated = 0
                let enumCap = 200  // pathological-tree stall guard
                collectInteractiveIds(
                  snap.dictionaryRepresentation,
                  ignore: effectiveIgnore,
                  ids: &ids,
                  enumerated: &enumerated,
                  cap: enumCap
                )
                return ids
              }
              if observed.count >= probeConfig.minIdentifierCount {
                reachedInteractive = true
                // Sample up to 8 to keep the wire small.
                interactiveNamedIds = Array(observed.prefix(8))
                break
              }
              try? await Task.sleep(nanoseconds: interactivePollNs)
            }
          }
          lifecycleCounters.advance {
            $0.launchAppTotal &+= 1
            if req.waitForForegroundMs != nil && req.waitForForegroundMs != 0 {
              if reachedForeground {
                $0.launchAppReachedForeground &+= 1
              } else {
                $0.launchAppTimedOutBeforeForeground &+= 1
              }
            }
            if let ms = req.waitForInteractiveMs, ms > 0 {
              if reachedInteractive {
                $0.launchAppReachedInteractive &+= 1
              } else {
                $0.launchAppTimedOutBeforeInteractive &+= 1
              }
            }
          }
          // Persist interactiveNamedIds on the session so
          // list/diagnostic surfaces the WHICH-ids, not just the count.
          // Update happens on every launch (both success and timeout).
          // On timeout, `interactiveNamedIds` is empty — that's the
          // signal to the consumer that the gate didn't fire.
          sessions.lock()
          if var entry = sessionTable[req.sessionId] {
            entry.lastInteractiveNamedIds = interactiveNamedIds
            sessionTable[req.sessionId] = entry
            persistSessions()
          }
          sessions.unlock()
          // Mirror non-empty samples into the runner-scope
          // box so `/diagnostic/dump.runner.lastInteractiveNamedIds`
          // survives session close.
          lastInteractiveIdsBox.update(interactiveNamedIds)
          return SmixRunnerServer.SessionAppLifecycleOutcome(
            notFound: false,
            ok: true,
            wallMs: launchDoneWallMs,
            waitedMs: waitedMs,
            terminalState: terminalState,
            terminatedCooperatively: false,
            reachedInteractive: reachedInteractive,
            interactiveNamedIds: interactiveNamedIds
          )
        }
      ),
      // Per-session `/system-popups` 500 ms floor. Hard-
      // wired at 500 ms because the failure mode is a runaway poll
      // loop (~1.7 QPS × 6 XCUIQuery); no consumer benefit at faster
      // than 500 ms cadence and the arbitration cost is decisive.
      popupPacer: PopupPacer(floorMs: 500),
      // 20 s app-alive suppression window after an observed XCTIssue
      // about the target app. The cache is instantiated into a named
      // local `localAppAliveCache` so the diagnostic handler can
      // direct-capture it, rather than reading it via the task-local —
      // which does not propagate through FlyingFox's per-request spawn.
      appAliveCache: localAppAliveCache,
      // Initially `.healthy`; downgraded by the runner
      // supervisor as it observes SimRenderServer / xcodebuild
      // signals or by handlers that catch specific error shapes.
      simHealthPublisher: SimHealthPublisher(initial: .healthy),
      // Categorization for /tree unavailable envelope. UITest-scope
      // logic: read `XCUIApplication.state` and say what it says.
      //
      // The request does not always name a bundle — `smix describe`
      // sends no `App-Bundle-Id` header — and this used to answer
      // `unknown` in that case, from a runner that was started with
      // `--bundle` and has known which app it drives the whole time.
      // The answer was "I have no idea" while the fact sat in scope.
      //
      // An earlier comment here claimed this scans
      // ~/Library/Logs/DiagnosticReports/ for a recent .ips. It does
      // not, and it cannot: this code runs inside the simulator, and
      // that directory is on the host.
      unavailableReasonInferer: { requestBundleId in
        let target = requestBundleId.flatMap { $0.isEmpty ? nil : $0 } ?? bundleId
        guard !target.isEmpty else { return .unknown }
        let state = await SmixRunnerServer.onMain {
          XCUIApplication(bundleIdentifier: target).state
        }
        switch state {
        case .notRunning:
          return .notRunning
        case .runningForeground, .runningBackground, .runningBackgroundSuspended:
          return .aliveButTreeEmpty
        case .unknown:
          return .unknown
        @unknown default:
          return .unknown
        }
      },
      // App-rebind half of `POST /soft-cycle`. Terminate + launch the
      // runner-bound app fresh — the SAME semantics `up()`'s boot
      // `app.launch()` gives a hard cycle — so a soft cycle is a faster
      // equivalent, not a weaker one (an activate-only rebind would leave
      // the app in its prior state, silently narrowing what `cycle`
      // means). The FlyingFox bounce that follows is the runner core's
      // concern (`runServerLoop`).
      softCycleHandler: {
        await SmixRunnerServer.onMain {
          app.terminate()
          app.launch()
        }
        return SmixRunnerServer.SoftCycleOutcome(rebound: true, mode: "relaunch")
      }
    )
  }
}

/// Recursive walk over
/// `XCUIElementSnapshot.dictionaryRepresentation` to collect every
/// non-empty `accessibilityIdentifier` in the tree that is NOT in the
/// caller-supplied ignore list. Same pattern as
/// [`collectPopupNodes`] below — nothing about it re-invokes XCUITest,
/// so a tree that shrinks or grows between the outer poll iterations
/// can't crash the loop.
///
/// The `enumerated`/`cap` counter is a pathological-tree stall guard
/// (large lists could otherwise hold the run loop for seconds); once
/// hit we stop walking, but the accumulated `ids` up to that point
/// are still valid.
private func collectInteractiveIds(
  _ node: [XCUIElement.AttributeName: Any],
  ignore: Set<String>,
  ids: inout [String],
  enumerated: inout Int,
  cap: Int
) {
  if enumerated >= cap { return }
  enumerated += 1
  func val(_ k: String) -> Any? { node[XCUIElement.AttributeName(rawValue: k)] }
  if let id = val("identifier") as? String, !id.isEmpty, !ignore.contains(id) {
    ids.append(id)
  }
  if let kids = val("children") as? [[XCUIElement.AttributeName: Any]] {
    for k in kids {
      collectInteractiveIds(k, ignore: ignore, ids: &ids, enumerated: &enumerated, cap: cap)
      if enumerated >= cap { return }
    }
  }
}

/// Recursively walk `XCUIElementSnapshot.dictionaryRepresentation` in
/// memory, collecting every button (label / identifier) and staticText
/// (label) in a popup container's subtree.
///
/// This replaces per-element live `.label` / `.exists` / `.identifier`
/// access: while a modal alert is up each of those costs ~1.2 s, so N
/// elements accumulate past FlyingFox's 15 s socket timeout and hang the
/// runner's main thread. After a single `container.snapshot()` everything
/// here is memory, making the cost independent of the element count N.
///
/// Note that dictionaryRepresentation does NOT expose
/// userTestingAttributes (measured: absent; the dict carries only
/// displayID / elementType / enabled / frame / identifier / label / title
/// / hasFocus / selected / sizeClass / windowContextID). Role
/// classification (cancel / destructive) is therefore not collected here —
/// collectSystemPopups fills it in via a live predicate query, and only
/// for SpringBoard native alerts.
///
/// XCUIElement.ElementType raw values: button=9, staticText=48 — the same
/// enum numbering space as the alert=7 / sheet=5 / dialog=8 / popover=18
/// values used in collectSystemPopups' inline switch.
private func collectPopupNodes(
  _ node: [XCUIElement.AttributeName: Any],
  buttons: inout [(label: String, id: String, frame: CGRect)],
  texts: inout [String]
) {
  func val(_ k: String) -> Any? { node[XCUIElement.AttributeName(rawValue: k)] }
  let etRaw = (val("elementType") as? Int) ?? 0
  let label = (val("label") as? String) ?? ""
  if etRaw == 9 {
    let id = (val("identifier") as? String) ?? ""
    let frame = frameFromDictValue(val("frame"))
    buttons.append((label: label, id: id, frame: frame))
  } else if etRaw == 48 {
    texts.append(label)
  }
  if let kids = val("children") as? [[XCUIElement.AttributeName: Any]] {
    for k in kids { collectPopupNodes(k, buttons: &buttons, texts: &texts) }
  }
}

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
private func convertSnapshot(
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
private func convertSnapshotDict(
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
  // Match the live first-responder identifier (the one-shot KVC walk done
  // in snapshotHandler) against this node's identifier. `focusHint == nil`
  // ⇒ no node is focused (typically the keyboard is not up). The
  // identifier must be non-empty to be tagged, so that a nil hint cannot
  // empty-string-match a node that has no identifier.
  let hasFocus = focusHint.map { hint in
    !identifier.isEmpty && hint == identifier
  } ?? false
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
private func buildAllWindowsSnapshot(
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
    children.append(convertSnapshot(appSnap, focusHint: FocusedIdentifier.find(in: appSnap)))
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

private func collectSpringBoardWindows() -> [TreeRoute.A11ySnapshotData] {
  var out: [TreeRoute.A11ySnapshotData] = []
  let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
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

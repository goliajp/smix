import FlyingFox
import Foundation

#if canImport(CoreGraphics)
import CoreGraphics
#endif

// Wraps FlyingFox HTTPServer so the XCUITest runner does not need to import
// FlyingFox directly — it links SmixRunnerCore only. Subsequent checkpoints
// register more routes on this server (screenshot/swipe/...).
// One-shot graceful-shutdown signal. `/shutdown` fires it; the
// stop-observer task in `runForever` awaits it then calls `server.stop()`.
// Repeated `fire()` is idempotent (continuation resumed at most once) so a
// double POST /shutdown cannot crash. Never fired (no /shutdown call) →
// `wait()` suspends forever → `server.run()` is never stopped → `runForever`
// blocks exactly as it would without the signal (purely additive opt-in).
// Minimal JSON-string escape for selector field re-serialization
// inside the /scroll route handler. Same character set as
// ScrollRoute.jsonEscape (control + quote + backslash). Local helper because
// the route handler reassembles a {"text":"..."} JSON literal to forward as
// a single string parameter to the UITest closure (the closure layer parses
// it back; mirrors how findHandler/fillHandler receive their selectorText
// today as a plain string — one wire form per direction).
private func jsonEscapeSelector(_ s: String) -> String {
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

actor ShutdownSignal {
  private var fired = false
  private var waiters: [CheckedContinuation<Void, Never>] = []

  var didFire: Bool { fired }

  func fire() {
    if fired { return }
    fired = true
    let pending = waiters
    waiters.removeAll()
    for w in pending { w.resume() }
  }

  func wait() async {
    if fired { return }
    await withCheckedContinuation { (c: CheckedContinuation<Void, Never>) in
      waiters.append(c)
    }
  }
}

/// Per-session `/system-popups` request pacer.
///
/// The XCTest arbitration cost of `/system-popups` is 6 accessibility
/// query descents (Alert × 2, Sheet × 2, Dialog, Popover) per call. An
/// unpaced consumer has been measured driving the endpoint at ~1.7 QPS
/// across a 340 s bootstrap flow — 3400+ XCUIQuery executions on top of
/// every other route, contributing to the arbitration exhaustion that
/// preceded a SimRenderServer `brk 1` trip. This actor enforces a 500 ms
/// floor per session id (or `"default"` for header-less callers);
/// too-soon calls return `SystemPopupsRoute.tooManyRequests`.
///
/// Business-unaware. The runtime routes-side wraps it around the
/// `systemPopupsHandler` closure; consumers see a 429 when they exceed
/// the floor and back off per `Retry-After`.
public actor PopupPacer {
  private var lastCallAt: [String: Date] = [:]
  private let floor: TimeInterval

  public init(floorMs: Int = 500) {
    self.floor = TimeInterval(floorMs) / 1000.0
  }

  /// Try to take a slot for `sessionKey`. Returns the elapsed ms
  /// remaining before the next slot on failure; `nil` on success.
  public func tryTake(sessionKey: String) -> Int? {
    let now = Date()
    if let last = lastCallAt[sessionKey] {
      let elapsed = now.timeIntervalSince(last)
      if elapsed < floor {
        let remainMs = Int(((floor - elapsed) * 1000).rounded(.up))
        return remainMs
      }
    }
    lastCallAt[sessionKey] = now
    return nil
  }
}

/// Per-bundle app-alive cache.
///
/// The runner's XCUIElement query layer surfaces "Application X is not
/// running" via `XCTIssue` when the target app has crashed. Without a
/// suppression window, the runner keeps firing 6 XCUIQuery descents per
/// `/system-popups` and full a11y snapshots per `/tree` even though
/// every one will return the same failure — an arbitration firehose
/// with no product-side benefit. This actor records the bundle-id →
/// dead-until timestamp; upstream routes short-circuit during the
/// window and return empty snapshots without hitting XCUITest.
///
/// Invalidation: any subsequent `/sim/launch` or `/session/open` for
/// the same bundle-id clears the entry (the caller is asking to
/// re-establish the target).
public actor AppAliveCache {
  private var deadUntil: [String: Date] = [:]
  private let ttl: TimeInterval

  // Observability counters. A stderr log line alone cannot prove a code
  // path fired: a zero grep count is ambiguous between "path not fired"
  // and "log line dropped mid-cycle". Each mutation advances a counter
  // instead, and the counters flow through /diagnostic/dump, so "is it
  // working" becomes a numeric check.
  private var counters = Counters()

  public struct Counters: Sendable {
    /// Total calls to [`markDead`].
    public var markDeadTotal: UInt64 = 0
    /// Total calls to [`markAlive`] (both explicit unblock + re-probe).
    public var markAliveTotal: UInt64 = 0
    /// `isSuppressed` returned true (suppressed a call).
    public var suppressHitTotal: UInt64 = 0
    /// `isSuppressed` returned false (allowed a call).
    public var suppressMissTotal: UInt64 = 0
    /// A background re-probe task was spawned.
    /// Incremented by the runner-side XCTIssue observer when it
    /// spawns the polling Task.
    public var reprobeAttemptedTotal: UInt64 = 0
    /// A re-probe iteration observed the app return to running →
    /// markAlive was called from within the re-probe loop.
    public var reprobeSucceededTotal: UInt64 = 0
    /// The re-probe was invalidated mid-flight because someone else
    /// called markAlive (e.g. a fresh /session/open).
    public var reprobeInvalidatedEarly: UInt64 = 0
    /// The re-probe exhausted all 6 polling iterations without seeing
    /// the app come back — the suppression stayed and the caller is
    /// blocked.
    public var reprobeExhaustedWindow: UInt64 = 0
  }

  public init(ttlMs: Int = 20_000) {
    self.ttl = TimeInterval(ttlMs) / 1000.0
  }

  /// Mark the bundle-id as "known dead" for `ttl` seconds from now.
  public func markDead(bundleId: String) {
    deadUntil[bundleId] = Date().addingTimeInterval(ttl)
    counters.markDeadTotal &+= 1
  }

  /// Invalidate a previous "dead" mark for the given bundle. Called
  /// on `/sim/launch` and `/session/open` for the same bundle id.
  public func markAlive(bundleId: String) {
    deadUntil.removeValue(forKey: bundleId)
    counters.markAliveTotal &+= 1
  }

  /// Returns true when the caller should short-circuit XCUITest for
  /// this bundle.
  public func isSuppressed(bundleId: String) -> Bool {
    if let until = deadUntil[bundleId] {
      if Date() < until {
        counters.suppressHitTotal &+= 1
        return true
      }
      deadUntil.removeValue(forKey: bundleId)
    }
    counters.suppressMissTotal &+= 1
    return false
  }

  /// Re-probe telemetry. Called by the XCTIssue observer when it
  /// spawns a background alive-check Task.
  public func noteReprobeAttempted() {
    counters.reprobeAttemptedTotal &+= 1
  }

  /// Re-probe observed the app return to running.
  public func noteReprobeSucceeded() {
    counters.reprobeSucceededTotal &+= 1
  }

  /// Re-probe was interrupted by an external markAlive.
  public func noteReprobeInvalidatedEarly() {
    counters.reprobeInvalidatedEarly &+= 1
  }

  /// Re-probe iterations exhausted without recovery.
  public func noteReprobeExhaustedWindow() {
    counters.reprobeExhaustedWindow &+= 1
  }

  /// Snapshot counters for /diagnostic/dump. Copied
  /// out under the actor's serial context so callers see a coherent
  /// point-in-time view.
  public func counterSnapshot() -> Counters {
    return counters
  }
}

/// Server-side sim-health state, emitted on every
/// response via the `X-Sim-Health` header. Consumers on all 4 SDKs
/// parse the header and surface transitions via `Session.state` /
/// `session.on('state', ...)` / `session.stateStream` / `stateFlow`.
public actor SimHealthPublisher {
  public enum State: String, Sendable {
    case healthy
    case degraded
    case cycling
    case dead
  }

  private var state: State = .healthy

  public init(initial: State = .healthy) {
    self.state = initial
  }

  /// Cheap snapshot for the header emitter.
  public func snapshot() -> State {
    state
  }

  public func set(_ next: State) {
    state = next
  }
}

/// Per-request context propagated from HTTP headers into every
/// handler that operates on the "app under test".
///
/// **Why this exists**: binding
/// `let app = XCUIApplication(bundleIdentifier: bundleId)` once at runner
/// boot time does not work. Every handler closure captures that `app`
/// reference and uses it forever, even if the sim's foreground app changes. If
/// Preferences (or any non-target app) happens to be foreground at
/// runner boot (`TargetBundleResolver.resolve` falls back to
/// `com.apple.Preferences`), every subsequent `/tree` / `/find` call
/// asks XCUITest for a snapshot of an app that is no longer visible,
/// returning `snapshot_unavailable` forever.
///
/// **Wire**: client sends `App-Bundle-Id: <bundle>` and optionally
/// `App-Activate: true` on every request that touches the target app.
/// Runner parses headers into a `RequestContext`, sets it as a
/// `@TaskLocal`, and calls the handler. The handler reads
/// `SmixRunnerServer.currentContext` when it needs to know the target.
///
/// **Backward-compat**: absent header → `RequestContext.default` →
/// runner uses its cached boot-time `app`.
public struct RequestContext: Sendable {
  /// The bundle id the CLIENT thinks it is talking to for THIS request.
  /// nil = client didn't say (use runner-bound default).
  public let bundleId: String?
  /// If true, call `.activate()` on the resolved target before doing
  /// the operation. Auto-recovers from cases where XCUITest's implicit
  /// "app under test" resolution latched onto a stale foreground.
  public let activate: Bool
  /// The client's session id when it opened one via
  /// `POST /session/open`. When present, `resolveApp()` looks up the
  /// session's cached `XCUIApplication` and skips per-request
  /// `.activate()` regardless of `activate`. Absent → per-request
  /// rebind, rate-limited to at most one activation per 5 s per
  /// bundle-id.
  public let sessionId: String?

  public init(bundleId: String? = nil, activate: Bool = false, sessionId: String? = nil) {
    self.bundleId = bundleId
    self.activate = activate
    self.sessionId = sessionId
  }

  public static let `default` = RequestContext()

  /// Parse from FlyingFox request headers. Recognized keys are
  /// case-insensitive; `App-Bundle-Id`, `App-Activate`, and
  /// `Session-Id` are the canonical forms.
  public static func from(headers: HTTPHeaders) -> RequestContext {
    let bundle = headers[HTTPHeader("App-Bundle-Id")]?
      .trimmingCharacters(in: .whitespaces)
    let activateStr = headers[HTTPHeader("App-Activate")]?
      .trimmingCharacters(in: .whitespaces)
    let activate = activateStr?.lowercased() == "true" || activateStr == "1"
    let session = headers[HTTPHeader("Session-Id")]?
      .trimmingCharacters(in: .whitespaces)
    return RequestContext(
      bundleId: bundle?.isEmpty == false ? bundle : nil,
      activate: activate,
      sessionId: session?.isEmpty == false ? session : nil
    )
  }
}

public actor SmixRunnerServer {
  /// Set inside every route handler via
  /// `SmixRunnerServer.$currentContext.withValue(ctx) { handler() }`.
  /// Handlers read this when they need per-request bundle / activate
  /// info. Uses `@TaskLocal` because handler closures are captured at
  /// server construction time in SmixRunnerUITests.swift —
  /// changing 20+ typealias signatures to thread context explicitly
  /// would be an invasive refactor with no wire-level benefit. Task-
  /// local scoped-mutation gives every handler read access to the
  /// current-request context without touching closure signatures.
  @TaskLocal public static var currentContext: RequestContext = .default

  /// Task-local sim-health publisher. `runForever` sets
  /// this via `.withValue` before dispatching each request; the
  /// `guardedResponse` wrapper reads it and attaches the
  /// `X-Sim-Health` response header on the way out. `nil` when the
  /// caller opted out — no header is emitted and SDKs cope.
  @TaskLocal public static var currentSimHealth: SimHealthPublisher? = nil

  /// Task-local app-alive cache. Routes on the
  /// request-with-target path (`/tree`, `/system-popups`, etc.) read
  /// this to short-circuit XCUITest when a bundle is known-dead.
  @TaskLocal public static var currentAppAliveCache: AppAliveCache? = nil

  /// Task-local popup pacer for the `/system-popups` route middleware.
  @TaskLocal public static var currentPopupPacer: PopupPacer? = nil

  /// Canonical main-actor hop for XCUITest APIs that require
  /// it (activate / launch / terminate on iOS 26+ SDK). Any new
  /// XCUITest call added to the runner must be categorized:
  ///
  /// - **Main-actor-isolated** (activate/launch/terminate/setOrientation
  ///   /setAlertHandler on iOS 26+) → route through this helper.
  /// - **Sendable** (snapshot, descendants, isHittable, frame reads) →
  ///   call directly.
  ///
  /// XCUITest requires `.activate()` to be dispatched on the main
  /// thread on iOS 26+ SDKs; calling it off the main async continuation
  /// raises `NSInternalInconsistencyException`. This helper hops on.
  public static func onMain<T: Sendable>(_ body: @MainActor () -> T) async -> T {
    await MainActor.run { body() }
  }

  /// Closure that the UITest target provides
  /// to categorize why `snapshotHandler` returned nil (or the tree
  /// query threw). Returns a categorized `AppUnavailableReason`
  /// value. `nil` bundle id → the caller should return `.unknown`.
  ///
  /// The UITest target reads `XCUIApplication.state` and
  /// `~/Library/Logs/DiagnosticReports/` for the given bundle id (both
  /// XCTest-scope APIs, not accessible from `SmixRunnerCore` directly)
  /// to discriminate `crashed-during-init` vs `alive-but-tree-empty`.
  /// The closure runs on demand from the `/tree` handler, so
  /// runtime cost is per-failure only.
  public typealias UnavailableReasonInferer = @Sendable (String?) async -> AppUnavailableReason

  /// Task-local inference closure. Set by
  /// `runForever` when the UITest target provides one; `nil` opts
  /// out — UITest targets that don't know about categorization
  /// get the `.unknown` fallback.
  @TaskLocal public static var currentUnavailableReasonInferer: UnavailableReasonInferer? = nil

  public enum TapOutcome: Sendable {
    // `frame` / `appFrame` are populated by the UITest handler so
    // the SDK can host-HID-inject at coord when `mode == .resolve`. Both
    // stay optional so plain `resolveAndTap` outcomes keep serializing
    // without them.
    case matched(
      label: String,
      stages: TapRoute.TapStages? = nil,
      frame: CGRect? = nil,
      appFrame: CGRect? = nil
    )
    case notFound
  }
  // `scope` carries the `?include=` query value (nil when absent),
  // identical to SnapshotHandler. nil ⇒ the default element-resolution
  // source (the SDK runner-client posts /tap WITHOUT a query).
  // "all-windows" ⇒ the UITest target resolves the candidate element from the SAME
  // see-through capture (`buildAllWindowsSnapshot`'s per-window +
  // flat-descendants set) that `/tree?include=all-windows` already uses,
  // so an element the SDK can SEE through a native modal is also tappable.
  public typealias TapHandler = @Sendable (
    TapRoute.TapRequest, _ scope: String?
  ) async -> TapOutcome

  // Runner-side XCUIElement.typeText path. The smix
  // SDK side delegates to these routes for fill / clear / pressKey because
  // host-HID keyboard injection requires per-key code mapping + modifier
  // handling that is materially larger work than digitizer; runner
  // typeText is the practical path. Each handler returns KeyboardOutcome
  // (success carries per-stage timing; notFound when element resolution
  // fails or — for pressKey — the key string is unsupported).
  //
  // Handlers carry sub-stage timing so the SDK side can record
  // hot-spot data alongside TapStages (focus_ms = resolve + focus tap;
  // daemon_send_ms = `_XCT_sendString:` round-trip).
  public enum KeyboardOutcome: Sendable {
    case success(focusMs: UInt32, daemonSendMs: UInt32)
    case notFound
  }

  // `scope` mirrors TapHandler: nil ⇒ the default focus-tap element
  // source; "all-windows" ⇒ the see-through capture (so a typable field
  // masked by a native modal is still focus-tappable). Same `?include=`
  // query mechanism as /tree.
  public typealias FillHandler = @Sendable (
    _ selector: String, _ text: String, _ scope: String?, _ dispatch: InputDispatchMode
  ) async -> KeyboardOutcome

  /// How `/fill` puts text into the app.
  ///
  /// The wire format has documented this header since v1, the client
  /// sends it, and `smix run --force-key-events` sets it — while no
  /// runner read it, so the flag changed nothing. It exists for RN
  /// apps whose hidden `<TextInput>` defeats a11y-focus lookup, which
  /// means the callers reaching for it are the ones for whom the
  /// default already failed.
  public enum InputDispatchMode: String, Sendable {
    /// Resolve the field through the accessibility tree and tap it to
    /// take focus, then type. The default.
    case a11y
    /// Skip focus resolution entirely and type into whatever holds
    /// focus now. For fields the a11y tree cannot address.
    case keyEvents = "key-events"
    /// Same as `a11y` today; reserved for a runner-side decision.
    case auto

    /// Parse the header. An unknown value is `auto` rather than an
    /// error: a client from a future version naming a mode this runner
    /// does not have should degrade, not fail the step.
    public static func parse(_ raw: String?) -> InputDispatchMode {
      guard let raw, !raw.isEmpty else { return .a11y }
      return InputDispatchMode(rawValue: raw) ?? .auto
    }
  }
  public typealias ClearHandler = @Sendable (
    _ selector: String, _ scope: String?
  ) async -> KeyboardOutcome
  public typealias PressKeyHandler = @Sendable (
    _ key: String
  ) async -> KeyboardOutcome

  // /find handler. Returns boolean: true = element exists in
  // current XCUIElement query, false = not found. No `.notFound` case
  // because /find is a query, not an action — false IS a valid result.
  //
  // `scope` mirrors TapHandler: nil ⇒ the default `app.descendants(.any)`
  // query (the SDK runner-client posts /find WITHOUT a query);
  // "all-windows" ⇒ the see-through element set so `expect.toBeVisible()`
  // of content behind a native modal returns the truthful answer instead
  // of a masked false.
  //
  // `requireOnScreen`: when true, the handler additionally requires the
  // LIVE element frame to intersect the app frame. Snapshot frames drift
  // on iOS 26.5 + RN Fabric for below-the-fold elements — they report
  // in-viewport coords — so the live query is needed to re-resolve
  // current layout. Exists-only callers pass false.
  public typealias FindHandler = @Sendable (
    _ selectorText: String, _ scope: String?, _ requireOnScreen: Bool
  ) async -> Bool

  /// XCUI snapshot is constructed inside the UITest target closure
  /// (XCUIApplication.snapshot() is a throwing, blocking API) and returned
  /// as a POCO so SmixRunnerCore stays free of XCTest/XCUI imports.
  /// nil indicates the snapshot is unavailable (app not launched / crashed) —
  /// the server responds with 500 + snapshot_unavailable in that case.
  public typealias SnapshotResult = (root: TreeRoute.A11ySnapshotData, appFrame: CGRect)
  // `scope` carries the `?include=` query value (absent ⇒ nil). nil ⇒ a
  // single `app.snapshot()` root. "all-windows" ⇒ per-window snapshots
  // plus a flat-descendants fallback, merged into a synthetic root, so
  // /tree can see through any opaque native modal overlay and reach the
  // AX-reachable content beneath it. The handler owns interpretation of
  // `scope`; the server forwards the raw signal verbatim.
  public typealias SnapshotHandler = @Sendable (_ scope: String?) async -> SnapshotResult?

  // System popup sense handler. Popup sense is a core flat capability
  // (peer of /tree / /find / /tap), not something buried in a driver.
  // `scope` carries the `?include=` query value with
  // the same semantics as SnapshotHandler. The handler enumerates
  // SpringBoard alerts / sheets / dialogs and returns POCOs the server
  // serializes via `SystemPopupsRoute.success(popups:)`.
  public typealias SystemPopupsHandler = @Sendable (_ scope: String?) async -> [SystemPopupsRoute.Popup]

  // POST /scroll handler. Drives the runner-side XCUITest
  // swipe-until-visible loop. The route module owns decode + envelope; the
  // handler owns selector → element resolution + per-swipe existence probe.
  // Returns matched + swipes count; the wire layer (RunnerClient.scroll)
  // transforms `matched=false` into RunnerScrollNotMatched on the SDK
  // side (HTTP stays 200 — same discipline as /find: business result is in
  // body, transport errors are 4xx/5xx). `scope` carries `?include=` query
  // value (`"all-windows"` ⇒ see-through resolution path).
  public typealias ScrollHandler = @Sendable (
    _ selector: String, _ direction: String,
    _ maxSwipes: Int, _ timeoutMs: Int, _ scope: String?
  ) async -> (matched: Bool, swipes: Int)

  // POST /foreground handler. App-level act capability (no
  // selector / no scope / no loop) — instantiates XCUIApplication
  // (bundleIdentifier:) and calls .activate(). XCUIApplication.activate
  // is documented idempotent: repeated call on frontmost app is no-op.
  // No `?include=` query (foreground is app-level not element-level).
  // Returns `ok:true` on "activate request dispatched" (XCUITest didn't
  // throw mid-call), `ok:false` when the smixGuarded trampoline caught
  // an NSException (vanished bundle / sim mid-crash / etc.).
  public typealias ForegroundHandler = @Sendable (_ bundleId: String) async -> Bool

  // POST /back handler. App-level navigation pop capability —
  // queries `XCUIApplication.navigationBars.buttons.firstMatch` and calls
  // `.tap()`. The standard XCUITest path is i18n-safe (firstMatch is
  // positional, not label-based). No selector / no scope / no bundleId
  // (back operates on the runner-bound app — caller chose the bound app
  // at runForever() time). Returns `ok:true` when the tap dispatched,
  // `ok:false` when there's no navbar back button on the current screen
  // (top-level / root screens) or smixGuarded caught an NSException.
  public typealias BackHandler = @Sendable () async -> Bool

  /// Single-swipe gesture handler. Caller (driver host-side loop)
  /// passes direction; handler triggers XCUITest swipe gesture (`app.swipeUp()`
  /// for content direction "down", symmetric for "up") with no probe / no
  /// selector. Returns true on swipe completed, false on swizzler / XCUITest
  /// internal NSException (vanished-element / app terminated mid-gesture).
  public typealias SwipeOnceHandler = @Sendable (_ direction: String, _ scope: String?) async -> Bool

  /// POST /tap-at-norm-coord handler. Coord-based tap via
  /// `app.coordinate(withNormalizedOffset: CGVector(dx:nx, dy:ny)).tap()`.
  /// The Apple native UI event chain fires RN Pressable React events,
  /// whereas host-HID-synthesized touches do not fire inside an RN modal.
  /// It also sidesteps Apple's element-query ordering ambiguity: the host
  /// supplies concrete coords and the runner never re-queries. Returns
  /// true on tap dispatched, false on smixGuarded NSException.
  public typealias TapAtCoordHandler = @Sendable (_ nx: Double, _ ny: Double) async -> Bool

  /// POST /tap-by-id handler. Resolves an element by accessibility
  /// identifier and invokes `XCUIElement.tap()` (the XCTest gesture-recognizer
  /// chain). Lets the SDK opt into the path that drives SwiftUI .sheet /
  /// .alert / .confirmationDialog / .fullScreenCover binding actions —
  /// the default host-HID-at-coord tap lands on the frame but doesn't fire
  /// SwiftUI's onTap closure for modal dismiss buttons.
  /// Returns true when the element was found and tapped, false when not
  /// found (no element matching the identifier) or when a smixGuarded
  /// NSException surfaces.
  public typealias TapByIdHandler = @Sendable (_ identifier: String) async -> Bool

  /// POST /find-text-by-ocr handler. Apple Vision OCR (VNRecognize
  /// TextRequest) over the current XCUIScreen screenshot. Returns the
  /// matching text observation's bounding box in UIKit-normalized
  /// `(nx, ny, w, h)` (top-left origin, y-down, [0,1]). nil when no match.
  /// Sense-layer fallback for a11y-less and i18n cases — covers roughly
  /// 40-50% of "third-party lib without testID but with visible text"
  /// scenarios.
  public typealias FindTextByOcrHandler = @Sendable (
    _ text: String, _ locales: [String], _ recognitionLevel: String
  ) async -> (Double, Double, Double, Double)?

  /// POST /swipe-at-norm-coord handler. Coord-based swipe gesture
  /// via Apple native event chain `XCSynthesizedEventRecord` +
  /// `XCPointerEventPath` (`initForTouchAtPoint:` → `moveToPoint:atOffset:`
  /// → `liftUpAtOffset:`). Companion to `TapAtCoordHandler` on the
  /// normalized-coordinate escape-hatch path. Returns true on swipe
  /// dispatched, false on smixGuarded NSException / synthesize error.
  public typealias SwipeAtCoordHandler = @Sendable (
    _ fromNx: Double, _ fromNy: Double, _ toNx: Double, _ toNy: Double
  ) async -> Bool

  /// POST /double-tap handler. XCUIElement.doubleTap() public API path —
  /// note it does not fire on React Native modals.
  /// Returns true on double-tap dispatched; false on smixGuarded NSException
  /// or element-not-found.
  public typealias DoubleTapHandler = @Sendable (
    _ selectorText: String
  ) async -> Bool

  /// POST /long-press handler. XCUIElement.press(forDuration:)
  /// public API. `durationMs` is in milliseconds; the caller is
  /// responsible for the ms → seconds conversion.
  public typealias LongPressHandler = @Sendable (
    _ selectorText: String, _ durationMs: UInt32
  ) async -> Bool

  /// POST /set-orientation handler. XCUIDevice.shared.orientation is a
  /// framework-documented public XCUI property, so the rule requiring
  /// private symbols to be resolved via dlsym does not apply here.
  /// Returns true on dispatch, false on smixGuarded NSException.
  public typealias SetOrientationHandler = @Sendable (
    _ orientation: String
  ) async -> Bool

  // POST /hide-keyboard handler. App-level keyboard dismiss
  // capability — queries `XCUIApplication.shared.keyboards.firstMatch` and
  // calls `.swipeDown()` (XCUITest standard portable API; no private API).
  // Idempotent: if no keyboard is on screen, returns `ok:true` (no-op).
  // No selector / no scope / no bundleId — keyboard is frontmost-app scope.
  // Returns `ok:true` when the dismiss dispatched (or keyboard absent =
  // already-dismissed), `ok:false` when smixGuarded caught an NSException.
  public typealias HideKeyboardHandler = @Sendable () async -> Bool

  // POST /input-text handler. Types the given text into the
  // CURRENTLY FOCUSED element (no selector, no focus-tap — the caller
  // focused the field first; FFI App.fill taps then posts /input-text).
  // Same wire contract as the Android runner's /input-text. Returns
  // `true` when the daemon send succeeded, `false` on send failure
  // (surfaces as `ok:false` = refusal to the Rust OkEnvelope).
  public typealias InputTextHandler = @Sendable (_ text: String) async -> Bool

  // POST /system-popup-action {popupId, buttonId} — the act side.
  // Companion to SystemPopupsHandler (sense side). The handler walks the
  // same enumerate scan order — SpringBoard alerts → SpringBoard sheets
  // → bound-app alerts/sheets/dialogs/popovers — matches popup + button
  // by the id derivation `collectSystemPopups` uses
  // (container.identifier fallback "popup-N"; b.identifier fallback
  // "b-N"), and taps the matched button via the EventSynthesizer +
  // daemonProxySynthesize dlsym chain. .found ⇒ 200 ok envelope;
  // .notFound (popup or button id missed, or synthesize raised) ⇒ 404
  // not_found envelope echoing the input ids.
  public enum SystemPopupActionOutcome: Sendable {
    case found
    case notFound
  }
  public typealias SystemPopupActionHandler = @Sendable (
    _ popupId: String, _ buttonId: String
  ) async -> SystemPopupActionOutcome

  // SelectResolve handler dispatches the 3 /select/resolve*
  // endpoints. Single closure receives the Request + Action discriminator
  // and returns a Result variant matching the action. In production
  // SmixRunnerUITests injects a closure that calls
  // SmixCoreFFIBindings.resolveSelector / resolveSelectorCount /
  // resolveSelectorLabels. Throws → 500 ffiError envelope.
  public typealias SelectResolveHandler = @Sendable (
    _ request: SelectResolveRoute.Request,
    _ action: SelectResolveRoute.Action
  ) async throws -> SelectResolveRoute.Result

  // Record route handlers triple. /record/start enables the
  // EventRecorder capture buffer (swizzle already installed in UITest
  // setUp gated by SMIX_RECORD_ENABLED). /record/stop drains + disables;
  // /record/poll drains without disabling for streaming reads. Triple
  // bundled together (vs three independent typealiases) because the three
  // share lifecycle state in the UITest target's recorder — `start` /
  // `stop` / `poll` are not independently composable: a poll without a
  // prior start returns []; a start without a stop leaks buffer.
  public struct RecordHandlers: Sendable {
    public let start: @Sendable () async -> Void
    public let stop: @Sendable () async -> [RecordedEvent]
    public let poll: @Sendable () async -> [RecordedEvent]
    public init(
      start: @escaping @Sendable () async -> Void,
      stop: @escaping @Sendable () async -> [RecordedEvent],
      poll: @escaping @Sendable () async -> [RecordedEvent]
    ) {
      self.start = start
      self.stop = stop
      self.poll = poll
    }
  }

  /// Outcome of a session-open request from the UITest handler's
  /// perspective. The runner's session table is owned by the UITest target
  /// (where XCUIApplication instances live); this DTO shape lets Core
  /// serialize the response without pulling XCUIApplication into the
  /// Package library.
  public struct SessionOpenOutcome: Sendable {
    public let sessionId: String
    public let activatedOnce: Bool
    public init(sessionId: String, activatedOnce: Bool) {
      self.sessionId = sessionId
      self.activatedOnce = activatedOnce
    }
  }

  /// Outcome of a session-close request. `ok` is always true for
  /// known sessions; unknown sessions also return true (idempotent close).
  public struct SessionCloseOutcome: Sendable {
    public let ok: Bool
    public init(ok: Bool) {
      self.ok = ok
    }
  }

  /// Outcome of a renew-activation request. `notFound = true`
  /// when the session id does not exist in the table; otherwise `ok` +
  /// `activated` (was `.activate()` actually called, or rate-limited).
  public struct SessionRenewOutcome: Sendable {
    public let notFound: Bool
    public let ok: Bool
    public let activated: Bool
    public init(notFound: Bool, ok: Bool, activated: Bool) {
      self.notFound = notFound
      self.ok = ok
      self.activated = activated
    }
  }

  /// `/session/close-all` outcome.
  public struct SessionCloseAllOutcome: Sendable {
    public let closed: Int
    public init(closed: Int) {
      self.closed = closed
    }
  }

  /// `/session/relaunch-app` outcome.
  public struct SessionRelaunchOutcome: Sendable {
    public let notFound: Bool
    public let ok: Bool
    public let wallMs: UInt64
    public init(notFound: Bool, ok: Bool, wallMs: UInt64) {
      self.notFound = notFound
      self.ok = ok
      self.wallMs = wallMs
    }
  }

  /// `/session/terminate-app` / `/session/launch-app` outcome.
  ///
  /// Carries wait-for-foreground observations (`waitedMs`,
  /// `terminalState`) and a cooperative-terminate flag
  /// (`terminatedCooperatively`) so the diagnostic dump can surface
  /// whether the terminate call went through the XCUIApplication
  /// pathway or fell back to a hard kill (bug_type 309 signal).
  ///
  /// `reachedInteractive` + `interactiveNamedIds` let `launchApp` tell
  /// "process foreground but tree unusable" (splash / dev-launcher /
  /// sparse a11y) apart from "process foreground AND probeable"
  /// numerically.
  public struct SessionAppLifecycleOutcome: Sendable {
    public let notFound: Bool
    public let ok: Bool
    public let wallMs: UInt64
    /// Wall-clock milliseconds spent in the
    /// wait-for-foreground poll loop. Zero when the caller passed
    /// `waitForForegroundMs: nil` or on terminate calls.
    public let waitedMs: UInt64
    /// Final observed `XCUIApplication.state` at the
    /// moment the handler returned. Wire-encoded as the XCTest raw
    /// value: 0 unknown / 1 notRunning / 2 runningBackgroundSuspended /
    /// 3 runningBackground / 4 runningForeground.
    public let terminalState: UInt8
    /// Set on terminate outcomes when the cooperative
    /// `XCUIApplication.terminate()` pathway observed the process
    /// `.notRunning` after the call returned. Always false on launch.
    public let terminatedCooperatively: Bool
    /// Set on `launch-app` outcomes when the
    /// interactive fingerprint (≥ `minIdentifierCount` non-ignored
    /// ax-ids in the tree) was observed within
    /// `waitForInteractiveMs`. Always false on terminate.
    public let reachedInteractive: Bool
    /// Sample of up to 8 `accessibilityIdentifier`
    /// values captured at the moment `reachedInteractive` fired.
    /// Debug aid so consumers can tell "the probe fired on the right
    /// screen" from "the probe fired on a splash-screen artifact
    /// leak". Empty otherwise.
    public let interactiveNamedIds: [String]
    public init(
      notFound: Bool,
      ok: Bool,
      wallMs: UInt64,
      waitedMs: UInt64 = 0,
      terminalState: UInt8 = 0,
      terminatedCooperatively: Bool = false,
      reachedInteractive: Bool = false,
      interactiveNamedIds: [String] = []
    ) {
      self.notFound = notFound
      self.ok = ok
      self.wallMs = wallMs
      self.waitedMs = waitedMs
      self.terminalState = terminalState
      self.terminatedCooperatively = terminatedCooperatively
      self.reachedInteractive = reachedInteractive
      self.interactiveNamedIds = interactiveNamedIds
    }
  }

  /// `/session/list` outcome.
  public struct SessionListOutcome: Sendable {
    public let sessions: [SessionRoute.SessionSummary]
    public init(sessions: [SessionRoute.SessionSummary]) {
      self.sessions = sessions
    }
  }

  /// `/diagnostic/dump` outcome.
  public struct DiagnosticOutcome: Sendable {
    public let snapshot: SessionRoute.DiagnosticSnapshot
    public init(snapshot: SessionRoute.DiagnosticSnapshot) {
      self.snapshot = snapshot
    }
  }

  /// Session lifecycle handlers. Bundled together (vs independent
  /// typealiases) because they share the session-table state in the
  /// UITest target.
  public struct SessionHandlers: Sendable {
    public let open: @Sendable (SessionRoute.OpenRequest) async -> SessionOpenOutcome
    public let close: @Sendable (SessionRoute.CloseRequest) async -> SessionCloseOutcome
    public let renew: @Sendable (SessionRoute.RenewRequest) async -> SessionRenewOutcome
    /// Invoked by `POST /session/close-all`.
    public let closeAll: @Sendable () async -> SessionCloseAllOutcome
    /// Invoked by `POST /session/relaunch-app`.
    public let relaunchApp: @Sendable (SessionRoute.RelaunchRequest) async -> SessionRelaunchOutcome
    /// Invoked by `POST /session/list`.
    public let list: @Sendable () async -> SessionListOutcome
    /// Invoked by `POST /diagnostic/dump`.
    public let diagnostic: @Sendable () async -> DiagnosticOutcome
    /// Invoked by `POST /session/terminate-app`.
    public let terminateApp: @Sendable (SessionRoute.AppLifecycleRequest) async -> SessionAppLifecycleOutcome
    /// Invoked by `POST /session/launch-app`.
    public let launchApp: @Sendable (SessionRoute.AppLifecycleRequest) async -> SessionAppLifecycleOutcome
    public init(
      open: @escaping @Sendable (SessionRoute.OpenRequest) async -> SessionOpenOutcome,
      close: @escaping @Sendable (SessionRoute.CloseRequest) async -> SessionCloseOutcome,
      renew: @escaping @Sendable (SessionRoute.RenewRequest) async -> SessionRenewOutcome,
      closeAll: @escaping @Sendable () async -> SessionCloseAllOutcome,
      relaunchApp: @escaping @Sendable (SessionRoute.RelaunchRequest) async -> SessionRelaunchOutcome,
      list: @escaping @Sendable () async -> SessionListOutcome,
      diagnostic: @escaping @Sendable () async -> DiagnosticOutcome,
      terminateApp: @escaping @Sendable (SessionRoute.AppLifecycleRequest) async -> SessionAppLifecycleOutcome,
      launchApp: @escaping @Sendable (SessionRoute.AppLifecycleRequest) async -> SessionAppLifecycleOutcome
    ) {
      self.open = open
      self.close = close
      self.renew = renew
      self.closeAll = closeAll
      self.relaunchApp = relaunchApp
      self.list = list
      self.diagnostic = diagnostic
      self.terminateApp = terminateApp
      self.launchApp = launchApp
    }
  }

  public init() {}

  // Single point of HTTPServer construction. The runner port must be
  // localhost-only; FlyingFox's `HTTPServer(port:)` shorthand binds
  // IPv6 wildcard `::` (= all interfaces, LAN-reachable, real compliance bug).
  // We bind IPv4 127.0.0.1 explicitly because the SDK `RunnerClient` connects
  // via `127.0.0.1` (IPv4); the FlyingFox `.loopback(port:)` shorthand returns
  // IPv6 ::1 only, which IPv4 clients cannot reach (separate kernel addresses).
  // Tests use port 0 to probe the actual bound address via `server.listeningAddress`.
  internal static func makeServer(port: UInt16) -> HTTPServer {
    // `inet(ip4:port:)` throws only on malformed IP strings; "127.0.0.1" is a
    // compile-time constant so the failure case is unreachable.
    let addr = try! sockaddr_in.inet(ip4: "127.0.0.1", port: port)
    return HTTPServer(address: addr)
  }

  // DispatchTime delta → ms (truncates fractional). Shared by tap
  // (TapStages) and keyboard (KeyboardOutcome.success) timing paths in the
  // UITest closure.
  public static func msBetween(_ start: DispatchTime, _ end: DispatchTime) -> UInt32 {
    let nanos = end.uptimeNanoseconds &- start.uptimeNanoseconds
    return UInt32(min(nanos / 1_000_000, UInt64(UInt32.max)))
  }

  // Server-side route guard — the structural backstop against a route
  // error taking the whole runner down.
  //
  // The runner's route closures call into the UITest target's handler
  // closures, which in turn drive XCUITest. When a resolved element
  // vanishes mid-interaction XCUITest fails the running test
  // (`element.tap()` → "Failed to get matching snapshot: No matches
  // found"). An ObjC trampoline turns that into a thrown Swift Error;
  // this guard then ensures such an error — or ANY error the response
  // builder raises — never escapes the route closure into FlyingFox and
  // from there into `runForever`'s withThrowingTaskGroup, which would
  // re-throw it and terminate `test_runForever`, restarting the runner.
  //
  // Contract: on success the builder's HTTPResponse is returned
  // byte-identical; on throw the route's own error envelope `fallback`
  // is returned and the error is swallowed. No XCUI / XCTest here, so
  // this stays unit-decidable in SmixRunnerCore.
  public static func guardedResponse(
    fallback: HTTPResponse,
    _ build: () async throws -> HTTPResponse
  ) async -> HTTPResponse {
    let raw: HTTPResponse
    do {
      raw = try await build()
    } catch {
      // Surface the caught exception type+reason to stderr so modal-overlay
      // side-effects stay diagnosable (e.g. a snapshot 500 on the dashboard
      // after a `+load` swizzle) instead of being silently swallowed.
      FileHandle.standardError.write(
        Data("smix-runner: guardedResponse caught: \(error)\n".utf8))
      raw = fallback
    }
    // Attach the X-Sim-Health header to every response the guard emits.
    // The task-local publisher is set per-request by runForever's
    // wrapping context; a nil publisher means the runner opted out and
    // no header is emitted.
    if let publisher = Self.currentSimHealth {
      let state = await publisher.snapshot()
      return await withHeader(raw, name: "X-Sim-Health", value: state.rawValue)
    }
    return raw
  }

  /// Return a copy of the response with an extra
  /// header injected. FlyingFox's HTTPResponse is a value type so
  /// mutating a copy is cheap.
  public static func withHeader(_ response: HTTPResponse, name: String, value: String) async -> HTTPResponse {
    var headers = response.headers
    headers[HTTPHeader(name)] = value
    let body: Data
    do {
      body = try await response.bodyData
    } catch {
      body = Data()
    }
    return HTTPResponse(
      statusCode: response.statusCode,
      headers: headers,
      body: body
    )
  }

  /// Wraps [`guardedResponse`] with per-request context
  /// propagation. Reads `App-Bundle-Id` / `App-Activate` headers into a
  /// [`RequestContext`] and sets it as `SmixRunnerServer.currentContext`
  /// (task-local) for the duration of the handler.
  ///
  /// Every route that operates on the target app (`/tap`, `/tree`,
  /// `/find`, `/scroll`, `/swipe-*`, `/back`, `/hide-keyboard`, etc.)
  /// should use this wrapper. Routes that touch springboard, XCUIDevice,
  /// or infrastructure-only surfaces (`/health`, `/shutdown`, `/system-
  /// popups`, `/press-key`, `/set-orientation`, `/record/*`) continue
  /// using bare [`guardedResponse`] — they don't consult context.
  public static func contextGuardedResponse(
    request: HTTPRequest,
    fallback: HTTPResponse,
    _ build: () async throws -> HTTPResponse
  ) async -> HTTPResponse {
    let ctx = RequestContext.from(headers: request.headers)
    return await Self.$currentContext.withValue(ctx) {
      // Reuse the plain guardedResponse for the actual error trampoline —
      // task-local is already set by .withValue above.
      await guardedResponse(fallback: fallback, build)
    }
  }

  /// Register `/session/open`, `/session/close`,
  /// `/session/renew-activation`. Session-lifecycle handlers exist to
  /// avoid an activation storm: consumers open once, run their entire
  /// flow against the cached binding, and close on exit. The
  /// per-request `App-Activate: true` path remains available
  /// (rate-limited) for consumers that do not use sessions.
  public static func registerSessionRoutes(
    server: HTTPServer,
    handlers: SessionHandlers
  ) async {
    await server.appendRoute("POST /session/open") { request in
      let body: Data
      do { body = try await request.bodyData }
      catch { return SessionRoute.badRequest(reason: "failed to read body: \(error)") }
      let req: SessionRoute.OpenRequest
      do { req = try SessionRoute.decodeOpen(body) }
      catch {
        return SessionRoute.badRequest(reason: "\(error)")
      }
      return await Self.guardedResponse(
        fallback: SessionRoute.badRequest(reason: "handler crashed")
      ) {
        let outcome = await handlers.open(req)
        // A successful open re-establishes the target for
        // this bundle; clear any suppression entry.
        if let cache = Self.currentAppAliveCache {
          await cache.markAlive(bundleId: req.bundleId)
        }
        let now = UInt64(Date().timeIntervalSince1970 * 1000)
        return SessionRoute.openResponse(
          SessionRoute.OpenResponse(
            sessionId: outcome.sessionId,
            activatedOnce: outcome.activatedOnce,
            serverTimeMs: now
          )
        )
      }
    }
    await server.appendRoute("POST /session/close") { request in
      let body: Data
      do { body = try await request.bodyData }
      catch { return SessionRoute.badRequest(reason: "failed to read body: \(error)") }
      let req: SessionRoute.CloseRequest
      do { req = try SessionRoute.decodeClose(body) }
      catch {
        return SessionRoute.badRequest(reason: "\(error)")
      }
      return await Self.guardedResponse(
        fallback: SessionRoute.closeResponse(ok: false)
      ) {
        let outcome = await handlers.close(req)
        return SessionRoute.closeResponse(ok: outcome.ok)
      }
    }
    await server.appendRoute("POST /session/renew-activation") { request in
      let body: Data
      do { body = try await request.bodyData }
      catch { return SessionRoute.badRequest(reason: "failed to read body: \(error)") }
      let req: SessionRoute.RenewRequest
      do { req = try SessionRoute.decodeRenew(body) }
      catch {
        return SessionRoute.badRequest(reason: "\(error)")
      }
      return await Self.guardedResponse(
        fallback: SessionRoute.renewResponse(ok: false, activated: false)
      ) {
        let outcome = await handlers.renew(req)
        if outcome.notFound {
          return SessionRoute.notFound(reason: "unknown session id")
        }
        return SessionRoute.renewResponse(ok: outcome.ok, activated: outcome.activated)
      }
    }
    // POST /session/close-all
    await server.appendRoute("POST /session/close-all") { _ in
      return await Self.guardedResponse(
        fallback: SessionRoute.closeAllResponse(closed: 0)
      ) {
        let outcome = await handlers.closeAll()
        return SessionRoute.closeAllResponse(closed: outcome.closed)
      }
    }
    // POST /session/list
    await server.appendRoute("POST /session/list") { _ in
      return await Self.guardedResponse(
        fallback: SessionRoute.listResponse([])
      ) {
        let outcome = await handlers.list()
        return SessionRoute.listResponse(outcome.sessions)
      }
    }
    // POST /diagnostic/dump
    await server.appendRoute("POST /diagnostic/dump") { _ in
      return await Self.guardedResponse(
        fallback: SessionRoute.diagnosticResponse(
          SessionRoute.DiagnosticSnapshot(
            sessions: [], simHealth: "unknown", supervisorPid: nil, uptimeMs: 0
          )
        )
      ) {
        let outcome = await handlers.diagnostic()
        return SessionRoute.diagnosticResponse(outcome.snapshot)
      }
    }
    // POST /session/terminate-app + /session/launch-app
    for (path, isTerminate) in [("POST /session/terminate-app", true), ("POST /session/launch-app", false)] {
      let isTerm = isTerminate
      await server.appendRoute(HTTPRoute(path)) { request in
        let body: Data
        do { body = try await request.bodyData }
        catch { return SessionRoute.badRequest(reason: "failed to read body: \(error)") }
        let req: SessionRoute.AppLifecycleRequest
        do { req = try SessionRoute.decodeAppLifecycle(body) }
        catch {
          return SessionRoute.badRequest(reason: "\(error)")
        }
        return await Self.guardedResponse(
          fallback: SessionRoute.appLifecycleResponse(ok: false, wallMs: 0)
        ) {
          let outcome = isTerm
            ? await handlers.terminateApp(req)
            : await handlers.launchApp(req)
          if outcome.notFound {
            return SessionRoute.notFound(reason: "unknown session id")
          }
          return SessionRoute.appLifecycleResponse(
            ok: outcome.ok,
            wallMs: outcome.wallMs,
            waitedMs: outcome.waitedMs,
            terminalState: outcome.terminalState,
            terminatedCooperatively: outcome.terminatedCooperatively,
            reachedInteractive: outcome.reachedInteractive,
            interactiveNamedIds: outcome.interactiveNamedIds
          )
        }
      }
    }
    // POST /session/relaunch-app
    await server.appendRoute("POST /session/relaunch-app") { request in
      let body: Data
      do { body = try await request.bodyData }
      catch { return SessionRoute.badRequest(reason: "failed to read body: \(error)") }
      let req: SessionRoute.RelaunchRequest
      do { req = try SessionRoute.decodeRelaunch(body) }
      catch {
        return SessionRoute.badRequest(reason: "\(error)")
      }
      return await Self.guardedResponse(
        fallback: SessionRoute.relaunchResponse(ok: false, wallMs: 0)
      ) {
        let outcome = await handlers.relaunchApp(req)
        if outcome.notFound {
          return SessionRoute.notFound(reason: "unknown session id")
        }
        // A successful relaunch clears any suppression
        // for the target bundle.
        if outcome.ok,
           let cache = Self.currentAppAliveCache,
           let bundleId = Self.currentContext.bundleId {
          await cache.markAlive(bundleId: bundleId)
        }
        return SessionRoute.relaunchResponse(ok: outcome.ok, wallMs: outcome.wallMs)
      }
    }
  }

  // Register the 3 /select/resolve* endpoints. Public static
  // so tests can boot a stub server + register without going through
  // full runForever signature. Each route decodes the shared
  // SelectResolveRoute.Request, dispatches to the handler with the
  // appropriate Action discriminator, and serializes the Result variant
  // via SelectResolveRoute response builders. Decode failure → 400
  // badRequest; handler throw → 500 ffiError.
  public static func registerSelectResolveRoutes(
    server: HTTPServer,
    handler: @escaping SelectResolveHandler
  ) async {
    let routes: [(String, SelectResolveRoute.Action)] = [
      ("POST /select/resolve", .resolve),
      ("POST /select/resolve-count", .count),
      ("POST /select/resolve-labels", .labels),
    ]
    for (path, action) in routes {
      await server.appendRoute(HTTPRoute(stringLiteral: path)) { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return SelectResolveRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: SelectResolveRoute.Request
        do {
          req = try SelectResolveRoute.decode(body)
        } catch let e as SelectResolveRoute.DecodeError {
          return SelectResolveRoute.badRequest(reason: e.reason)
        } catch {
          return SelectResolveRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: SelectResolveRoute.ffiError(reason: "handler crashed")
        ) {
          let result = try await handler(req, action)
          switch result {
          case .ids(let ids):
            return SelectResolveRoute.successIds(ids)
          case .count(let count):
            return SelectResolveRoute.successCount(count)
          case .labels(let labels):
            return SelectResolveRoute.successLabels(labels)
          }
        }
      }
    }
  }

  public func runForever(
    port: UInt16,
    tapHandler: @escaping TapHandler,
    snapshotHandler: @escaping SnapshotHandler,
    fillHandler: FillHandler? = nil,
    clearHandler: ClearHandler? = nil,
    pressKeyHandler: PressKeyHandler? = nil,
    findHandler: FindHandler? = nil,
    systemPopupsHandler: SystemPopupsHandler? = nil,
    systemPopupActionHandler: SystemPopupActionHandler? = nil,
    scrollHandler: ScrollHandler? = nil,
    foregroundHandler: ForegroundHandler? = nil,
    backHandler: BackHandler? = nil,
    swipeOnceHandler: SwipeOnceHandler? = nil,
    hideKeyboardHandler: HideKeyboardHandler? = nil,
    inputTextHandler: InputTextHandler? = nil,
    tapAtCoordHandler: TapAtCoordHandler? = nil,
    tapByIdHandler: TapByIdHandler? = nil,
    findTextByOcrHandler: FindTextByOcrHandler? = nil,
    swipeAtCoordHandler: SwipeAtCoordHandler? = nil,
    doubleTapHandler: DoubleTapHandler? = nil,
    longPressHandler: LongPressHandler? = nil,
    setOrientationHandler: SetOrientationHandler? = nil,
    recordHandlers: RecordHandlers? = nil,
    selectResolveHandler: SelectResolveHandler? = nil,
    sessionHandlers: SessionHandlers? = nil,
    /// Per-session `/system-popups` pacer. `nil` opts out
    /// (legacy unlimited behavior).
    popupPacer: PopupPacer? = nil,
    /// App-alive cache. `nil` opts out (no short-circuit
    /// on XCTIssue "Application not running").
    appAliveCache: AppAliveCache? = nil,
    /// Publisher for the `X-Sim-Health` response header.
    /// `nil` opts out (no header emitted).
    simHealthPublisher: SimHealthPublisher? = nil,
    /// Closure the /tree route consults to
    /// categorize why a snapshot returned nil. UITest target
    /// implements via `XCUIApplication.state` + .ips scan. `nil` opts
    /// out and falls back to an `.aliveButTreeEmpty` best-guess.
    unavailableReasonInferer: UnavailableReasonInferer? = nil
  ) async throws {
    let server = Self.makeServer(port: port)
    let shutdownSignal = ShutdownSignal()

    // The extended /health body is what lets the CLI detect on-disk
    // runner version drift — without it the runner never tells the CLI
    // which version it is. The version comes from the
    // TEST_RUNNER_SMIX_RUNNER_VERSION env var (forwarded by the Rust
    // side of `smix runner up`); uptime is measured from server start.
    // sessionsOpen / activationsTotal are placeholders reporting 0 so
    // the wire shape stays stable.
    let runnerVersion = ProcessInfo.processInfo.environment[
      "SMIX_RUNNER_VERSION"
    ] ?? "unknown"
    let bootDate = Date()
    await server.appendRoute("GET /health") { _ in
      let uptime = UInt64(Date().timeIntervalSince(bootDate) * 1000)
      return HealthRoute.responseDetail(
        runnerVersion: runnerVersion,
        uptimeMs: uptime,
        lastRequestAtMs: 0,
        sessionsOpen: 0,
        activationsTotal: 0
      )
    }

    // Register /select/resolve* routes when handler provided.
    if let selectResolveHandler {
      await Self.registerSelectResolveRoutes(server: server, handler: selectResolveHandler)
    }

    // Register /session/* routes when handlers provided.
    if let sessionHandlers {
      await Self.registerSessionRoutes(server: server, handlers: sessionHandlers)
    }
    // Graceful teardown. The handler MUST NOT `await server.stop()`
    // inline: stop() re-enters the same HTTPServer actor that server.run() is
    // executing on (deadlock), and the response must be sent before the socket
    // closes. So it only fires the one-shot signal + returns immediately; the
    // stop-observer task below performs `server.stop()` out of band.
    await server.appendRoute("POST /shutdown") { _ in
      await shutdownSignal.fire()
      return ShutdownRoute.response()
    }
    await server.appendRoute("POST /tap") { request in
      let body: Data
      do {
        body = try await request.bodyData
      } catch {
        return TapRoute.badRequest(reason: "failed to read body: \(error)")
      }
      let req: TapRoute.TapRequest
      do {
        req = try TapRoute.decode(body)
      } catch let e as TapRoute.DecodeError {
        return TapRoute.badRequest(reason: "\(e)")
      } catch {
        return TapRoute.badRequest(reason: "\(error)")
      }
      // The handler drives XCUITest; if a resolved element vanishes
      // mid-tap the ObjC trampoline surfaces a thrown error. Convert it
      // to the route's own notFound envelope instead of letting it
      // escape the closure and kill the runner.
      //
      // Same `?include=` mechanism as GET /tree
      // (`request.query["include"]`, FlyingFox `[QueryItem]` string
      // subscript, method-independent). nil ⇒ the default element
      // source (the SDK posts no query).
      let scope = request.query["include"]
      return await Self.contextGuardedResponse(request: request,
        fallback: TapRoute.notFound(selector: req.selector)
      ) {
        let outcome = await tapHandler(req, scope)
        switch outcome {
        case .matched(let label, let stages, let frame, let appFrame):
          return TapRoute.success(
            matchedLabel: label,
            stages: stages,
            frame: frame,
            appFrame: appFrame
          )
        case .notFound:
          return TapRoute.notFound(selector: req.selector)
        }
      }
    }
    // Cumulative /tree successful-serve counter for the
    // `X-Tree-Snapshot-Refresh-Count` response header. Consumers
    // debugging batch snapshot drift need a monotonic signal that the
    // runner is (or isn't) doing fresh work.
    // Simple @unchecked-Sendable wrapper around an atomic counter.
    final class TreeServeCounter: @unchecked Sendable {
      private let lock = NSLock()
      private var value: UInt64 = 0
      func advance() -> UInt64 {
        lock.lock(); defer { lock.unlock() }
        value &+= 1
        return value
      }
    }
    let treeServeCounter = TreeServeCounter()
    await server.appendRoute("GET /tree") { request in
      // snapshotHandler calls XCUIApplication.snapshot(), which throws
      // under modal masking; the trampoline surfaces it. Categorize the
      // failure so consumers get an actionable `reason` + `hint` in the
      // error envelope. The
      // fallback (used when the guarded closure throws entirely) stays
      // as the `.unknown` category — we don't have process state to
      // discriminate from that path.
      return await Self.contextGuardedResponse(
        request: request,
        fallback: TreeRoute.unavailable(
          reason: .driverDisconnected,
          hint: AppUnavailableReason.driverDisconnected.defaultHint
        )
      ) {
        // App-alive suppression short-circuit. When the
        // target bundle is known-dead (observed XCTIssue in the last
        // 20 s), return unavailable without running XCUIQuery. This
        // cuts the /tree firehose that continues after an app crash.
        if let cache = Self.currentAppAliveCache,
           let bundleId = Self.currentContext.bundleId,
           await cache.isSuppressed(bundleId: bundleId) {
          // Cache-suppressed means we observed an XCTIssue about the
          // app not running. Most likely
          // `crashed-during-init` (process was up, then died); could
          // also be an XCUITest race that the re-probe cycle will
          // resolve. Ship `crashed-during-init` as the best guess so
          // consumers know to look at .ips first.
          return TreeRoute.unavailable(
            reason: .crashedDuringInit,
            hint: AppUnavailableReason.crashedDuringInit.defaultHint
          )
        }
        // Time the snapshotHandler call end-to-end for
        // the `X-Tree-Snapshot-Wall-Ms` header. Trending upward = the
        // underlying XCUITest snapshot pipeline is bogging down.
        let snapStart = Date()
        guard let snap = await snapshotHandler(request.query["include"]) else {
          // snapshotHandler returned nil without throwing. Ask the
          // UITest-provided inference closure (set as
          // `currentUnavailableReasonInferer` task-local at boot when
          // the UITest target supports categorization). If the closure
          // isn't wired, fall back to `.aliveButTreeEmpty`
          // as the best guess — the most common consumer-observed
          // case is "process foreground but a11y tree came back
          // empty" (splash screen or sparse annotation) rather than
          // "process actually crashed".
          let inferredReason: AppUnavailableReason
          if let inferer = Self.currentUnavailableReasonInferer {
            inferredReason = await inferer(Self.currentContext.bundleId)
          } else {
            inferredReason = .aliveButTreeEmpty
          }
          return TreeRoute.unavailable(
            reason: inferredReason,
            hint: inferredReason.defaultHint
          )
        }
        let snapWallMs = UInt64(Date().timeIntervalSince(snapStart) * 1000)
        let refreshCount = treeServeCounter.advance()
        let payload = TreeRoute.serialize(
          snap.root,
          appFrame: snap.appFrame,
          logSink: { line in
            FileHandle.standardError.write(Data((line + "\n").utf8))
          }
        )
        // Emit tree size + node count as response headers so SDK
        // can record hot-spot data without changing the JSON wire shape,
        // plus the snapshot refresh count + wall-ms.
        return TreeRoute.successWithMeta(
          payload,
          sizeBytes: payload.count,
          nodeCount: TreeRoute.countNodes(snap.root),
          snapshotRefreshCount: refreshCount,
          snapshotWallMs: snapWallMs
        )
      }
    }
    // Keyboard ops are only registered when handlers are provided, so
    // UITest targets without fill support stay binary compatible.
    //
    // The wire shape supports embedding the post-action AX tree snapshot
    // in the same response (saving the SDK one HTTP round-trip when an
    // `expect` follows a fill/clear/pressKey), but on the iOS 26 sim
    // `XCUIApplication.snapshot()` costs 80-150 ms per call — heavier
    // than the /tree round-trip it saves. Measured over a 30-step flow
    // with 7 fills, enabling snapshot-in-response REGRESSED per-step
    // timing from 441 ms to 487 ms (+10%). The capability is kept in the
    // wire (`KeyboardRoute.successWithTree` + the `tree` response field)
    // and in the SDK-side cache (`SimctlDriver.cacheTreeIfPresent` +
    // `cachedTree` TTL) so it can be re-enabled if snapshot cost drops
    // (e.g. a shallower snapshot or a batched event sequence).
    // Current behaviour: emit stages only, never a tree.
    @Sendable func successFor(_ outcome: KeyboardOutcome) -> HTTPResponse? {
      if case let .success(focusMs, daemonSendMs) = outcome {
        return KeyboardRoute.successWithStages(focusMs: focusMs, daemonSendMs: daemonSendMs)
      }
      return nil
    }

    if let fillHandler {
      await server.appendRoute("POST /fill") { request in
        let body: Data
        do { body = try await request.bodyData } catch {
          return KeyboardRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: KeyboardRoute.FillRequest
        do { req = try KeyboardRoute.decodeFill(body) }
        catch let e as KeyboardRoute.DecodeError { return KeyboardRoute.badRequest(reason: "\(e)") }
        catch { return KeyboardRoute.badRequest(reason: "\(error)") }
        let dispatch = InputDispatchMode.parse(
          request.headers[HTTPHeader("Input-Dispatch-Mode")])
        let outcome = await fillHandler(
          req.selector.text, req.text, request.query["include"], dispatch)
        return successFor(outcome) ?? KeyboardRoute.notFound(selector: req.selector)
      }
    }
    if let clearHandler {
      await server.appendRoute("POST /clear") { request in
        let body: Data
        do { body = try await request.bodyData } catch {
          return KeyboardRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: KeyboardRoute.ClearRequest
        do { req = try KeyboardRoute.decodeClear(body) }
        catch let e as KeyboardRoute.DecodeError { return KeyboardRoute.badRequest(reason: "\(e)") }
        catch { return KeyboardRoute.badRequest(reason: "\(error)") }
        let outcome = await clearHandler(
          req.selector.text, request.query["include"])
        return successFor(outcome) ?? KeyboardRoute.notFound(selector: req.selector)
      }
    }
    if let pressKeyHandler {
      await server.appendRoute("POST /press-key") { request in
        let body: Data
        do { body = try await request.bodyData } catch {
          return KeyboardRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: KeyboardRoute.PressKeyRequest
        do { req = try KeyboardRoute.decodePressKey(body) }
        catch let e as KeyboardRoute.DecodeError { return KeyboardRoute.badRequest(reason: "\(e)") }
        catch { return KeyboardRoute.badRequest(reason: "\(error)") }
        let outcome = await pressKeyHandler(req.key)
        return successFor(outcome) ?? KeyboardRoute.unsupportedKey(key: req.key)
      }
    }

    // /find lets the SDK side ask "does this selector match" without
    // the cost of /tree (full snapshot serialization + JS predicate walk).
    if let findHandler {
      await server.appendRoute("POST /find") { request in
        let body: Data
        do { body = try await request.bodyData } catch {
          return FindRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: FindRoute.FindRequest
        do { req = try FindRoute.decode(body) }
        catch let e as FindRoute.DecodeError { return FindRoute.badRequest(reason: "\(e)") }
        catch { return FindRoute.badRequest(reason: "\(error)") }
        // Same `?include=` mechanism as GET /tree. nil ⇒ the default
        // `app.descendants(.any)` query (the SDK posts /find with no
        // query).
        let found = await findHandler(
          req.selector.text, request.query["include"], req.requireOnScreen)
        return FindRoute.success(found: found)
      }
    }

    // `GET /system-popups[?include=]`. Core-flat popup sense; the SDK
    // runner-client posts without a query for the default nil scope;
    // `?include=all-windows` reaches the handler verbatim (same
    // mechanism as /tree). Sense failure ⇒ guard returns
    // the empty-popups envelope (NOT 5xx) so the upper layer reads
    // "nothing observed right now" rather than a protocol error.
    if let systemPopupsHandler {
      await server.appendRoute("GET /system-popups") { request in
        return await Self.contextGuardedResponse(request: request,
          fallback: SystemPopupsRoute.success(popups: [])
        ) {
          // Per-session 500 ms floor. sessionKey defaults
          // to the RequestContext session id when present; otherwise
          // "default" (a single bucket for header-less callers).
          if let pacer = Self.currentPopupPacer {
            let sessionKey = Self.currentContext.sessionId ?? "default"
            if let remainMs = await pacer.tryTake(sessionKey: sessionKey) {
              return SystemPopupsRoute.tooManyRequests(retryAfterMs: remainMs)
            }
          }
          // App-alive suppression short-circuit. When
          // the bundle is known-dead, return empty popups without
          // running any XCUIQuery.
          if let cache = Self.currentAppAliveCache,
             let bundleId = Self.currentContext.bundleId,
             await cache.isSuppressed(bundleId: bundleId) {
            return SystemPopupsRoute.success(popups: [])
          }
          let popups = await systemPopupsHandler(request.query["include"])
          return SystemPopupsRoute.success(popups: popups)
        }
      }
    }

    // POST /system-popup-action. Decodes Request {popupId,
    // buttonId}, dispatches to systemPopupActionHandler. .found ⇒ 200
    // ok envelope; .notFound (popup or button id missed at the UITest
    // side) ⇒ 404 not_found envelope echoing the input ids so the caller
    // can correlate against its prior enumerate. Guarded so a
    // vanished-element NSException inside the handler (the same
    // XCUITest hazard /tap and /scroll already protect against)
    // surfaces as `not_found` instead of escaping the closure. No
    // `?include=` — the action route is element-level but id-keyed; the
    // include scope was decided at enumerate time on the sense path.
    if let systemPopupActionHandler {
      await server.appendRoute("POST /system-popup-action") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return SystemPopupActionRoute.badRequest(
            reason: "failed to read body: \(error)")
        }
        let req: SystemPopupActionRoute.Request
        do {
          req = try SystemPopupActionRoute.decode(body)
        } catch let e as SystemPopupActionRoute.DecodeError {
          return SystemPopupActionRoute.badRequest(reason: "\(e)")
        } catch {
          return SystemPopupActionRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: SystemPopupActionRoute.notFound(
            popupId: req.popupId, buttonId: req.buttonId)
        ) {
          let outcome = await systemPopupActionHandler(req.popupId, req.buttonId)
          switch outcome {
          case .found:
            return SystemPopupActionRoute.success()
          case .notFound:
            return SystemPopupActionRoute.notFound(
              popupId: req.popupId, buttonId: req.buttonId)
          }
        }
      }
    }

    // POST /scroll. Decodes the ScrollRequest, drives the
    // runner-side XCUITest swipe-until-visible loop via scrollHandler,
    // and emits the matched/swipes envelope. Guarded so a vanished-element
    // failure inside the handler (the same mid-interaction XCUITest hazard
    // /tap and /fill already protect against) surfaces as
    // `matched=false, swipes=0` instead of escaping the closure into
    // FlyingFox's withThrowingTaskGroup. maxSwipes / timeoutMs are
    // hardcoded defaults (30 / 30_000ms) — not caller-tunable via SDK
    // opts today.
    if let scrollHandler {
      await server.appendRoute("POST /scroll") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return ScrollRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: ScrollRoute.ScrollRequest
        do {
          req = try ScrollRoute.decode(body)
        } catch let e as ScrollRoute.DecodeError {
          return ScrollRoute.badRequest(reason: "\(e)")
        } catch {
          return ScrollRoute.badRequest(reason: "\(error)")
        }
        let scope = request.query["include"]
        // Serialize the selector as JSON for the handler — runner side
        // parses out text / id (mirrors how findHandler receives just the
        // text string today; scroll carries both possible base fields).
        let selectorJSON: String
        if let t = req.selector.text {
          selectorJSON = #"{"text":"\#(jsonEscapeSelector(t))"}"#
        } else if let id = req.selector.id {
          selectorJSON = #"{"id":"\#(jsonEscapeSelector(id))"}"#
        } else {
          // Should not reach: decode() rejects when both fields absent.
          return ScrollRoute.badRequest(reason: "selector missing both text and id")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: ScrollRoute.success(matched: false, swipes: 0)
        ) {
          let outcome = await scrollHandler(
            selectorJSON, req.direction, 30, 30_000, scope
          )
          return ScrollRoute.success(
            matched: outcome.matched, swipes: outcome.swipes
          )
        }
      }
    }

    // POST /foreground. Decodes ForegroundRequest, calls
    // foregroundHandler with bundleId, wraps result in {ok:<bool>} envelope.
    // Guarded so an XCUITest NSException inside the handler (the
    // XCUIApplication.activate same mid-interaction hazard /tap and /scroll
    // already protect against) surfaces as `ok:false` instead of escaping
    // the closure into FlyingFox's withThrowingTaskGroup. No `?include=`
    // query — foreground is an app-level act, not element-level (no scope).
    if let foregroundHandler {
      await server.appendRoute("POST /foreground") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return ForegroundRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: ForegroundRoute.ForegroundRequest
        do {
          req = try ForegroundRoute.decode(body)
        } catch let e as ForegroundRoute.DecodeError {
          return ForegroundRoute.badRequest(reason: "\(e)")
        } catch {
          return ForegroundRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: ForegroundRoute.success(ok: false)
        ) {
          let ok = await foregroundHandler(req.bundleId)
          return ForegroundRoute.success(ok: ok)
        }
      }
    }

    // POST /back. Decodes BackRequest (empty body OK), calls
    // backHandler, wraps result in {ok:<bool>} envelope. Guarded so an
    // XCUITest NSException inside the handler (the navigationBars query +
    // .tap() mid-interaction hazard that /tap and /scroll already protect
    // against) surfaces as `ok:false` instead of escaping the closure.
    // No `?include=` query — back is app-level navigation, not element-level.
    if let backHandler {
      await server.appendRoute("POST /back") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return BackRoute.badRequest(reason: "failed to read body: \(error)")
        }
        do {
          _ = try BackRoute.decode(body)
        } catch let e as BackRoute.DecodeError {
          return BackRoute.badRequest(reason: "\(e)")
        } catch {
          return BackRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: BackRoute.success(ok: false)
        ) {
          let ok = await backHandler()
          return BackRoute.success(ok: ok)
        }
      }
    }

    // POST /swipe-once. Single-swipe gesture, no probe, no
    // selector. Used by driver-side host-loop scrollUntilVisible to bypass
    // runner-side XCUIElement query.firstMatch stall on dict-only RN elements
    // (which triggers FlyingFox 15s handler timeout → /scroll 500). `?include=`
    // query optional (forwarded as scope param, currently only ignored — swipe
    // is app-level gesture).
    if let swipeOnceHandler {
      await server.appendRoute("POST /swipe-once") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return SwipeOnceRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: SwipeOnceRoute.SwipeOnceRequest
        do {
          req = try SwipeOnceRoute.decode(body)
        } catch let e as SwipeOnceRoute.DecodeError {
          return SwipeOnceRoute.badRequest(reason: "\(e)")
        } catch {
          return SwipeOnceRoute.badRequest(reason: "\(error)")
        }
        let scope = request.query["include"]
        return await Self.contextGuardedResponse(request: request,
          fallback: SwipeOnceRoute.success(ok: false)
        ) {
          let ok = await swipeOnceHandler(req.direction.rawValue, scope)
          return SwipeOnceRoute.success(ok: ok)
        }
      }
    }

    // POST /tap-at-norm-coord. Coord-based tap via
    // `app.coordinate(withNormalizedOffset:).tap()`. Body `{"nx", "ny"}` ∈ [0,1].
    // Pairs a host-side DFS-first resolve with the runner's native UI
    // event chain.
    if let tapAtCoordHandler {
      await server.appendRoute("POST /tap-at-norm-coord") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return TapAtCoordRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: TapAtCoordRoute.TapAtCoordRequest
        do {
          req = try TapAtCoordRoute.decode(body)
        } catch let e as TapAtCoordRoute.DecodeError {
          return TapAtCoordRoute.badRequest(reason: "\(e)")
        } catch {
          return TapAtCoordRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: TapAtCoordRoute.success(ok: false)
        ) {
          let ok = await tapAtCoordHandler(req.nx, req.ny)
          return TapAtCoordRoute.success(ok: ok)
        }
      }
    }

    // POST /tap-by-id {"id":"<a11y-id>"}. XCUIElement.tap() via
    // the XCTest gesture-recognizer chain for SwiftUI .sheet/.alert/
    // .confirmationDialog/.fullScreenCover dismiss buttons that the default
    // host-HID-at-coord path can't trigger. Parallel to /tap-at-norm-coord;
    // /tap itself is left untouched.
    if let tapByIdHandler {
      await server.appendRoute("POST /tap-by-id") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return TapByIdRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: TapByIdRoute.TapByIdRequest
        do {
          req = try TapByIdRoute.decode(body)
        } catch let e as TapByIdRoute.DecodeError {
          return TapByIdRoute.badRequest(reason: "\(e)")
        } catch {
          return TapByIdRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: TapByIdRoute.success(ok: false)
        ) {
          let ok = await tapByIdHandler(req.id)
          return TapByIdRoute.success(ok: ok)
        }
      }
    }

    // POST /find-text-by-ocr. Apple Vision OCR over current
    // XCUIScreen screenshot. L5 sense layer per a11y-i18n master plan.
    if let findTextByOcrHandler {
      await server.appendRoute("POST /find-text-by-ocr") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return FindTextByOcrRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: FindTextByOcrRoute.FindTextByOcrRequest
        do {
          req = try FindTextByOcrRoute.decode(body)
        } catch let e as FindTextByOcrRoute.DecodeError {
          return FindTextByOcrRoute.badRequest(reason: "\(e)")
        } catch {
          return FindTextByOcrRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: FindTextByOcrRoute.success(found: false, frame: nil)
        ) {
          let frame = await findTextByOcrHandler(req.text, req.locales, req.recognitionLevel)
          return FindTextByOcrRoute.success(found: frame != nil, frame: frame)
        }
      }
    }

    // POST /swipe-at-norm-coord. From-to coordinate swipe gesture
    // via Apple native event chain (XCSynthesizedEventRecord +
    // XCPointerEventPath moveToPoint / liftUp). Body
    // `{"fromNx","fromNy","toNx","toNy"}` ∈ [0,1]. Normalized-coordinate
    // escape-hatch companion to /tap-at-norm-coord.
    if let swipeAtCoordHandler {
      await server.appendRoute("POST /swipe-at-norm-coord") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return SwipeAtCoordRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: SwipeAtCoordRoute.SwipeAtCoordRequest
        do {
          req = try SwipeAtCoordRoute.decode(body)
        } catch let e as SwipeAtCoordRoute.DecodeError {
          return SwipeAtCoordRoute.badRequest(reason: "\(e)")
        } catch {
          return SwipeAtCoordRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: SwipeAtCoordRoute.success(ok: false)
        ) {
          let ok = await swipeAtCoordHandler(req.fromNx, req.fromNy, req.toNx, req.toNy)
          return SwipeAtCoordRoute.success(ok: ok)
        }
      }
    }

    // POST /double-tap. XCUIElement.doubleTap() public API path.
    // Body {selector: {text}}. Sibling of the /tap envelope, but with no
    // stages — this is a single path, so timing is not broken down.
    if let doubleTapHandler {
      await server.appendRoute("POST /double-tap") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return DoubleTapRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: DoubleTapRoute.DoubleTapRequest
        do {
          req = try DoubleTapRoute.decode(body)
        } catch let e as DoubleTapRoute.DecodeError {
          return DoubleTapRoute.badRequest(reason: "\(e)")
        } catch {
          return DoubleTapRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: DoubleTapRoute.notFound(selectorText: req.selectorText)
        ) {
          let ok = await doubleTapHandler(req.selectorText)
          return ok
            ? DoubleTapRoute.success()
            : DoubleTapRoute.notFound(selectorText: req.selectorText)
        }
      }
    }

    // POST /long-press. XCUIElement.press(forDuration:) public API.
    // Body {selector: {text}, durationMs: N}. Duration is in milliseconds.
    if let longPressHandler {
      await server.appendRoute("POST /long-press") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return LongPressRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: LongPressRoute.LongPressRequest
        do {
          req = try LongPressRoute.decode(body)
        } catch let e as LongPressRoute.DecodeError {
          return LongPressRoute.badRequest(reason: "\(e)")
        } catch {
          return LongPressRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: LongPressRoute.notFound(selectorText: req.selectorText)
        ) {
          let ok = await longPressHandler(req.selectorText, req.durationMs)
          return ok
            ? LongPressRoute.success()
            : LongPressRoute.notFound(selectorText: req.selectorText)
        }
      }
    }

    // POST /set-orientation. XCUIDevice.shared.orientation public
    // XCUI API. Body {orientation: "portrait|portraitUpsideDown|landscapeLeft|landscapeRight"}.
    if let setOrientationHandler {
      await server.appendRoute("POST /set-orientation") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return SetOrientationRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: SetOrientationRoute.SetOrientationRequest
        do {
          req = try SetOrientationRoute.decode(body)
        } catch let e as SetOrientationRoute.DecodeError {
          return SetOrientationRoute.badRequest(reason: "\(e)")
        } catch {
          return SetOrientationRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: SetOrientationRoute.success(ok: false)
        ) {
          let ok = await setOrientationHandler(req.orientation)
          return SetOrientationRoute.success(ok: ok)
        }
      }
    }

    // POST /hide-keyboard. Decodes HideKeyboardRequest (empty
    // body OK; mirrors /back), calls hideKeyboardHandler, wraps result in
    // {ok:<bool>} envelope. Guarded so an XCUITest NSException inside the
    // handler (keyboards.firstMatch.swipeDown() mid-interaction hazard that
    // /tap / /back already protect against) surfaces as `ok:false` instead
    // of escaping the closure. No `?include=` query — keyboard dismiss is
    // app-level, not element-level.
    if let hideKeyboardHandler {
      await server.appendRoute("POST /hide-keyboard") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return HideKeyboardRoute.badRequest(reason: "failed to read body: \(error)")
        }
        do {
          _ = try HideKeyboardRoute.decode(body)
        } catch let e as HideKeyboardRoute.DecodeError {
          return HideKeyboardRoute.badRequest(reason: "\(e)")
        } catch {
          return HideKeyboardRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: HideKeyboardRoute.success(ok: false)
        ) {
          let ok = await hideKeyboardHandler()
          return HideKeyboardRoute.success(ok: ok)
        }
      }
    }

    // POST /input-text. Decodes InputTextRequest ({"text": <string>},
    // same body the Kotlin runner accepts), calls inputTextHandler, wraps
    // the result in {ok:<bool>}. Guarded so an NSException inside the
    // handler surfaces as `ok:false` instead of escaping the closure. No
    // `?include=` query — typing targets the focused element, not a
    // selector-resolved one.
    if let inputTextHandler {
      await server.appendRoute("POST /input-text") { request in
        let body: Data
        do {
          body = try await request.bodyData
        } catch {
          return InputTextRoute.badRequest(reason: "failed to read body: \(error)")
        }
        let req: InputTextRoute.InputTextRequest
        do {
          req = try InputTextRoute.decode(body)
        } catch let e as InputTextRoute.DecodeError {
          return InputTextRoute.badRequest(reason: "\(e)")
        } catch {
          return InputTextRoute.badRequest(reason: "\(error)")
        }
        return await Self.contextGuardedResponse(request: request,
          fallback: InputTextRoute.success(ok: false)
        ) {
          let ok = await inputTextHandler(req.text)
          return InputTextRoute.success(ok: ok)
        }
      }
    }

    // POST /record/start, POST /record/stop, GET /record/poll.
    // Recorder is gated by `SMIX_RECORD_ENABLED` env in UITest setUp (default
    // off — zero v1.x impact). When the handler triple isn't wired (env off
    // or older UITest target), the routes simply aren't registered.
    if let recordHandlers {
      await server.appendRoute("POST /record/start") { request in
        let body: Data
        do { body = try await request.bodyData } catch {
          return RecordRoute.badRequest(reason: "failed to read body: \(error)")
        }
        do { _ = try RecordRoute.decodeStart(body) }
        catch let e as RecordRoute.DecodeError { return RecordRoute.badRequest(reason: "\(e)") }
        catch { return RecordRoute.badRequest(reason: "\(error)") }
        return await Self.contextGuardedResponse(request: request,
          fallback: RecordRoute.handlerCrashed()
        ) {
          await recordHandlers.start()
          return RecordRoute.startSuccess()
        }
      }
      await server.appendRoute("POST /record/stop") { request in
        let body: Data
        do { body = try await request.bodyData } catch {
          return RecordRoute.badRequest(reason: "failed to read body: \(error)")
        }
        do { _ = try RecordRoute.decodeStop(body) }
        catch let e as RecordRoute.DecodeError { return RecordRoute.badRequest(reason: "\(e)") }
        catch { return RecordRoute.badRequest(reason: "\(error)") }
        return await Self.contextGuardedResponse(request: request,
          fallback: RecordRoute.handlerCrashed()
        ) {
          let events = await recordHandlers.stop()
          return RecordRoute.stopSuccess(events: events)
        }
      }
      await server.appendRoute("GET /record/poll") { _ in
        // /record/poll doesn't touch the target app — bare guardedResponse.
        return await Self.guardedResponse(
          fallback: RecordRoute.handlerCrashed()
        ) {
          let events = await recordHandlers.poll()
          return RecordRoute.pollSuccess(events: events)
        }
      }
    }

    // Run the server concurrently with a stop-observer. On
    // /shutdown the observer calls `server.stop(timeout: 0)` (close socket
    // immediately, no in-flight long connections to preserve — the /shutdown
    // response was already sent). FlyingFox `server.run()` then re-throws the
    // socket-closed/cancellation error from its own catch; that throw, once
    // shutdown was signalled, IS the expected graceful path — swallow it so
    // `runForever` returns normally (→ test_runForever returns → XCTest
    // exit 0). If run() throws WITHOUT a shutdown signal (a genuine startup
    // failure), it is re-thrown unchanged, and if /shutdown is never called
    // the observer awaits forever so run() is never stopped.
    var sawShutdown = false
    // Set the runtime-scoped actors as task-locals so every request
    // handler (spawned as child tasks by FlyingFox) inherits them via
    // Swift's structured concurrency propagation. nil values mean
    // header-less / floor-less behavior.
    try await Self.$currentSimHealth.withValue(simHealthPublisher) {
    try await Self.$currentAppAliveCache.withValue(appAliveCache) {
    try await Self.$currentPopupPacer.withValue(popupPacer) {
    try await Self.$currentUnavailableReasonInferer.withValue(unavailableReasonInferer) {
      do {
        try await withThrowingTaskGroup(of: Void.self) { group in
          group.addTask {
            try await server.run()
          }
          group.addTask {
            await shutdownSignal.wait()
            await server.stop(timeout: 0)
          }
          do {
            try await group.next()
          } catch {
            sawShutdown = await shutdownSignal.didFire
            if !sawShutdown {
              group.cancelAll()
              throw error
            }
          }
          group.cancelAll()
        }
      } catch {
        if !sawShutdown { throw error }
      }
    }}}}
  }
}

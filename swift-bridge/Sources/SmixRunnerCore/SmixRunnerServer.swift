import FlyingFox
import Foundation

#if canImport(CoreGraphics)
import CoreGraphics
#endif

// Wraps FlyingFox HTTPServer so the XCUITest runner does not need to import
// FlyingFox directly — it links SmixRunnerCore only. Subsequent checkpoints
// register more routes on this server (screenshot/swipe/...).
// v1.4 C3 — one-shot graceful-shutdown signal. `/shutdown` fires it; the
// stop-observer task in `runForever` awaits it then calls `server.stop()`.
// Repeated `fire()` is idempotent (continuation resumed at most once) so a
// double POST /shutdown cannot crash. Never fired (no /shutdown call) →
// `wait()` suspends forever → `server.run()` is never stopped → `runForever`
// blocks exactly as before this checkpoint (purely additive opt-in).
// v1.5 C1 S1 — minimal JSON-string escape for selector field re-serialization
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

/// v0.2.1 — per-request context propagated from HTTP headers into every
/// handler that operates on the "app under test".
///
/// **Why this exists**: pre-v0.2.1, SmixRunnerUITests.swift bound
/// `let app = XCUIApplication(bundleIdentifier: bundleId)` once at runner
/// boot time. Every handler closure captured that `app` reference and
/// used it forever, even if the sim's foreground app changed. If
/// Preferences (or any non-target app) happened to be foreground at
/// runner boot (`TargetBundleResolver.resolve` fell back to
/// `com.apple.Preferences`), all subsequent `/tree` / `/find` calls
/// asked XCUITest for a snapshot of an app that was no longer visible,
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
  /// v1.0.2 — the client's session id when it opened one via
  /// `POST /session/open`. When present, `resolveApp()` looks up the
  /// session's cached `XCUIApplication` and skips per-request
  /// `.activate()` regardless of `activate`. Absent → legacy path
  /// (per-request rebind, now rate-limited to at most one activation
  /// per 5 s per bundle-id).
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
  /// v0.2.1 — set inside every route handler via
  /// `SmixRunnerServer.$currentContext.withValue(ctx) { handler() }`.
  /// Handlers read this when they need per-request bundle / activate
  /// info. Uses `@TaskLocal` because handler closures are captured at
  /// server construction time (line 720 in SmixRunnerUITests.swift) —
  /// changing 20+ typealias signatures to thread context explicitly
  /// would be an invasive refactor with no wire-level benefit. Task-
  /// local scoped-mutation gives every handler read access to the
  /// current-request context without touching closure signatures.
  @TaskLocal public static var currentContext: RequestContext = .default

  /// v0.3.1 — canonical main-actor hop for XCUITest APIs that require
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

  public enum TapOutcome: Sendable {
    // v1.1 C3 — `frame` / `appFrame` are populated by the UITest handler so
    // the SDK can host-HID-inject at coord when `mode == .resolve`. Both
    // remain optional so legacy `resolveAndTap` outcomes serialize identical
    // to v1.1 C1.
    case matched(
      label: String,
      stages: TapRoute.TapStages? = nil,
      frame: CGRect? = nil,
      appFrame: CGRect? = nil
    )
    case notFound
  }
  // v1.4 ③-C1 (see-through续修) — `scope` carries the `?include=` query
  // value (nil when absent), identical to SnapshotHandler. nil ⇒ the
  // byte-identical legacy element-resolution source (zero-regression
  // anchor: the SDK runner-client posts /tap WITHOUT a query). "all-windows"
  // ⇒ the UITest target resolves the candidate element from the SAME
  // see-through capture (`buildAllWindowsSnapshot`'s per-window +
  // flat-descendants set) that `/tree?include=all-windows` already uses,
  // so an element the SDK can SEE through a native modal is also tappable.
  public typealias TapHandler = @Sendable (
    TapRoute.TapRequest, _ scope: String?
  ) async -> TapOutcome

  // v1.2 keyboard input — runner-side XCUIElement.typeText path. The smix
  // SDK side delegates to these routes for fill / clear / pressKey because
  // host-HID keyboard injection requires per-key code mapping + modifier
  // handling that is materially larger work than digitizer; runner
  // typeText is the practical path. Each handler returns KeyboardOutcome
  // (success carries per-stage timing; notFound when element resolution
  // fails or — for pressKey — the key string is unsupported).
  //
  // v1.2 C2 — handlers carry sub-stage timing so the SDK side can record
  // hot-spot data alongside TapStages (focus_ms = resolve + focus tap;
  // daemon_send_ms = `_XCT_sendString:` round-trip).
  public enum KeyboardOutcome: Sendable {
    case success(focusMs: UInt32, daemonSendMs: UInt32)
    case notFound
  }

  // v1.4 ③-C1 (see-through续修) — `scope` mirrors TapHandler: nil ⇒ legacy
  // byte-identical focus-tap element source; "all-windows" ⇒ the
  // see-through capture (so a typable field masked by a native modal is
  // still focus-tappable). Same `?include=` query mechanism as /tree.
  public typealias FillHandler = @Sendable (
    _ selector: String, _ text: String, _ scope: String?
  ) async -> KeyboardOutcome
  public typealias ClearHandler = @Sendable (
    _ selector: String, _ scope: String?
  ) async -> KeyboardOutcome
  public typealias PressKeyHandler = @Sendable (
    _ key: String
  ) async -> KeyboardOutcome

  // v1.2 C4 — /find handler. Returns boolean: true = element exists in
  // current XCUIElement query, false = not found. No `.notFound` case
  // because /find is a query, not an action — false IS a valid result.
  // v1.4 ③-C1 (see-through续修) — `scope` mirrors TapHandler: nil ⇒ legacy
  // byte-identical `app.descendants(.any)` query (zero-regression anchor:
  // SDK runner-client posts /find WITHOUT a query); "all-windows" ⇒ the
  // see-through element set so `expect.toBeVisible()` of content behind a
  // native modal returns the truthful answer instead of a masked false.
  public typealias FindHandler = @Sendable (
    _ selectorText: String, _ scope: String?
  ) async -> Bool

  /// v0.3 C1 — XCUI snapshot is constructed inside the UITest target closure
  /// (XCUIApplication.snapshot() is a throwing, blocking API) and returned
  /// as a POCO so SmixRunnerCore stays free of XCTest/XCUI imports.
  /// nil indicates the snapshot is unavailable (app not launched / crashed) —
  /// the server responds with 500 + snapshot_unavailable in that case.
  public typealias SnapshotResult = (root: TreeRoute.A11ySnapshotData, appFrame: CGRect)
  // `scope` 携 `?include=` query 值 (absent ⇒ nil). nil ⇒ legacy single
  // `app.snapshot()` root (byte-identical to 历史路径 = 零回归锚).
  // "all-windows" ⇒ per-window snapshot + flat descendants fallback 合并
  // 进 synthetic root, /tree 可看穿任何 opaque native modal overlay 取到
  // 底下 AX-reachable 内容. handler 拥有 scope 解释权; server 仅 verbatim
  // 转发 raw signal.
  public typealias SnapshotHandler = @Sendable (_ scope: String?) async -> SnapshotResult?

  // v1.4 ③-C1 (third restart) S3.a — system popup sense handler. Per
  // CLAUDE.md §9 #8 + §12.1 popup sense is a core flat capability (peer of
  // /tree / /find / /tap). `scope` carries the `?include=` query value with
  // the same semantics as SnapshotHandler. The handler enumerates
  // SpringBoard alerts / sheets / dialogs and returns POCOs the server
  // serializes via `SystemPopupsRoute.success(popups:)`.
  public typealias SystemPopupsHandler = @Sendable (_ scope: String?) async -> [SystemPopupsRoute.Popup]

  // v1.5 C1 S1 — POST /scroll handler. Drives the runner-side XCUITest
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

  // v1.5 C4b — POST /foreground handler. App-level act capability (no
  // selector / no scope / no loop) — instantiates XCUIApplication
  // (bundleIdentifier:) and calls .activate(). XCUIApplication.activate
  // is documented idempotent: repeated call on frontmost app is no-op.
  // No `?include=` query (foreground is app-level not element-level).
  // Returns `ok:true` on "activate request dispatched" (XCUITest didn't
  // throw mid-call), `ok:false` when the smixGuarded trampoline caught
  // an NSException (vanished bundle / sim mid-crash / etc.).
  public typealias ForegroundHandler = @Sendable (_ bundleId: String) async -> Bool

  // v1.5 C5d' — POST /back handler. App-level navigation pop capability —
  // queries `XCUIApplication.navigationBars.buttons.firstMatch` and calls
  // `.tap()`. The standard XCUITest path is i18n-safe (firstMatch is
  // positional, not label-based). No selector / no scope / no bundleId
  // (back operates on the runner-bound app — caller chose the bound app
  // at runForever() time). Returns `ok:true` when the tap dispatched,
  // `ok:false` when there's no navbar back button on the current screen
  // (top-level / root screens) or smixGuarded caught an NSException.
  public typealias BackHandler = @Sendable () async -> Bool

  /// v1.5 C5i-d — single-swipe gesture handler. Caller (driver host-side loop)
  /// passes direction; handler triggers XCUITest swipe gesture (`app.swipeUp()`
  /// for content direction "down", symmetric for "up") with no probe / no
  /// selector. Returns true on swipe completed, false on swizzler / XCUITest
  /// internal NSException (vanished-element / app terminated mid-gesture).
  public typealias SwipeOnceHandler = @Sendable (_ direction: String, _ scope: String?) async -> Bool

  /// v1.6 c5 — POST /tap-at-norm-coord handler. Coord-based tap via
  /// `app.coordinate(withNormalizedOffset: CGVector(dx:nx, dy:ny)).tap()`
  /// — Apple native UI event chain 触 RN Pressable React event (host-HID
  /// 合成 touch 在 RN modal 不触), 又跳 Apple element query ordering 排序歧义
  /// (host 提供具体 coord, runner 不 re-query). Returns true on tap dispatched,
  /// false on smixGuarded NSException.
  public typealias TapAtCoordHandler = @Sendable (_ nx: Double, _ ny: Double) async -> Bool

  /// v5.3 c4 — POST /tap-by-id handler. Resolves an element by accessibility
  /// identifier and invokes `XCUIElement.tap()` (the XCTest gesture-recognizer
  /// chain). Lets the SDK opt into the path that drives SwiftUI .sheet /
  /// .alert / .confirmationDialog / .fullScreenCover binding actions —
  /// the default host-HID-at-coord tap lands on the frame but doesn't fire
  /// SwiftUI's onTap closure for modal dismiss buttons (v5.3 c3 (b) findings).
  /// Returns true when the element was found and tapped, false when not
  /// found (no element matching the identifier) or when a smixGuarded
  /// NSException surfaces.
  public typealias TapByIdHandler = @Sendable (_ identifier: String) async -> Bool

  /// v5.19 c1a — POST /find-text-by-ocr handler. Apple Vision OCR (VNRecognize
  /// TextRequest) over the current XCUIScreen screenshot. Returns the
  /// matching text observation's bounding box in UIKit-normalized
  /// `(nx, ny, w, h)` (top-left origin, y-down, [0,1]). nil when no match.
  /// L5 sense layer for a11y-less + i18n initiative — covers ~40-50% of
  /// "third-party lib without testID but with visible text" scenarios.
  public typealias FindTextByOcrHandler = @Sendable (
    _ text: String, _ locales: [String], _ recognitionLevel: String
  ) async -> (Double, Double, Double, Double)?

  /// v5.2 c1 — POST /swipe-at-norm-coord handler. Coord-based swipe gesture
  /// via Apple native event chain `XCSynthesizedEventRecord` +
  /// `XCPointerEventPath` (`initForTouchAtPoint:` → `moveToPoint:atOffset:`
  /// → `liftUpAtOffset:`). Companion to `TapAtCoordHandler` under §9 #3
  /// partial-lift escape hatch. Returns true on swipe dispatched, false on
  /// smixGuarded NSException / synthesize error.
  public typealias SwipeAtCoordHandler = @Sendable (
    _ fromNx: Double, _ fromNy: Double, _ toNx: Double, _ toNy: Double
  ) async -> Bool

  /// v5.2 c3 — POST /double-tap handler. XCUIElement.doubleTap() public API
  /// path (RN-modal-not-firing 留 mode 切换 §10 决策, c3 单路径).
  /// Returns true on double-tap dispatched; false on smixGuarded NSException
  /// or element-not-found.
  public typealias DoubleTapHandler = @Sendable (
    _ selectorText: String
  ) async -> Bool

  /// v5.2 c3 — POST /long-press handler. XCUIElement.press(forDuration:)
  /// public API; duration 单位 ms (调用方负责 ms → seconds 转换).
  public typealias LongPressHandler = @Sendable (
    _ selectorText: String, _ durationMs: UInt32
  ) async -> Bool

  /// v5.2 c5 — POST /set-orientation handler. XCUIDevice.shared.orientation
  /// public XCUI API (framework documented property; §9 #6 dlsym 不变量
  /// 在此不触发). Returns true on dispatch, false on smixGuarded NSException.
  public typealias SetOrientationHandler = @Sendable (
    _ orientation: String
  ) async -> Bool

  // v1.5 c5g'' — POST /hide-keyboard handler. App-level keyboard dismiss
  // capability — queries `XCUIApplication.shared.keyboards.firstMatch` and
  // calls `.swipeDown()` (XCUITest standard portable API; no private API).
  // Idempotent: if no keyboard is on screen, returns `ok:true` (no-op).
  // No selector / no scope / no bundleId — keyboard is frontmost-app scope.
  // Returns `ok:true` when the dismiss dispatched (or keyboard absent =
  // already-dismissed), `ok:false` when smixGuarded caught an NSException.
  public typealias HideKeyboardHandler = @Sendable () async -> Bool

  // v4.2 c1 — G9 act side. POST /system-popup-action {popupId, buttonId}.
  // Companion to SystemPopupsHandler (sense side). The handler walks the
  // same enumerate scan order — SpringBoard alerts → SpringBoard sheets
  // → bound-app alerts/sheets/dialogs/popovers — matches popup + button
  // by the id derivation `collectSystemPopups` uses
  // (container.identifier fallback "popup-N"; b.identifier fallback
  // "b-N"), and taps the matched button via the v1.8 c2 EventSynthesizer
  // + v4.0 c3 daemonProxySynthesize dlsym chain. .found ⇒ 200 ok envelope;
  // .notFound (popup or button id missed, or synthesize raised) ⇒ 404
  // not_found envelope echoing the input ids.
  public enum SystemPopupActionOutcome: Sendable {
    case found
    case notFound
  }
  public typealias SystemPopupActionHandler = @Sendable (
    _ popupId: String, _ buttonId: String
  ) async -> SystemPopupActionOutcome

  // v7.9 c1 — SelectResolve handler dispatches the 3 /select/resolve*
  // endpoints. Single closure receives the Request + Action discriminator
  // and returns a Result variant matching the action. In production
  // SmixRunnerUITests injects a closure that calls
  // SmixCoreFFIBindings.resolveSelector / resolveSelectorCount /
  // resolveSelectorLabels. Throws → 500 ffiError envelope.
  public typealias SelectResolveHandler = @Sendable (
    _ request: SelectResolveRoute.Request,
    _ action: SelectResolveRoute.Action
  ) async throws -> SelectResolveRoute.Result

  // v2.0 c2 — record route handlers triple. /record/start enables the
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

  /// v1.0.3 — outcome of a session-open request from the UITest handler's
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

  /// v1.0.3 — outcome of a session-close request. `ok` is always true for
  /// known sessions; unknown sessions also return true (idempotent close).
  public struct SessionCloseOutcome: Sendable {
    public let ok: Bool
    public init(ok: Bool) {
      self.ok = ok
    }
  }

  /// v1.0.3 — outcome of a renew-activation request. `notFound = true`
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

  /// v1.0.4 §D5 — /session/close-all outcome.
  public struct SessionCloseAllOutcome: Sendable {
    public let closed: Int
    public init(closed: Int) {
      self.closed = closed
    }
  }

  /// v1.0.4 §D14 — /session/relaunch-app outcome.
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

  /// v1.0.3 — session lifecycle handlers. Triple bundled (vs three
  /// independent typealiases) because they share the session-table state
  /// in the UITest target. v1.0.4 extends with close-all + relaunch-app.
  public struct SessionHandlers: Sendable {
    public let open: @Sendable (SessionRoute.OpenRequest) async -> SessionOpenOutcome
    public let close: @Sendable (SessionRoute.CloseRequest) async -> SessionCloseOutcome
    public let renew: @Sendable (SessionRoute.RenewRequest) async -> SessionRenewOutcome
    /// v1.0.4 §D5 — invoked by `POST /session/close-all`.
    public let closeAll: @Sendable () async -> SessionCloseAllOutcome
    /// v1.0.4 §D14 — invoked by `POST /session/relaunch-app`.
    public let relaunchApp: @Sendable (SessionRoute.RelaunchRequest) async -> SessionRelaunchOutcome
    public init(
      open: @escaping @Sendable (SessionRoute.OpenRequest) async -> SessionOpenOutcome,
      close: @escaping @Sendable (SessionRoute.CloseRequest) async -> SessionCloseOutcome,
      renew: @escaping @Sendable (SessionRoute.RenewRequest) async -> SessionRenewOutcome,
      closeAll: @escaping @Sendable () async -> SessionCloseAllOutcome,
      relaunchApp: @escaping @Sendable (SessionRoute.RelaunchRequest) async -> SessionRelaunchOutcome
    ) {
      self.open = open
      self.close = close
      self.renew = renew
      self.closeAll = closeAll
      self.relaunchApp = relaunchApp
    }
  }

  public init() {}

  // v1.2 C1 — single point of HTTPServer construction. v1.2 invariant #3 requires
  // Runner port localhost-only; FlyingFox's `HTTPServer(port:)` shorthand binds
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

  // v1.2 C2 — DispatchTime delta → ms (truncates fractional). Shared by tap
  // (TapStages) and keyboard (KeyboardOutcome.success) timing paths in the
  // UITest closure.
  public static func msBetween(_ start: DispatchTime, _ end: DispatchTime) -> UInt32 {
    let nanos = end.uptimeNanoseconds &- start.uptimeNanoseconds
    return UInt32(min(nanos / 1_000_000, UInt64(UInt32.max)))
  }

  // v1.4 ③-C1 B3 — server-side route guard (defense-in-depth layer 1).
  //
  // The runner's route closures call into the UITest target's handler
  // closures, which in turn drive XCUITest. When a resolved element
  // vanishes mid-interaction XCUITest fails the running test
  // (SmixRunnerUITests.swift:262 element.tap() → "Failed to get matching
  // snapshot: No matches found"). B1 turns that into a thrown Swift Error
  // via an ObjC trampoline; B3 is the structural backstop that guarantees
  // such an error — or ANY error the response builder raises — never
  // escapes the route closure into FlyingFox and from there into
  // `runForever`'s withThrowingTaskGroup (`:288-309`), which would
  // re-throw it and terminate `test_runForever` (= runner restart, the
  // exact failure this checkpoint exists to kill).
  //
  // Contract: on success the builder's HTTPResponse is returned
  // byte-identical (zero behaviour change → zero regression for every
  // existing route); on throw the route's own error envelope `fallback`
  // is returned and the error is swallowed. Purely additive, no XCUI /
  // XCTest, unit-decidable in SmixRunnerCore.
  public static func guardedResponse(
    fallback: HTTPResponse,
    _ build: () async throws -> HTTPResponse
  ) async -> HTTPResponse {
    do {
      return try await build()
    } catch {
      // v1.6 c2 — surface caught exception type+reason to stderr so c2 dig
      // can identify modal-overlay side-effects (e.g. snapshot 500 on dashboard
      // after `+load` swizzle). Pre-existing behavior was silent swallow.
      FileHandle.standardError.write(
        Data("smix-runner: guardedResponse caught: \(error)\n".utf8))
      return fallback
    }
  }

  /// v0.2.1 — wraps [`guardedResponse`] with per-request context
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

  /// v1.0.3 — register `/session/open`, `/session/close`,
  /// `/session/renew-activation`. Session-lifecycle handlers are the
  /// systemic fix for the activation storm: consumers open once, run
  /// their entire flow against the cached binding, and close on exit.
  /// The per-request `App-Activate: true` path stays as legacy
  /// (rate-limited) for consumers that haven't migrated.
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
    // v1.0.4 §D5 — POST /session/close-all
    await server.appendRoute("POST /session/close-all") { _ in
      return await Self.guardedResponse(
        fallback: SessionRoute.closeAllResponse(closed: 0)
      ) {
        let outcome = await handlers.closeAll()
        return SessionRoute.closeAllResponse(closed: outcome.closed)
      }
    }
    // v1.0.4 §D14 — POST /session/relaunch-app
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
        return SessionRoute.relaunchResponse(ok: outcome.ok, wallMs: outcome.wallMs)
      }
    }
  }

  // v7.9 c1 — register the 3 /select/resolve* endpoints. Public static
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
    tapAtCoordHandler: TapAtCoordHandler? = nil,
    tapByIdHandler: TapByIdHandler? = nil,
    findTextByOcrHandler: FindTextByOcrHandler? = nil,
    swipeAtCoordHandler: SwipeAtCoordHandler? = nil,
    doubleTapHandler: DoubleTapHandler? = nil,
    longPressHandler: LongPressHandler? = nil,
    setOrientationHandler: SetOrientationHandler? = nil,
    recordHandlers: RecordHandlers? = nil,
    selectResolveHandler: SelectResolveHandler? = nil,
    sessionHandlers: SessionHandlers? = nil
  ) async throws {
    let server = Self.makeServer(port: port)
    let shutdownSignal = ShutdownSignal()
    await server.appendRoute("GET /health") { _ in
      HealthRoute.response()
    }

    // v7.9 c1 — register /select/resolve* routes when handler provided.
    if let selectResolveHandler {
      await Self.registerSelectResolveRoutes(server: server, handler: selectResolveHandler)
    }

    // v1.0.3 — register /session/* routes when handlers provided.
    if let sessionHandlers {
      await Self.registerSessionRoutes(server: server, handlers: sessionHandlers)
    }
    // v1.4 C3 — graceful teardown. The handler MUST NOT `await server.stop()`
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
      // v1.4 ③-C1 B3 — the handler drives XCUITest; if a resolved element
      // vanishes mid-tap the trampoline (B1) surfaces a thrown error.
      // Convert it to the route's own notFound envelope instead of letting
      // it escape the closure and kill the runner.
      // v1.4 ③-C1 (see-through续修) — same `?include=` mechanism as
      // GET /tree (`request.query["include"]`, FlyingFox `[QueryItem]`
      // string subscript, method-independent). nil ⇒ legacy element
      // source (zero-regression: SDK posts no query).
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
    await server.appendRoute("GET /tree") { request in
      // v1.4 ③-C1 B3 — snapshotHandler calls XCUIApplication.snapshot()
      // which throws under modal masking; the trampoline surfaces it.
      // unavailable() is the route's own error envelope (legacy: a nil
      // snapshot already mapped here, so a thrown snapshot is wire-equivalent).
      return await Self.contextGuardedResponse(request: request,fallback: TreeRoute.unavailable()) {
        guard let snap = await snapshotHandler(request.query["include"]) else {
          return TreeRoute.unavailable()
        }
        let payload = TreeRoute.serialize(
          snap.root,
          appFrame: snap.appFrame,
          logSink: { line in
            FileHandle.standardError.write(Data((line + "\n").utf8))
          }
        )
        // v1.2 C2 — emit tree size + node count as response headers so SDK
        // can record hot-spot data without changing the JSON wire shape.
        return TreeRoute.successWithMeta(
          payload,
          sizeBytes: payload.count,
          nodeCount: TreeRoute.countNodes(snap.root)
        )
      }
    }
    // v1.2 keyboard ops — only register when handlers are provided so
    // legacy UITest targets (older smix versions without fill support)
    // stay binary compatible with the v1.1 SmixRunner.
    // v1.2 P2 — keyboard ops route helper. WIRE SHAPE supports embedding
    // the post-action AX tree snapshot in the same response (saves the
    // SDK one HTTP round-trip when an `expect` follows a fill/clear/
    // pressKey), but on iOS 26 sim `XCUIApplication.snapshot()` costs
    // 80-150ms per call — heavier than the /tree round-trip it saves.
    // Empirical bench (v1.2 complex flow, 30 steps × 7 fills): enabling
    // snapshot-in-response REGRESSED per-step from 441ms (P1 alone) to
    // 487ms (+10%). Capability is preserved in the wire (the
    // `KeyboardRoute.successWithTree` builder + `tree` response field)
    // and SDK-side cache (`SimctlDriver.cacheTreeIfPresent` +
    // `cachedTree` TTL) so future versions can re-enable when snapshot
    // cost drops (e.g., shallower snapshot, batched event sequence).
    // Current behaviour: always emit plain {"ok":true} envelope, no tree.
    // v1.2 C2 — success envelope. P2 tree-in-response capability is preserved
    // in `KeyboardRoute.successWithTree` for future re-enable but unused here
    // (snapshot regression cited above). Current path emits stages only.
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
        let outcome = await fillHandler(
          req.selector.text, req.text, request.query["include"])
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

    // v1.2 C4 — /find lets the SDK side ask "does this selector match" without
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
        // v1.4 ③-C1 (see-through续修) — same `?include=` mechanism as
        // GET /tree. nil ⇒ legacy `app.descendants(.any)` (zero-regression
        // anchor: SDK posts /find with no query).
        let found = await findHandler(
          req.selector.text, request.query["include"])
        return FindRoute.success(found: found)
      }
    }

    // v1.4 ③-C1 (third restart) S3.a — `GET /system-popups[?include=]`.
    // Core-flat popup sense; SDK runner-client posts without a query for
    // the legacy nil scope; `?include=all-windows` reaches the handler
    // verbatim (same mechanism as /tree). Sense failure ⇒ guard returns
    // the empty-popups envelope (NOT 5xx) so the upper layer reads
    // "nothing observed right now" rather than a protocol error.
    if let systemPopupsHandler {
      await server.appendRoute("GET /system-popups") { request in
        return await Self.contextGuardedResponse(request: request,
          fallback: SystemPopupsRoute.success(popups: [])
        ) {
          let popups = await systemPopupsHandler(request.query["include"])
          return SystemPopupsRoute.success(popups: popups)
        }
      }
    }

    // v4.2 c1 — POST /system-popup-action. Decodes Request {popupId,
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

    // v1.5 C1 S1 — POST /scroll. Decodes the ScrollRequest, drives the
    // runner-side XCUITest swipe-until-visible loop via scrollHandler,
    // and emits the matched/swipes envelope. Guarded so a vanished-element
    // failure inside the handler (the same mid-interaction XCUITest hazard
    // /tap and /fill already protect against) surfaces as
    // `matched=false, swipes=0` instead of escaping the closure into
    // FlyingFox's withThrowingTaskGroup. maxSwipes / timeoutMs are
    // hardcoded defaults (30 / 30_000ms) — caller-tunable via SDK opts
    // would be a future surface; cold-plan v1.5 §40 anchors these.
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

    // v1.5 C4b — POST /foreground. Decodes ForegroundRequest, calls
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

    // v1.5 C5d' — POST /back. Decodes BackRequest (empty body OK), calls
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

    // v1.5 c5i-d — POST /swipe-once. Single-swipe gesture, no probe, no
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

    // v1.6 c5 — POST /tap-at-norm-coord. Coord-based tap via
    // `app.coordinate(withNormalizedOffset:).tap()`. Body `{"nx", "ny"}` ∈ [0,1].
    // 合 host-side DFS-first resolve + runner native UI event chain.
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

    // v5.3 c4 — POST /tap-by-id {"id":"<a11y-id>"}. XCUIElement.tap() via
    // the XCTest gesture-recognizer chain for SwiftUI .sheet/.alert/
    // .confirmationDialog/.fullScreenCover dismiss buttons that the default
    // host-HID-at-coord path can't trigger. Parallel to /tap-at-norm-coord —
    // /tap stays untouched so v1 41-段 baseline is unaffected.
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

    // v5.19 c1a — POST /find-text-by-ocr. Apple Vision OCR over current
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

    // v5.2 c1 — POST /swipe-at-norm-coord. From-to coordinate swipe gesture
    // via Apple native event chain (XCSynthesizedEventRecord +
    // XCPointerEventPath moveToPoint / liftUp). Body
    // `{"fromNx","fromNy","toNx","toNy"}` ∈ [0,1]. §9 #3 partial-lift escape
    // hatch companion to /tap-at-norm-coord.
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

    // v5.2 c3 — POST /double-tap. XCUIElement.doubleTap() public API path.
    // Body {selector: {text}}. Sibling of /tap envelope (no stages — c3 走
    // 单路径不细分 timing).
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
          fallback: DoubleTapRoute.success(ok: false)
        ) {
          let ok = await doubleTapHandler(req.selectorText)
          return DoubleTapRoute.success(ok: ok)
        }
      }
    }

    // v5.2 c3 — POST /long-press. XCUIElement.press(forDuration:) public API.
    // Body {selector: {text}, durationMs: N}. duration 单位 ms.
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
          fallback: LongPressRoute.success(ok: false)
        ) {
          let ok = await longPressHandler(req.selectorText, req.durationMs)
          return LongPressRoute.success(ok: ok)
        }
      }
    }

    // v5.2 c5 — POST /set-orientation. XCUIDevice.shared.orientation public
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

    // v1.5 c5g'' — POST /hide-keyboard. Decodes HideKeyboardRequest (empty
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

    // v2.0 c2 — POST /record/start, POST /record/stop, GET /record/poll.
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
          fallback: RecordRoute.startSuccess()
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
          fallback: RecordRoute.stopSuccess(events: [])
        ) {
          let events = await recordHandlers.stop()
          return RecordRoute.stopSuccess(events: events)
        }
      }
      await server.appendRoute("GET /record/poll") { _ in
        // /record/poll doesn't touch the target app — bare guardedResponse.
        return await Self.guardedResponse(
          fallback: RecordRoute.pollSuccess(events: [])
        ) {
          let events = await recordHandlers.poll()
          return RecordRoute.pollSuccess(events: events)
        }
      }
    }

    // v1.4 C3 — run the server concurrently with a stop-observer. On
    // /shutdown the observer calls `server.stop(timeout: 0)` (close socket
    // immediately, no in-flight long connections to preserve — the /shutdown
    // response was already sent). FlyingFox `server.run()` then re-throws the
    // socket-closed/cancellation error from its own catch (HTTPServer.swift
    // :92-110); that throw, once shutdown was signalled, IS the expected
    // graceful path — swallow it so `runForever` returns normally (→
    // test_runForever returns → XCTest exit 0). If run() throws WITHOUT a
    // shutdown signal (a genuine startup failure), it is re-thrown unchanged,
    // and if /shutdown is never called the observer awaits forever so run()
    // is never stopped (byte-identical to before this checkpoint).
    var sawShutdown = false
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
  }
}

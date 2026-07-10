import XCTest
import SmixRunnerCore
import ObjectiveC.runtime
import Vision
import UIKit

// v1.6 c2 — Swift swizzler 仅 maxDepth fallback 入口. modal overlay
// (`snapshotKeyHonorModalViews=0`) 由 ObjC `SmixA11ySwizzle.m` `+load` 永久
// 装 (dyld 阶段, 早于 XCTest framework). 跟 maestro `cli-2.2.0` 双套并行
// 1:1 同源:
//   - ObjC `+load` `XCAXClient_iOS+FBSnapshotReqParams.m` (modal overlay)
//   - Swift `AXClientSwizzler.swift` (maxDepth fallback, ViewHierarchyHandler
//     IllegalArgumentError path 才触发; happy path 不触发)
//
// 修 v1.5 c5i-h 触 iOS 26.5 `unrecognized selector` 真因: c5i-h 改字面
// `Standin.self` 但仍走 ObjC msgSend (在 XCAXClient_iOS instance 上调
// Standin selector). 此版用 `class_getMethodImplementation(Standin.self,
// swizzledSel) + unsafeBitCast` IMP-直调走 C ABI 绕 ObjC msgSend.

private var _overwriteDefaultParameters: [String: Int] = [:]

private final class AXClientStandin: NSObject {
  // IMP-直调 lookup: 用字面 `AXClientStandin.self` (非 type(of: self) — 后者
  // 在 swizzled call 时 self=XCAXClient_iOS instance, 在它上找 Standin
  // selector 不存在). `class_getMethodImplementation` 拿 IMP 后 unsafeBitCast
  // 走 C ABI, 不走 ObjC msgSend 校验 selector 跟 self.class 注册.
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
  // force Standin 进 ObjC runtime table 早 — 避免 setupOnce lazy 触发时
  // `class_getMethodImplementation(Standin.self, ...)` lookup miss.
  fileprivate static let proxy = AXClientStandin()

  /// maxDepth fallback 注入入口. ViewHierarchyHandler 风格 (跟 maestro
  /// `AXClientSwizzler.swift` 同源). setter 第一次访问触 lazy `setupOnce`.
  /// happy path 不触发 (modal overlay 已由 ObjC `+load` 永久装).
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

// v1.2 P1 — fast keyboard path via XCTest private daemon proxy.
// `XCTRunnerDaemonSession.sharedSession.daemonProxy` exposes the
// `_XCT_sendString:maximumFrequency:completion:` selector, which submits a
// string to the on-device test daemon and types it into the currently
// focused element at the requested typing frequency. This is 10-100×
// faster than `XCUIElement.typeText` (which forces a separate XCUITest
// query + isHittable + per-char keyboard event roundtrip), because the
// daemon talks directly to the IOHIDEvent layer inside the sim.
//
// Cross-tool reference: maestro uses the same selector at
// `typingFrequency=10`; smix defaults to 200 (v1.2 C4 experiment from
// 100 — see docs/v1.md decision log) since (a) the daemon throttles
// internally if events overflow, and (b) our flow is non-redactable QA
// scripting (`shouldRedact: false`). 200 verified via complex bench
// (3 iter × 30 step + login-tap + tap-text-selector) with 0 regression.
//
// Invariants honoured (CLAUDE.md §9): no Apple binary patching, no
// cross-process injection — the daemon proxy is XCTest framework's own
// public-ish private API, already loaded in our test target.
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

// v1.2.2 — cache of the currently-focused keyboard field's text length so
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

// v1.4 ③-C1 B1/B2 — runner-lifetime resilience (defense-in-depth).
//
// Root cause (v1.md §7 2026-05-19, code-level): when a resolved element
// vanishes mid-interaction XCUITest's engine fails the running test
// ("Failed to get matching snapshot: No matches found …"). It surfaces
// two ways and the source cannot decide a priori which lands first
// (honest unknown — that is exactly why defense is layered, not bet on
// one): (a) a raw ObjC exception from `_XCTFailureHandler`, and/or (b) an
// `XCTIssue` recorded via `XCTestCase.record(_:)` that fails the test.
//
//   B1 (load-bearing): `SmixRunCatching` (ObjC @try/@catch trampoline,
//        UITest target only) converts (a) into a Swift Error so the
//        handler maps it to its EXISTING wire shape — `.notFound` (tap),
//        `nil` (snapshot → 500 snapshot_unavailable), `false` (find).
//        Wire shape unchanged.
//   B2 (backstop): `record(_:)` is overridden. A handler sets
//        `inHandlerSpan` only around its XCUITest calls; while set, a
//        recorded issue is written to stderr (AI-readable, CLAUDE.md
//        §9.5) and `super.record` is NOT called, so the issue does not
//        fail `test_runForever`. OUTSIDE that span (real setUp / launch
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

/// Run `body` inside the ObjC exception trampoline AND the B2 handler
/// span. Returns `nil` when the XCUITest engine failed (vanished element
/// → caught NSException); the caller maps `nil` to its existing
/// element-not-found wire shape. On success returns the body's value.
/// The thrown NSException reason is written to stderr (AI-readable).
///
/// v1.4 ③-C1 (sc4 regression fix): `SmixRunCatching` marshals `body`
/// onto the main thread before running it. FlyingFox route closures run
/// off-main on the cooperative pool; baseline unwrapped `element.tap()`
/// worked because XCUITest internally marshals the touch dispatch, but
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
// 当 bound-app 前有 native modal overlay 时, iOS accessibility 把覆盖层下
// 的内容从 `app.descendants(matching: .any).matching(predicate)` 单 app
// 元素树里 MASK 掉 → /find /tap 返 404. 但 masked 内容对 XCUITest 低层
// 扁平枚举 (`descendants(.any).allElementsBoundByAccessibilityElement`
// + per-window descendants) 可达——这就是 `buildAllWindowsSnapshot`
// 收的元素集. 本 resolver 从该 see-through 集枚举, 第一个满足
// `label == text OR identifier == text` 的元素被返回, 让 /find /tap 透过
// 覆盖层达到 /tree?include=all-windows 已有的同款触达范围.
//
// nil scope (no `?include=`) ⇒ the caller keeps its byte-identical
// legacy `app.descendants(.any)` query — the zero-regression anchor: the
// SDK runner-client posts /find & /tap WITHOUT a query, so every existing
// flow resolves through the unchanged code path.
//
// Why a flat probe and not `.matching(predicate)` over the see-through
// set: an XCUIElementQuery's `.matching` re-runs against the live single
// app-element tree (the very tree the modal masks). The see-through
// reach only exists at the per-element `allElementsBoundByAccessibility
// Element` / per-window-`descendants` enumeration layer, so we must
// enumerate THERE and test each element's `label` / `identifier`
// in-process. Each `.label` / `.identifier` read is wrapped by the
// caller's `smixGuarded` span (B1 main-thread trampoline) so a vanished
// element during enumeration maps to the existing not-found wire shape,
// never a runner kill.
private func firstSeeThroughMatch(
  app: XCUIApplication, text: String
) -> XCUIElement? {
  func matches(_ el: XCUIElement) -> Bool {
    // `.exists` is required before `.label`/`.identifier`; a stale handle
    // from a prior enumeration frame can otherwise throw. Mirrors the
    // `guard el.exists` in buildAllWindowsSnapshot.
    guard el.exists else { return false }
    return el.label == text || el.identifier == text
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
  //    alert button (operation layer / core flat capability per CLAUDE.md
  //    §12.1). Same `label == text OR identifier == text` matcher; same
  //    `smixGuarded` span at the caller guards mid-enumeration vanish.
  let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
  let sbDesc = springboard.descendants(matching: .any)
    .allElementsBoundByAccessibilityElement
  for el in sbDesc where matches(el) { return el }
  return nil
}

// v1.4 ③-C1 (third restart) S3.a — SpringBoard popup enumeration. Per
// CLAUDE.md §9 #8 + §12.1 popup sense is a core flat capability; this
// helper returns the active SpringBoard alerts / sheets / dialogs as
// `SystemPopupsRoute.Popup` POCOs the route serializer wraps in the
// `{"ok":true,"popups":[…]}` envelope. Role classification is locale-
// invariant: only `userTestingAttributes` predicates are consulted
// (the same Apple-internal AX test attribute the setUp interruption
// monitor uses to find the cancel button at line ~322). Dangerous flag
// = role==destructive OR DANGEROUS_LABEL_TOKENS hit (structural fallback,
// mirrored verbatim in src/core/popup-patterns.ts).
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

// /system-popups core sense — 三档枚举, locale-invariant + runtime-agnostic:
//
//   1. SpringBoard (`com.apple.springboard`) 的 `.alerts/.sheets/.dialogs/
//      .popovers`——任何系统层 modal (custom scheme confirm / local network /
//      tracking transparency / 系统警告等).
//   2. bound-app 的 `.alerts/.sheets/.dialogs/.popovers`——iOS standard
//      modal 四类 (XCUIElement.ElementType raw 7/5/8/18); app-side 任何
//      标准 modal 容器在此命中.
//   3. bound-app 的 NON-main window (`app.windows[i≥1]`)——非标准 modal
//      兜底: XCUIElement.ElementType.window (raw 9) 任何自绘 overlay /
//      自定义 portal / 第三方 toast 容器都在 binding index ≥ 1 出现; 主
//      app 窗口在 index 0, 永不是 popup. 这是 XCUITest 提供的结构性
//      (locale-invariant) 主-vs-overlay 判定.
//
// `source` 字段为各 popup 携 process bundleId, 上层据此区分系统 / 在-app.
// `outcomeHint` 仅 SpringBoard scheme-confirm 模式自动标 (locale-invariant
// userTestingAttributes 路径); 其它 popup 留 null, 决策权交回上层
// (CLAUDE.md §12.1: 任何 label-keyed pattern 禁入, 未来若需结构性识别新
// pattern 必须经 userTestingAttributes / SF Symbol identifier / 容器
// topology——绝不英文 label 字面).
private func collectSystemPopups(
  app: XCUIApplication,
  bundleId: String,
  includeAllWindows: Bool = false
) -> [SystemPopupsRoute.Popup] {
  let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
  var out: [SystemPopupsRoute.Popup] = []
  // v1.4 ③-C3 v2 — bound-app window 内 button/staticText 列举上限. RN
  // expo-dev-menu 等重 native overlay 含 50+ button 时, `.allElements
  // BoundByAccessibilityElement` + per-element `.exists` / `.label` /
  // `.identifier` 调用各耗 ~1-2s, 总耗 60-120s 超 SDK drainPopups budget +
  // FlyingFox 15s socket timeout 致整链卡死. 加结构性限: 单 container 最
  // 多列 40 button + 20 staticText (alert/sheet/dialog/popover 自然有限,
  // 远低于此; dev-menu/window 超限即截尾, decide policy 仍按 role +
  // outcomeHint 工作: 默认 affirmative-by-hint 在 buttons[0..N] 内已可找
  // 到; RN dismiss-by-xmark 同源 — xmark 通常 buttons[0] 或前几个). 截尾
  // 只丢冗余 button 元数据, 不丢已识别的 popup-handling 必需信号.
  let consumeMaxButtons = 40
  let consumeMaxStaticTexts = 20
  // C-fix — collectSystemPopups 跨容器整体 wall-clock budget。consume() 内
  // 已有局部 5s buttons / 3s texts budget, 但缺跨容器 deadline: 多容器
  // (springboard.alerts/sheets + app.alerts/sheets/dialogs/popovers) 累加
  // 可超 FlyingFox 15s socket timeout 致 runner 主线程 hang。11s < 15s 留
  // 余量, 每容器 consume 入口检查 deadline, 到点停后续容器返回 partial。
  let collectSystemPopupsBudgetSeconds: TimeInterval = 11.0
  let collectStart = Date()
  func budgetExceeded() -> Bool {
    Date().timeIntervalSince(collectStart) >= collectSystemPopupsBudgetSeconds
  }
  func consume(
    _ container: XCUIElement, fallbackType: String, source: String
  ) {
    if budgetExceeded() { return }
    // C-fix — snapshot-based enumeration: 单次 container.snapshot() 拿整棵
    // 子树, 之后纯内存遍历 dictionaryRepresentation 读 button/staticText 的
    // label/identifier/userTestingAttributes + container 自身 elementType/
    // identifier。根因 (本 session 实证): modal alert 弹出状态下 XCUITest 每次
    // live 元素属性访问 (.label/.exists/.identifier/.elementType/predicate
    // query) 各触发一次 full a11y snapshot ~1.2s; 旧 per-element 路径单容器累计
    // N×1.2s >15s 撞 FlyingFox socket timeout 致 runner 主线程 hang (旧
    // attribute-only query 版仍对 matched element 做 live .label/.exists, 未
    // 消除 per-element 访问)。改为单容器仅 1 次 live 访问 (snapshot, 隐含
    // existence 校验 — try? 失败即视容器消失 return, 取代旧 guard
    // container.exists), 其余全内存, 单容器耗时与元素数 N 无关。
    guard let snap = try? container.snapshot() else { return }
    let dict = snap.dictionaryRepresentation
    func dictVal(_ k: String) -> Any? { dict[XCUIElement.AttributeName(rawValue: k)] }

    var buttonNodes: [(label: String, id: String, frame: CGRect)] = []
    var textLabels: [String] = []
    collectPopupNodes(dict, buttons: &buttonNodes, texts: &textLabels)

    // role 分类: userTestingAttributes 不在 XCUIElementSnapshot
    // .dictionaryRepresentation (本 session 实测 utaPresent=false), 只能 live
    // predicate query 拿。仅对 SpringBoard native alert 做 — 其 a11y 树简单,
    // container-level 2 次 attribute query 快 (非旧 per-button 2N 次), 跟 v1.4
    // ③-C1 scheme-confirm 同源, 不触 hang。in-app RN modal **跳过**: RN button
    // 不设 userTestingAttributes (旧 live query 对其也恒判 default, 无收益), 且
    // 其 live element 访问慢正是 c3 hang 根因 (已由上面单次 snapshot 路径消除)。
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
    // container elementType 从 dict root 读 (避免 container.elementType live
    // 访问)。XCUIElement.ElementType raw: alert=7 sheet=5 dialog=8 popover=18。
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
  // v1.4 ③-C3 v2 — bound-app 非主 window 枚举仅在 includeAllWindows 显式
  // opt-in 时走 (默认跳过). RN expo-dev-menu / 自绘 modal 等重 native overlay
  // 在此分支; 默认不查省去 ~60-120s 全列耗 (per consume() 内每 button +
  // staticText `.exists/.label/.identifier` 调用 ~1-2s, dev-menu 50+ 元素).
  // SDK 默认 `app.system.drainPopups()` 不传 include ⇒ scope nil ⇒
  // includeAllWindows=false ⇒ 廉价 SpringBoard scheme-confirm 走通; 显式传
  // `app.system.drainPopups({include:'all-windows'})` 才进重 sense (作者
  // 自主选).
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

// v4.2 c1 — G9 act side. Walks the same SpringBoard / bound-app scan
// order `collectSystemPopups` uses (alerts → sheets → bound-app
// alerts/sheets/dialogs/popovers), matches popup by id derivation
// `popup-N` (global out-count) ⇋ container.identifier, then matches
// button by id derivation `b-N` (intra-popup index) ⇋ b.identifier.
// On match, taps via SmixEventRecord + SmixRunnerDaemonProxy.shared
// .synthesize (the v1.8 c2 + v4.0 c3 daemonProxySynthesize dlsym
// chain, §9 #6) at the matched button's frame center. Returns .found
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
      // v4.3 c1 — snapshot-based: 单次 el.snapshot() 拿 container 子树, 从
      // dictionaryRepresentation 内存读 identifier + button (id, frame), 消除
      // per-element .exists / .identifier / .frame live 访问。modal 弹出状态下
      // 每次 live 访问各触发 ~1.2s full a11y snapshot, per-element 累计撞 FlyingFox
      // 15s socket timeout — 镜像于 c-fix 已修的 enumerate (collectSystemPopups)
      // hang。snapshot() 隐含 existence 校验: try? 失败视容器消失, popupIdx 仍
      // 递增以保持与 enumerate (out-count) 的 popup-N id derivation 对齐。
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
  // v1.5 c5i-f — try `continueAfterFailure = true` (跟 maestro 同源) 实测
  // 退化 (2/5). XCTest internal swallow 有 side effect 跟 smix 既有 `record(_:)`
  // override 协同失稳, 留 v1.6 议题. 不设此 default = XCTest default 行为.
  //
  // v1.4 ③-C1 B2 — issue-recording backstop. While a handler's XCUITest
  // span is active a recorded XCTIssue (the second way a vanished element
  // surfaces — `_XCTFailureHandler` may `record(_:)` instead of `@throw`)
  // is swallowed (logged AI-readable, not propagated) so one failed
  // interaction does not terminate `test_runForever` = does not restart
  // the runner. Outside the span the default behaviour is preserved: a
  // genuine setUp/launch failure still fails the test loudly.
  override func record(_ issue: XCTIssue) {
    if HandlerSpanFlag.shared.inSpan {
      FileHandle.standardError.write(
        Data("smix-runner: swallowed in-handler XCTIssue: \(issue.compactDescription)\n".utf8))
      return
    }
    super.record(issue)
  }

  // R4.c (audit #4) — UI-interruption monitor deleted. The pre-R4
  // monitor tapped the affirmative button of every two-button
  // SpringBoard alert exposing a `cancel-button` AX attribute. That
  // bypassed `/system-popups` decision layer (CLAUDE.md §9 #8 + §12 —
  // decision権 belongs to the upper layer, not driver). The ③-C1 third
  // restart proved e2e does not depend on this monitor: `popup_decided
  // = rn-window-dismiss` runs entirely through the core `/system-popups`
  // sense path + `/tap` act path with the AI / e2e author making the
  // decision. Settings / v1-acceptance flows never trigger a two-button
  // alert so the monitor was inert there. Removing it keeps the runner
  // in the three-layer architecture without a hidden actor.

  func test_runForever() async throws {
    // v1.6 c2 — modal overlay 由 ObjC `SmixA11ySwizzle.m` `+load` 永久装
    // (dyld 阶段, 早于此 test entry). Swift `AXClientSwizzler` 仅 maxDepth
    // fallback 入口 (ViewHierarchyHandler IllegalArgumentError path), happy
    // path 不主动 install. 跟 maestro `cli-2.2.0` 双套并行同源.

    // v2.0 c2 — EventRecorder swizzle gated by `SMIX_RECORD_ENABLED` env.
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
    // v0.3.1 — `test_runForever` is an XCTest test method; XCTest
    // guarantees test methods execute on the main queue, so `.launch()`
    // / `.activate()` here are on-main by construction (no explicit
    // MainActor hop needed). Any XCUITest mutation added inside this
    // setup block inherits the same guarantee. Handlers below use
    // `resolveApp()` (async) with SmixRunnerServer.onMain instead.
    switch LaunchModeResolver.resolve(env: ProcessInfo.processInfo.environment) {
    case .launch: app.launch()
    case .activate: app.activate()
    }

    // v1.1 C3 S2.5 — capture app.frame ONCE per runner lifetime. XCUIApplication
    // .frame internally triggers a light snapshot (~50-150ms); for our default
    // tap dispatch (Settings is portrait-only, window doesn't resize across taps)
    // the value is invariant. Cache here so each tapHandler invocation only pays
    // for element resolution + element.frame read, not the global app.frame read.
    //
    // v0.2.1 — cache is now per-bundle so a client that switches the target app
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
    // v1.0.2 — per-bundle-id activation rate limit. Prior to v1.0.2
    // every request with `App-Activate: true` triggered an
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
    // v1.0.3 — session table. Sessions are opened via `POST /session/open`
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
    }
    let sessions: NSLock = NSLock()
    var sessionTable: [String: SessionEntry] = [:]
    let renewCooldown: TimeInterval = 2.0
    let resolveApp: @Sendable () async -> XCUIApplication = {
      let ctx = SmixRunnerServer.currentContext
      // Session-Id header path — hit the session table, no activation.
      if let sid = ctx.sessionId {
        sessions.lock()
        let entry = sessionTable[sid]
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

    // v0.2.1 — per-target frame lookup. Callers that used the module-scope
    // `cachedAppFrame` (v0.2.0) pass their resolved app; this returns the
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

    // v0.2.0 compat shim — the tapHandler path below reads
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
    let server = SmixRunnerServer()
    try await server.runForever(
      port: resolvedPort,
      tapHandler: { req, scope in
        // v0.2.1 — per-request target-app rebind.
        let app = await resolveApp()
        // v1.2.2 — tap navigates / changes focus; invalidate keyboard cache.
        KeyboardCache.shared.invalidate()
        // C2 selector subset: text → match by label OR identifier.
        // Settings rows are XCUIElementTypeCell, not button; using `.any` covers both.
        // v1.1 C1 — 3 stage timers around resolve / tap call / total. SMIX_C1_STAGE_LOG
        // env on the SDK side decides whether stages reach disk; runner always returns
        // them (cheap) so default opt-in path collects data.
        // v1.1 C3 — `.resolve` mode returns the element frame + cached app frame
        // (so SDK can host-HID-inject at coord) and skips element.tap() AND the
        // isHittable check. Rationale: HID injection at coord doesn't need
        // XCUITest's hittable semantic (which forces a redundant snapshot per
        // call); waitForExistence already proves the element is in the AX tree.
        // If the element is in-tree but visually offscreen, the tap will simply
        // miss and the caller's next expect/waitFor will surface it.
        // `.resolveAndTap` keeps isHittable + the legacy synchronous tap.
        // v1.1 C3 S2.7 — sub-stage timers within resolve (wait_existence_ms /
        // frame_read_ms) emitted only for .resolve mode to attribute the
        // measured P50 gap from the theoretical floor.
        let t0 = Date()
        let predicate = NSPredicate(
          format: "label == %@ OR identifier == %@",
          req.selector.text, req.selector.text
        )
        // v1.4 ③-C1 (see-through续修) — nil scope: byte-identical legacy
        // resolution (`query.firstMatch`, resolved OUTSIDE the guard,
        // exactly as before this fix = the zero-regression anchor; the
        // SDK posts /tap with no `?include=`). "all-windows": defer
        // resolution INTO the guard via `firstSeeThroughMatch` (the flat
        // enumeration reads `.label`/`.identifier` on possibly-vanishing
        // handles → must run in the B1 main-thread trampoline span).
        let seeThrough = (scope == "all-windows")
        let query = app.descendants(matching: .any).matching(predicate)
        let element0: XCUIElement? = seeThrough ? nil : query.firstMatch
        let tBeforeWait = Date()
        // v1.1 C3 S2.8 — short-circuit on cached-snapshot exists. The C3
        // bench (S2.7 instrumentation) attributed ~1072ms (99.98%) of
        // resolve_ms to waitForExistence — XCUITest's polling cycle has a
        // ~1s minimum even when the element is already in the live tree
        // (Settings → "General" row). Try the synchronous `.exists`
        // (single snapshot read) first; only fall back to the slow
        // waitForExistence path when the element is genuinely not yet
        // rendered (e.g., mid-navigation, animation still landing).
        //
        // 每个 XCUITest 访问被 ObjC trampoline span (`smixGuarded`) 包裹.
        // 任何状态转换 (overlay 渲染/dismiss、reload、动画 settle) 都可能
        // 让已 resolve 的元素中途消失, XCUITest 此时在 `.tap()` / `.frame` /
        // `.isHittable` 内抛 NSException. trampoline 把抛错映射到 `.notFound`
        // (既有 not-found wire), 保 runner 不崩 (B1 设计: runner-lifetime
        // resilience).
        let outcome: SmixRunnerServer.TapOutcome? = smixGuarded("tap") {
          let element: XCUIElement
          if seeThrough {
            // See-through: resolve from the masked-content-reaching flat /
            // per-window enumeration (same set buildAllWindowsSnapshot
            // uses). Already proven existing during enumeration, so no
            // waitForExistence cycle is needed (and none would help — the
            // masked element never enters the live single-app tree
            // waitForExistence polls).
            guard let m = firstSeeThroughMatch(app: app, text: req.selector.text)
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
          // see-through 路径下 `.isHittable` 不 gate: 它对一个被 native
          // modal 覆盖的 see-through-resolved 元素 (在 AX 树上可达但视觉
          // 被覆盖) 必报 false. read-through 语义明示 "AX reachable +
          // intent 是 THROUGH overlay" → 跳 hittability 改用 element AX
          // 中心 coordinate.tap() (coordinate tap 不 hittability-gated;
          // 注意 iOS hit-testing 仍按 z-order 路由实际触摸, 若 overlay
          // 截走触摸事件则不会触发 underlying onPress——读穿是 sense 能
          // 力, act 仍受 iOS hit-testing 约束). nil scope 走 legacy
          // `.isHittable` + `element.tap()` 路径 (零回归锚); B1 main-thread
          // trampoline 两边都兜底 vanished element.
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
            // v1.4 ③-C1 (sc4 regression fix) — the intervening
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
            // v4.0 c3 G8 fix — RN Pressable JS-thread onPress unreliable
            // through XCUIElement.tap()'s Apple gesture recognizer chain
            // (RN `RCTTouchHandler` UIGestureRecognizer gets that synthesised
            // gesture cancelled or routed past it). Alternative dispatch
            // emits a raw IOKit-level touch event in the resolved element
            // frame centre via `XCTRunnerDaemonSession.daemonProxy._XCT_
            // synthesizeEvent:completion:` (no XCUIElement-owner metadata),
            // which UIKit's standard hit-test routes through RN's gesture
            // chain → Pressable onPress fires. Same daemonProxy path as
            // v1.6 c5 tapAtCoordHandler + v1.8 c2 EventSynthesizer.swift.
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
            label: label.isEmpty ? req.selector.text : label,
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
        // v0.3 C1 — XCUIElement.snapshot() is a throwing, blocking call
        // (~50-100 ms on Settings). Returning nil here causes the server to
        // respond `500 {"ok":false,"error":"snapshot_unavailable"}` — used
        // when the target app has terminated mid-test.
        //
        // v1.4 ③-C1 A — `scope` carries the `?include=` query value.
        //
        //   nil / anything but "all-windows": LEGACY path. A single
        //     `app.snapshot()` root, byte-identical to before this
        //     checkpoint. This is the zero-regression anchor: no param ⇒
        //     the exact same bytes the SDK / host smoke gates already
        //     parse. Guarded so a mid-test crash → nil → 500
        //     snapshot_unavailable (the existing wire shape), never a
        //     thrown error that kills test_runForever.
        //
        //   "all-windows": SEE-THROUGH path. 任何 opaque native modal 都
        //     会让 iOS accessibility 把覆盖层下的内容从 single-app snapshot
        //     mask 掉, 即使 XCUITest 低层扁平枚举仍可达.
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
        // v5.1 c1 — one-shot KVC walk on the snapshot tree to find the
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
        let root = convertSnapshot(snap, rootIdentifierOverride: bundleId, focusHint: focusHint)
        // v1.2.1 — reuse cachedAppFrame (same invariance as v1.1 C3 S2.5
        // tapHandler optimization). app.frame access internally triggers a
        // light snapshot (~50-150ms); Settings is portrait-only and the
        // window doesn't resize across taps so the value is invariant for
        // this runner's lifetime. Saved ~50-100ms per /tree call.
        return (root: root, appFrame: cachedAppFrame)
      },
      // v1.2 keyboard ops — resolve text-only selectors via the same
      // predicate as tapHandler, then call XCUIElement.typeText /
      // .clearAndEnterText / app.typeKey for pressKey. All three are
      // best-effort: false return means element not found (404) or
      // unsupported key (400); the SDK surfaces these via the same
      // ExpectationFailure shape as tap.
      // v1.2 P1 — keyboard ops via XCTest daemon fast path
      // (XCTRunnerDaemonSession.sharedSession.daemonProxy._XCT_sendString...).
      // Daemon proxy types into whatever element is currently focused — much
      // faster than XCUIElement.typeText (which does per-char roundtrips
      // through the test target's main thread). For a non-`_focused_`
      // selector, the handler first taps the matching typable element to
      // give it focus, then submits the string to the daemon.
      fillHandler: { selectorText, text, scope in
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
        let t0 = DispatchTime.now()
        if !selectorText.isEmpty && selectorText != "_focused_" {
          // v1.4 ③-C1 B1 — focus-tap can hit a vanished element (same
          // root cause as tapHandler). Guard the resolve+tap span; a
          // caught failure leaves the field unfocused and the daemon
          // send below simply targets whatever is focused (best-effort,
          // unchanged contract) instead of killing the runner.
          //
          // v1.4 ③-C1 (see-through续修) — nil scope: byte-identical
          // legacy predicate resolution (zero-regression anchor: SDK
          // posts /fill with no `?include=`). "all-windows": when the
          // legacy predicate (which runs against the modal-masked single
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
                      let m = firstSeeThroughMatch(app: app, text: selectorText) {
              if !m.hasFocus { m.tap() }
            }
            return true
          }
        }
        let t1 = DispatchTime.now()
        do {
          try await DaemonKeyboard.shared.sendString(text, typingFrequency: 200)
          let t2 = DispatchTime.now()
          // v1.2.2 — track typed text length (typeText APPENDS to existing
          // field content). clear's hot path reads this to skip the
          // snapshot-triggering focused.value read.
          KeyboardCache.shared.appendFill(text)
          return .success(
            focusMs: SmixRunnerServer.msBetween(t0, t1),
            daemonSendMs: SmixRunnerServer.msBetween(t1, t2)
          )
        } catch { return .notFound }
      },
      clearHandler: { selectorText, scope in
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
        let t0 = DispatchTime.now()
        if !selectorText.isEmpty && selectorText != "_focused_" {
          // v1.4 ③-C1 B1 — guard focus-tap span (vanished-element safe).
          // v1.4 ③-C1 (see-through续修) — nil scope: byte-identical legacy
          // (zero-regression anchor: SDK posts /clear with no `?include=`).
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
                    let m = firstSeeThroughMatch(app: app, text: selectorText) {
              if !m.hasFocus { m.tap() }
            }
            return true
          }
        }
        // v1.2 P3+ — proportional delete count. Previously we always
        // sent max(value.count, 64) deletes = ~640ms minimum at
        // typingFrequency=100. Now scale to actual field content +
        // small overshoot, dropping clear of a 5-char field from
        // ~640ms → ~90ms (7× speedup on this operation).
        //
        // v1.2.2 — hot path: when caller uses `_focused_` AND we have a
        // tracked text length from a prior fill/pressKey, skip the
        // snapshot-triggering `focused.value` read (~50-80ms). Cache is
        // invalidated by tap / non-delete pressKey so an out-of-sync
        // cache resolves to nil → falls back to the safe snapshot path.
        let count: Int
        if selectorText == "_focused_", let cached = KeyboardCache.shared.length {
          count = max(cached + 4, 4)
        } else {
          // v1.4 ③-C1 B1 — the focused.value read triggers a snapshot
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
          // v1.2.2 — field is now empty.
          KeyboardCache.shared.recordClear()
          return .success(
            focusMs: SmixRunnerServer.msBetween(t0, t1),
            daemonSendMs: SmixRunnerServer.msBetween(t1, t2)
          )
        } catch { return .notFound }
      },
      pressKeyHandler: { key in
        let t0 = DispatchTime.now()
        // v5.2 c2 — iOS 硬件按键路径(home / lock / volumeUp / volumeDown)
        // 不是 keyboard event, 走 XCUIDevice public API; 不进 mapping dict.
        // 三种 supported:
        //   home          → XCUIDevice.shared.press(.home, forDuration: 0)
        //   volumeUp/Down → XCUIDevice.shared.press(.volumeUp/.volumeDown, ...)
        // lock 在 iOS sim 上 XCUIDevice.Button 未暴露公开 enum case, simctl 也
        // 无 lock 接口 → 显式 return .notFound + stderr 报 unsupported, 不静默
        // noop ([[priority-quality-perf-over-cost]] + §13)。
        switch key {
        case "home":
          XCUIDevice.shared.press(XCUIDevice.Button.home)
          let t2 = DispatchTime.now()
          KeyboardCache.shared.invalidate()
          return .success(focusMs: 0, daemonSendMs: SmixRunnerServer.msBetween(t0, t2))
        case "lock":
          FileHandle.standardError.write(
            Data("smix-runner: pressKey lock: unsupported on iOS Simulator (no XCUIDevice.Button.lock, simctl 无 lock verb); maestro 同源限制\n".utf8))
          return .notFound
        case "volumeUp", "volumeDown":
          // Apple 明示 XCUIDevice.Button.volumeUp/.volumeDown is unavailable
          // in iOS Simulator(physical iOS device only)。adapter 应在 runtime
          // 层 graceful skip 不到 wire 这一层;真到了说明 adapter 漏检 —
          // stderr 报 + .notFound 保 runner 不崩。
          FileHandle.standardError.write(
            Data("smix-runner: pressKey \(key): unavailable in iOS Simulator (Apple XCUIDevice.Button restriction); adapter 应预先 graceful skip\n".utf8))
          return .notFound
        default: break
        }
        // keyboard-event path (return / delete / tab / space / escape)
        let mapping: [String: String] = [
          "return": XCUIKeyboardKey.return.rawValue,
          "delete": XCUIKeyboardKey.delete.rawValue,
          "tab":    XCUIKeyboardKey.tab.rawValue,
          "space":  XCUIKeyboardKey.space.rawValue,
          "escape": XCUIKeyboardKey.escape.rawValue,
        ]
        guard let raw = mapping[key] else { return .notFound }
        let t1 = DispatchTime.now()
        do {
          try await DaemonKeyboard.shared.sendString(raw, typingFrequency: 200)
          let t2 = DispatchTime.now()
          // v1.2.2 — track keyboard cache: 'delete' decrements; others
          // (return/tab/escape/space) may change focus → invalidate.
          KeyboardCache.shared.recordPressKey(key)
          return .success(
            focusMs: SmixRunnerServer.msBetween(t0, t1),
            daemonSendMs: SmixRunnerServer.msBetween(t1, t2)
          )
        } catch { return .notFound }
      },
      // v1.2 C4 — /find: XCUIElement query for "does this label/identifier
      // exist", without paying the cost of XCUIApplication.snapshot() +
      // serialization. Used by SDK `expect.toBeVisible()` for simple
      // text selectors. Returns boolean.
      findHandler: { selectorText, scope in
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
        // v1.4 ③-C1 B1 — `.exists` triggers a snapshot that can fail
        // under modal masking. Guard it; a caught failure maps to the
        // existing not-found wire shape (`false`), never a runner kill.
        //
        // v1.4 ③-C1 (see-through续修) — nil scope: byte-identical legacy
        // `app.descendants(.any).matching(predicate).firstMatch.exists`
        // (zero-regression anchor: SDK posts /find with no `?include=`).
        // "all-windows": resolve from the same masked-content-reaching
        // see-through set /tree?include=all-windows uses, so
        // `expect.toBeVisible()` of content behind a native modal returns
        // the truthful answer instead of a masked false.
        if scope == "all-windows" {
          return smixGuarded("find-all-windows") { () -> Bool in
            firstSeeThroughMatch(app: app, text: selectorText) != nil
          } ?? false
        }
        return smixGuarded("find") { () -> Bool in
          let predicate = NSPredicate(
            format: "label == %@ OR identifier == %@",
            selectorText, selectorText
          )
          return app.descendants(matching: .any)
            .matching(predicate)
            .firstMatch
            .exists
        } ?? false
      },
      // v1.4 ③-C1 (third restart) S3.a — system popup sense. Per CLAUDE.md
      // §9 #8 + §12.1 popup感知 is a core flat capability; the handler
      // enumerates SpringBoard alerts / sheets / dialogs AND the bound
      // app's own alert / sheet / dialog / popover (the structural iOS
      // modal types, S3.a' bound-app modal extension) into Popup POCOs.
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
      // v4.2 c1 — G9 act side — POST /system-popup-action handler. Walks
      // the same SpringBoard / bound-app scan order collectSystemPopups
      // uses (alerts → sheets → bound-app alerts/sheets/dialogs/popovers)
      // so popup.id derivation (container.identifier fallback "popup-N"
      // by global out-count) and button.id derivation (b.identifier
      // fallback "b-N" by intra-popup index) round-trip from enumerate.
      // Matched button frame center → SmixEventRecord pointer touch →
      // SmixRunnerDaemonProxy.shared.synthesize (v1.8 c2 + v4.0 c3
      // daemonProxy dlsym chain, §9 #6) so SpringBoard alert handlers
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
      // v1.5 C1 S1 — scroll-until-visible. Resolves the target selector
      // (text or id) via the same label==% / identifier==% predicate as
      // findHandler / tapHandler (no xpath, no regex — CLAUDE.md §9.3
      // selector surface). Each loop iteration: probe existence on the
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
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
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
        // (route layer wraps as matched:false). 守 §9.3: only label/id
        // equality, no xpath / regex.
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
             firstSeeThroughMatch(app: app, text: sText!) != nil
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
            // v6.11 c1 — maestro navigation convention (wire = "what to
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
               firstSeeThroughMatch(app: app, text: t) != nil
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
      // v1.5 C4b — POST /foreground handler. Instantiates a fresh
      // XCUIApplication(bundleIdentifier:) — NOT the runner-bound `app` from
      // line 537 (caller-supplied bundleId may differ from runner bound app
      // when test switches apps mid-flow). XCUIApplication.activate is
      // Apple synchronous fire-and-forget; the handler returns true on
      // "activate request dispatched" (XCUITest didn't throw), not on
      // "app truly frontmost" — sense-layer "really in foreground" verification
      // is the caller's responsibility per §12.1 (caller runs app.tree() /
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
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
        // v1.5 c5i-e — multi-strategy back-nav fallback chain (跟 c5h'
        // Multi-strategy pop, mirroring the pattern the keyboard-dismiss
        // path uses.
        // Strategy 1: navigationBars.firstMatch — i18n-safe positional
        //   path; works for nav-stack screens.
        // Strategy 2: swipeRight from the left edge — the iOS
        //   interactive pop gesture; works for RN react-navigation
        //   Modal screens off the nav stack when the modal has
        //   `gestureEnabled: true`.
        let outcome: Bool? = smixGuarded("back") {
          // Strategy 1: navigation bar back button
          let navBars = app.navigationBars
          let firstButton = navBars.buttons.firstMatch
          if firstButton.exists {
            firstButton.tap()
            Thread.sleep(forTimeInterval: 0.5)
            return true
          }
          // Strategy 2: iOS interactive pop gesture (swipe right from left
          // edge). RN screens with `gestureEnabled:true` (default for stack
          // navigator screens, including Modal-presented screens) accept it.
          let leftEdge = app.coordinate(withNormalizedOffset: CGVector(dx: 0.01, dy: 0.5))
          let rightTarget = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.5))
          leftEdge.press(forDuration: 0.1, thenDragTo: rightTarget)
          Thread.sleep(forTimeInterval: 0.5)
          // Best-effort: assume gesture worked if it didn't raise NSException.
          // No "did navigate" sense check — fire-and-forget act (same as
          // Strategy 1 firstButton.tap). Caller verifies post-navigation
          // state via subsequent expect/waitFor.
          return true
        }
        return outcome ?? false
      },
      // v1.5 c5i-d — POST /swipe-once handler. Single XCUITest swipe gesture,
      // no probe, no selector. Driver-side host loop scrollUntilVisible alternates
      // between host-side dict tree probe (driver.tree + resolveSelector) and
      // calls to this handler, bypassing the runner-side query.firstMatch stall
      // on dict-only RN elements which previously triggered FlyingFox 15s
      // handler timeout / /scroll 500. direction "down" = scroll content down
      // (露下方内容) = swipe finger up (app.swipeUp()); symmetric for "up"
      // (跟既有 scrollHandler line 1116-1119 注释一致).
      swipeOnceHandler: { direction, _scope in
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
        let outcome: Bool? = smixGuarded("swipe-once") {
          // v6.11 c1 — all 4 directions follow maestro navigation
          // convention (the wire string names what content to SEE, not
          // the finger gesture direction). swift `XCUIElement.swipe<X>`
          // primitives are the inverse finger gesture, so e.g. "down"
          // (navigate down = see below) → swipeUp (finger up = content
          // moves up). v6.10 c1 had L/R in finger-direction convention
          // by mistake; corrected here to match U/D + Kotlin runner.
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
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
        // v1.5 c5h' user 校准 (2026-05-23): "软件键盘的相关处理, 肯定是
        // smix-core 的能力范围, 要健壮". swipeDown 有时不 dismiss
        // RN TextInput keyboard. Robust multi-strategy: 每步 verify keyboard
        // dismissed after each strategy.
        let outcome: Bool? = smixGuarded("hide-keyboard") {
          guard app.keyboards.firstMatch.exists else { return true }
          // Strategy 1: tap Return/Done/Continue/Search/Go key on keyboard
          for keyName in ["Return", "Done", "Continue", "Search", "Go", "Next", "Enter"] {
            let key = app.keyboards.buttons[keyName]
            if key.exists {
              key.tap()
              Thread.sleep(forTimeInterval: 0.5)
              if !app.keyboards.firstMatch.exists { return true }
            }
          }
          // Strategy 2: tap above keyboard (RN Keyboard.dismiss responds to outside touch)
          let above = app.coordinate(withNormalizedOffset: CGVector(dx: 0.5, dy: 0.15))
          above.tap()
          Thread.sleep(forTimeInterval: 0.5)
          if !app.keyboards.firstMatch.exists { return true }
          // Strategy 3: swipeDown (fallback)
          app.keyboards.firstMatch.swipeDown()
          Thread.sleep(forTimeInterval: 0.5)
          return !app.keyboards.firstMatch.exists
        }
        return outcome ?? false
      },
      // v1.6 c5 → v1.8 c2 — POST /tap-at-norm-coord handler. 走 maestro
      // `cli-2.2.0` 同源 daemonProxy `_XCT_synthesizeEvent:completion:` 私有
      // 路径 (raw IOKit-level event 经 UIKit 标准 hit-test, 触 RN Pressable
      // onPress). v1.6 c5 旧实现 `app.coordinate(withNormalizedOffset:).tap()`
      // 走 XCUI session 中层, event 含 XCUIElement-owner 元数据致 RN list
      // rendering 不触 data fetch (alerts-counting 30s skeleton-loader).
      // 见 `.scratch/v1.8-c1-fix-plan.md` 根因 dig.
      tapAtCoordHandler: { nx, ny in
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
        // 算 physical point (nx × app.frame.width + frame.origin) on main thread
        var px: CGFloat = 0
        var py: CGFloat = 0
        let setupOk = smixGuarded("tap-at-norm-coord-setup") {
          let frame = app.frame
          px = frame.origin.x + frame.size.width * CGFloat(nx)
          py = frame.origin.y + frame.size.height * CGFloat(ny)
          return true
        }
        guard setupOk == true else { return false }

        guard let record = SmixEventRecord(orientation: .portrait) else {
          FileHandle.standardError.write(
            Data("smix-runner: tap-at-norm-coord: XCSynthesizedEventRecord unavailable\n".utf8))
          return false
        }
        let pathAdded = record.addPointerTouchEvent(at: CGPoint(x: px, y: py))
        guard pathAdded else {
          FileHandle.standardError.write(
            Data("smix-runner: tap-at-norm-coord: XCPointerEventPath unavailable\n".utf8))
          return false
        }
        do {
          try await SmixRunnerDaemonProxy.shared.synthesize(record: record)
          return true
        } catch {
          FileHandle.standardError.write(
            Data("smix-runner: tap-at-norm-coord: synthesize error: \(error)\n".utf8))
          return false
        }
      },
      // v5.3 c4 — POST /tap-by-id handler. XCUIElement.tap() via the XCTest
      // gesture-recognizer chain for SwiftUI .sheet / .alert /
      // .confirmationDialog / .fullScreenCover dismiss buttons. The default
      // /tap-at-norm-coord path injects an IOKit-level touch at the button's
      // frame, but iOS modal-window UIWindow hit-testing routes the event to
      // the wrong target when the modal is owned by a separate window scene
      // — SwiftUI's dismiss-binding closure never fires (v5.3 c3 (b)).
      // XCUIElement.tap() goes through XCTRunnerDaemonSession against the
      // resolved element handle, so the gesture lands on the actual SwiftUI
      // hit-target regardless of window scene topology.
      tapByIdHandler: { identifier in
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
        // Resolve element + (v5.12 c1) swipe-scroll into view + compute the
        // post-scroll frame center. All XCUI ops sit inside smixGuarded
        // (main-thread + NSException trampoline). The actual tap dispatch
        // runs outside via the IOHID daemonProxy synthesize path so SwiftUI
        // bindings fire on iOS 17+ (XCUI coordinate-anchored tap dispatches
        // without firing Button onTap closures for non-modal Buttons —
        // observed v5.12 c1 ground-truth: big29 visible after swipe-scroll
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
            // v5.12 c1 used a fixed `Thread.sleep(1.2)` to wait for
            // deceleration before snapshotting. v5.14 c1 — replace with a
            // responsive snapshot-frame stability poll: snapshot midX every
            // 100ms, declare settled after 2 consecutive snapshots with
            // |Δ midX| < 0.5 (sub-pixel). 2.0s upper bound (slightly above
            // the v5.12 c1 1.2s observed worst-case to absorb slow scrolls).
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
      // v5.19 c1a — POST /find-text-by-ocr handler. Apple Vision OCR over the
      // current XCUIScreen screenshot. Returns the first matching observation's
      // bounding box normalized to [0,1] in UIKit coord space (top-left
      // origin, y-down). Vision's native bbox is bottom-left origin + y-up,
      // so handler converts to UIKit. L5 sense layer for a11y-i18n initiative
      // (per docs/plan-cold/v5.17-v5.22-a11y-i18n-master.md).
      //
      // Matching: case-insensitive substring on `topCandidates(1)` of each
      // observation. Returns the first hit (top-left-most by Vision
      // observation order). Caller can re-call with finer keyword for
      // disambiguation.
      findTextByOcrHandler: { text, locales, recognitionLevel in
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
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
      // v5.2 c1 — POST /swipe-at-norm-coord handler. From-to coordinate swipe
      // via Apple native event chain (XCSynthesizedEventRecord +
      // XCPointerEventPath initForTouchAtPoint → moveToPoint:atOffset:
      // → liftUpAtOffset:). §9 #3 partial-lift escape hatch sibling of
      // tap-at-norm-coord. Same daemonProxy dispatch path; same setup-time
      // app.frame coord conversion as tap.
      swipeAtCoordHandler: { fromNx, fromNy, toNx, toNy in
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
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
      // v5.2 c3 — POST /double-tap handler. XCUIElement.doubleTap() public
      // API. selector 解析路径同 tap handler (NSPredicate label|identifier),
      // 不接 see-through (modal scope) — see-through 是 tap-specific 路径,
      // c3 doubleTap 走 single-arm:NSPredicate descendants match → first hit.
      // 找不到 stderr + false (notFound).
      doubleTapHandler: { selectorText in
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
        return smixGuarded("double-tap") { () -> Bool in
          let predicate = NSPredicate(
            format: "label == %@ OR identifier == %@",
            selectorText, selectorText
          )
          let element = app.descendants(matching: .any)
            .matching(predicate)
            .firstMatch
          if !element.exists {
            FileHandle.standardError.write(
              Data("smix-runner: double-tap: element not found for selector=\(selectorText)\n".utf8))
            return false
          }
          element.doubleTap()
          return true
        } ?? false
      },
      // v5.2 c3 — POST /long-press handler. XCUIElement.press(forDuration:)
      // public API, duration 从 ms 转 seconds (TimeInterval).
      longPressHandler: { selectorText, durationMs in
        let app = await resolveApp()  // v0.2.1 — per-request target-app rebind.
        return smixGuarded("long-press") { () -> Bool in
          let predicate = NSPredicate(
            format: "label == %@ OR identifier == %@",
            selectorText, selectorText
          )
          let element = app.descendants(matching: .any)
            .matching(predicate)
            .firstMatch
          if !element.exists {
            FileHandle.standardError.write(
              Data("smix-runner: long-press: element not found for selector=\(selectorText)\n".utf8))
            return false
          }
          element.press(forDuration: TimeInterval(durationMs) / 1000.0)
          return true
        } ?? false
      },
      // v5.2 c5 — POST /set-orientation handler. XCUIDevice.shared.orientation
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
          let entry = SessionEntry(
            bundleId: req.bundleId,
            app: target,
            lastActivatedAt: Date()
          )
          sessions.lock()
          sessionTable[sid] = entry
          sessions.unlock()
          return SmixRunnerServer.SessionOpenOutcome(
            sessionId: sid,
            activatedOnce: activatedOnce
          )
        },
        close: { req in
          sessions.lock()
          sessionTable.removeValue(forKey: req.sessionId)
          sessions.unlock()
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
        // v1.0.4 §D5 — close every open session in the table. Used
        // by `smix runner cycle` and the supervisor auto-restart.
        // Idempotent; returns the count that was cleared.
        closeAll: {
          sessions.lock()
          let count = sessionTable.count
          sessionTable.removeAll()
          sessions.unlock()
          return SmixRunnerServer.SessionCloseAllOutcome(closed: count)
        },
        // v1.0.4 §D14 — terminate+launch the session's cached app in
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
          return SmixRunnerServer.SessionRelaunchOutcome(
            notFound: false, ok: true, wallMs: wallMs
          )
        }
      )
    )
  }
}

/// C-fix — 从 XCUIElementSnapshot.dictionaryRepresentation 递归内存遍历, 收集
/// popup container 子树里所有 button (label / identifier) 与 staticText (label)。
/// 取代旧 per-element live .label / .exists / .identifier 访问 (modal alert 弹出
/// 状态下各 ~1.2s, N 个元素累计 >15s 撞 FlyingFox 15s socket timeout 致 runner
/// 主线程 hang)。单次 container.snapshot() 后纯内存遍历, 耗时与元素数 N 无关。
/// 注意 dictionaryRepresentation **不暴露** userTestingAttributes (本 session
/// 实测 utaPresent=false; dict 仅 displayID / elementType / enabled / frame /
/// identifier / label / title / hasFocus / selected / sizeClass / windowContextID),
/// 故 role 分类 (cancel / destructive) 不在此收集 — 由 collectSystemPopups 仅对
/// SpringBoard native alert 走 live predicate query 补 (见该处注释)。
/// XCUIElement.ElementType raw: button=9, staticText=48 (跟 collectSystemPopups
/// inline switch 的 alert=7/sheet=5/dialog=8/popover=18 同一 enum 编号空间)。
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

/// v5.1 c1 — iOS 26.5 sim `XCUIElementSnapshot.dictionaryRepresentation`
/// filter 掉 `hasKeyboardFocus` key(Apple 自 iOS 15 起的 regression /
/// deprecation,maestro #2842 OPEN 也撞同墙)。底层 `XCElementSnapshot._hasKeyboardFocus`
/// ivar 仍承载真值;Foundation KVC `value(forKey: "hasKeyboardFocus")`
/// 透过 filter 直读 ivar。深度优先遍历 snapshot 子树,命中即返该节点
/// identifier。
///
/// 私有符号合规(`.claude/CLAUDE.md` §9 #6):纯 Foundation KVC selector
/// chain — 不硬链接私有符号、不 dlsym 私有常量、不调任何 `_XCT_*` 私有
/// selector;只依赖 Apple 公开的 `value(forKey:)` 入口 + 私有 ivar 的命名
/// 稳定性(Apple 跨 iOS 17/18/26 均保留同名 `_hasKeyboardFocus`,否则
/// 键盘焦点功能整片崩)。
///
/// 实证(2026-06-14):fill input-email → focused identifier = input-email;
/// fill input-password → focused identifier = input-password;`_focused_`
/// 不重 tap → 仍真读 first responder(证非 echo)。
enum FocusedIdentifier {
  /// Return the identifier of the first descendant of `snap` whose
  /// `_hasKeyboardFocus` ivar is true, or nil if none focused (典型:
  /// 键盘未弹起 / 焦点在 non-typable 元素 / fixture 刚 cold launch)。
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
/// v1.5 c5i-a S5 — 走 maestro `cli-2.2.0` 同源 `dictionaryRepresentation` 路径
/// (`AXElement(_ dict: [XCUIElement.AttributeName: Any])` 解 a11y server raw
/// attribute dict). 之前 smix 走 `s.children` 公开 Swift API 是 Apple curated
/// subset, RN drawer 这种 accessibility-rendered 但 Swift API filter 掉的子
/// 元素 (实测 "Dashboard" drawer item) 在 `.children` 数组里消失. raw dict
/// `children` key 含**所有** a11y server 子元素, 跟 maestro 实测 hierarchy
/// 一致.
///
/// `rootIdentifierOverride` is applied only at the top level call (children
/// recurse with nil) and only when the snapshot's own identifier is empty.
/// This compensates for `XCUIApplication.snapshot()` returning a root with
/// an empty identifier even though the caller knows the bundle id.
///
/// v5.1 c1 — `focusHint` 是 live first-responder 的 identifier(snapshotHandler
/// 一次性走 `FocusedIdentifier.find` 算好)。沿子树下传,匹配的 POCO 节点
/// `hasFocus=true`。`nil` ⇒ 无节点焦点(全树 false,典型 = 键盘未弹起)。
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

/// v1.5 c5i-a S5 — Apple a11y server raw dict → A11ySnapshotData. dict 由
/// `XCUIElementSnapshot.dictionaryRepresentation` 提供, key 类型
/// `XCUIElement.AttributeName` 是公开 Swift type. 跟 maestro
/// `AXElement.swift:18-58` 同源字段提取.
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
  // v5.1 c1 — match the live first-responder identifier (one-shot KVC walk
  // done in snapshotHandler) against this node's identifier. `focusHint
  // == nil` ⇒ no node focused (typical 键盘未弹起);non-empty identifier
  // 必须匹配才 tag(避免 hint=nil 时空字符串误匹配空 identifier 节点)。
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
/// 根因 (v1.md §7 2026-05-19, code-level): `app.snapshot()` 以单一 app 元素
/// 为 root, 任何 opaque native modal 会让 iOS accessibility 把覆盖层下的
/// 内容从该 snapshot mask 掉——但 XCUITest 自己的低层扁平枚举仍可达全部
/// 元素 (实测某场景: `app.snapshot()` 返 96 节点的覆盖层而 `label ==`
/// 直查仍能命中底下 2550 元素).
///
/// 策略 (defense in depth, 不信任任何单点 masking):
///   1. Per-window: snapshot 每个 `app.windows` 元素. 一个 modal 经常自
///      占一个 window, sibling window 仍暴露底层内容.
///   2. Flat fallback: `app.descendants(.any)
///      .allElementsBoundByAccessibilityElement`——XCUITest 能枚举的每个
///      元素逐个 snapshot. 这是触达 既-不在 app snapshot 也-不在 per-window
///      snapshot 中的 masked 内容的唯一层.
/// 两路合并为 ONE synthetic application root 的子节点, 经 UNCHANGED
/// `convertSnapshot` / `TreeRoute.serialize` 产出, 序列化形态零变化 (契约
/// 守). synthetic root 带 `bundleId` 让 host smoke `.identifier == bundle`
/// 仍过. 故意不去重: 超集树是目标; SDK-side resolver 自己 DFS-collect,
/// driver 只问"content marker 是否可达"——对超集 sound.
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
  // (any opaque modal 的 underlying AX-reachable content) is captured as
  // leaf nodes even when no window snapshot surfaced it.
  //
  // v1.4 ③-C3 v2 perf cap (经实测 RN expo-dev-menu 50+ native 元素列举
  // 每元素 ~1-2s `el.snapshot()` 致总耗 60-120s 卡 SDK drainPopups hardCap
  // / FlyingFox 15s socket timeout; 加结构性限: maxElements=80 + budgetMs=8s.
  // 任一限到即跳出, 超集树仍含 window snapshot + plain app snapshot 主路径
  // (line 988 + 1022) 保证 content marker 可达. 限内 80 元素足够 RN dev-mode
  // 主屏 / SpringBoard 系统 alert / 一般 modal, 仅 dev-menu/重 native overlay
  // 撞限——其在 line 988 windows snapshot 已捕到, 跳过 flat fallback 不丢
  // marker 可达性, 仅丢叶子节点冗余 (按行为合同"超集树"语义仍成立).
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

  // v1.4 ③-C1 (third restart) S3.a — SpringBoard alert / sheet / dialog
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
    identifier: bundleId,
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

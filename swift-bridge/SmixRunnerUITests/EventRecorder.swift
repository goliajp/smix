import Darwin
import Foundation
import ObjectiveC.runtime
import SmixRunnerCore

// AX-event capture via a swizzle of `XCAXClient_iOS.
// handleAccessibilityNotification:fromElement:payload:`. Same pattern as
// `SmixA11ySwizzle.m`'s `+load` swizzle: method_setImplementation (not
// exchange), the original IMP stashed in a C function pointer, and the
// swizzled IMP forwarding to Apple's original handler through that pointer
// (direct IMP call via unsafeBitCast, no msgSend) so XCAXClient internals
// continue to run unimpaired. The recorder is purely additive — when the
// SMIX_RECORD_ENABLED env var is not "1", `installSwizzle()` is never
// called and the swizzle is never installed, leaving every non-recording
// code path untouched.
//
// Selector note: maestro cli-2.2.0's copied `XCAXClient_iOS.h` lists
// `handleAccessibilityNotification:withPayload:` (2-arg). That selector
// does NOT exist on iOS 26.5's live `XCAXClient_iOS` class
// (`class_getInstanceMethod` returns nil). The class DOES carry
// `handleAccessibilityNotification:fromElement:payload:` (3-arg: int
// code + id element + id payload) and the cleaner block-API
// `addObserverForAXNotification:handler:`. The maestro header dump appears
// to be from an older iOS SDK; the swizzle target here is the real name on
// the live iOS 26.5 class, found via a runtime `class_copyMethodList` dump.
//
// Two C functions exported by the XCTAutomationSupport framework binary
// (they are absent from maestro's copied headers, so read the binary's
// symbol table rather than those headers):
//   T _XCGetWrappedAXUIElementFromNotificationPayload
//   T _XCTStringFromAXNotification
// Both are reached via dlsym against RTLD_DEFAULT (the `-2` sentinel, same
// pattern as SmixIndigoHID/IOHIDDigitizerTap.swift) rather than being
// hard-linked. `XCTStringFromAXNotification(int code)` returns the named
// Apple constant ("kAXHIDEventReceivedNotification" /
// "kAXFirstResponderChangedNotification" / "kAXUserTestingNotification"
// etc) used for kind classification.
// `XCGetWrappedAXUIElementFromNotificationPayload(id payload)` wraps the
// binary `_NSInlineData` payload into an `XCAccessibilityElement` instance
// for introspection of identifier / label / placeholderValue / value /
// frame / elementType.

@objc(SmixEventRecorderStandin)
final class SmixEventRecorderStandin: NSObject {
  // Direct IMP lookup: `class_getMethodImplementation(Standin.self,
  // swizzledSel)` pulls the static IMP — the same trick AXClientStandin uses
  // to dodge msgSend re-validation when the receiver class is XCAXClient_iOS
  // (which has never had `swizzledHandle:fromElement:payload:` registered).
  //
  // Signature taken from the live iOS 26.5 runtime via
  // `class_copyMethodList`: `handleAccessibilityNotification:fromElement:
  // payload:` is 3-arg `(int code, id element, id payload)` → Void.
  @objc func swizzledHandle(_ code: Int, fromElement element: Any?, payload: Any?) {
    EventRecorder.shared.captureFromSwizzle(code: code, element: element, payload: payload)
    // Forward to Apple's original implementation via the C function pointer
    // saved at install time. Mirrors SmixA11ySwizzle.m's direct-IMP,
    // zero-msgSend forwarding pattern.
    if let imp = EventRecorder.shared.originalHandleIMP {
      typealias HandleIMP = @convention(c) (NSObject, Selector, Int, Any?, Any?) -> Void
      let fn = unsafeBitCast(imp, to: HandleIMP.self)
      let selector = NSSelectorFromString("handleAccessibilityNotification:fromElement:payload:")
      // `self` here is the XCAXClient_iOS instance the IMP was originally
      // bound to (swizzle installed on its class method table) — Apple's
      // handler reads `self` as XCAXClient and runs its internal dispatch
      // (e.g., re-broadcast to subscribers, internal state machine).
      fn(self, selector, code, element, payload)
    }
  }
}

final class EventRecorder: @unchecked Sendable {
  static let shared = EventRecorder()

  // Captured at install time so the swizzled IMP can forward to Apple's
  // original handler. Stored as `IMP` (= function pointer); unsafeBitCast
  // is done at each call site to keep the typealias local.
  fileprivate var originalHandleIMP: IMP?

  // Whether the recorder is actively accumulating events. start() flips on,
  // stop() flips off. The swizzle stays installed across start/stop cycles
  // (install is idempotent); inactive recorder discards events but still
  // forwards to Apple's handler.
  private var active: Bool = false
  private var events: [RecordedEvent] = []
  private var appBundleIdHint: String?
  private let lock = NSLock()
  private static let installLock = NSLock()
  private static var installed = false

  // Force the Standin into the ObjC runtime table BEFORE installSwizzle
  // tries `class_getMethodImplementation(Standin.self, ...)`. Same eager
  // realization trick AXClientSwizzler uses.
  fileprivate static let standin: SmixEventRecorderStandin = SmixEventRecorderStandin()

  func installSwizzle(appBundleId: String? = nil) {
    Self.installLock.lock()
    defer { Self.installLock.unlock() }
    if let appBundleId {
      lock.lock(); appBundleIdHint = appBundleId; lock.unlock()
    }
    if Self.installed { return }
    _ = Self.standin
    guard let target = NSClassFromString("XCAXClient_iOS") else {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: XCAXClient_iOS not found, skipping\n".utf8))
      return
    }
    let origSel = NSSelectorFromString("handleAccessibilityNotification:fromElement:payload:")
    guard let origMethod = class_getInstanceMethod(target, origSel) else {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: handleAccessibilityNotification:fromElement:payload: not found, dumping XCAXClient_iOS methods\n".utf8))
      Self.dumpInstanceMethods(of: target, label: "XCAXClient_iOS")
      return
    }
    let replaceSel = #selector(SmixEventRecorderStandin.swizzledHandle(_:fromElement:payload:))
    guard let replaceIMP = class_getMethodImplementation(
      SmixEventRecorderStandin.self, replaceSel
    ) else {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: swizzled IMP lookup failed\n".utf8))
      return
    }
    originalHandleIMP = method_setImplementation(origMethod, replaceIMP)
    Self.installed = true
    FileHandle.standardError.write(
      Data("smix-runner: EventRecorder: handleAccessibilityNotification swizzle installed\n".utf8))
    Self.sweepNotificationNames()
    Self.registerForUIEventNotifications(target: target)
  }

  // Sweep code 1..9999 through XCTStringFromAXNotification to dump every
  // named notification known to XCTAutomationSupport. Helps
  // identify the HIDEvent / element-bearing notification codes Apple
  // exposes (not just the ones the framework happens to fire on startup).
  // One-shot; runs once per installSwizzle().
  private static func sweepNotificationNames() {
    var lines: [String] = []
    for code in 1...9999 {
      if let name = decodeNotificationName(code: Int32(code)) {
        // decodeNotificationName already calls dumpCodeNameOnce internally
        // for codes seen at runtime, but sweep covers codes never fired.
        // Filter out the `"AXNotification <N>"` fallback (= unnamed).
        if !name.hasPrefix("AXNotification ") {
          lines.append("\(code) → \(name)")
        }
      }
    }
    FileHandle.standardError.write(
      Data("smix-runner: EventRecorder: sweep found \(lines.count) named AX notifications\n".utf8))
    for line in lines {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: sweep  \(line)\n".utf8))
    }
  }

  // Actively subscribe to UI-element-bearing notifications by calling
  // `_registerForAXNotification:error:` for every named code the
  // sweep finds. Apple's XCAXClient_iOS only auto-registers framework
  // lifecycle notifications (PidStatusChanged / AppAccessibilityReady /
  // UserTesting); HIDEvent / FirstResponderChanged / element-attribute
  // notifications require explicit registration to be delivered to
  // `handleAccessibilityNotification:fromElement:payload:`.
  private static func registerForUIEventNotifications(target: AnyClass) {
    // XCAXClient_iOS has no class methods (meta-class dump confirmed) and
    // no obvious singleton accessor. maestro grabs the instance via
    // `XCUIDevice.sharedDevice.accessibilityInterface` (AXClientProxy.m).
    // We mirror that path: XCUIDevice is a public XCUITest class; the
    // `accessibilityInterface` property is private but accessible via
    // KVC / perform.
    guard let uiDeviceClass = NSClassFromString("XCUIDevice") else {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: XCUIDevice not found, cannot register\n".utf8))
      return
    }
    let sharedDeviceSel = NSSelectorFromString("sharedDevice")
    guard let sharedDeviceMethod = class_getClassMethod(uiDeviceClass, sharedDeviceSel) else {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: XCUIDevice +sharedDevice class method not found\n".utf8))
      return
    }
    let sharedImp = method_getImplementation(sharedDeviceMethod)
    typealias SharedFn = @convention(c) (AnyClass, Selector) -> NSObject
    let sharedFn = unsafeBitCast(sharedImp, to: SharedFn.self)
    let sharedDevice = sharedFn(uiDeviceClass, sharedDeviceSel)

    // Get the `accessibilityInterface` instance property → XCAXClient_iOS.
    let axInterfaceSel = NSSelectorFromString("accessibilityInterface")
    guard sharedDevice.responds(to: axInterfaceSel),
          let unmanaged = sharedDevice.perform(axInterfaceSel) else {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: XCUIDevice.sharedDevice.accessibilityInterface not accessible\n".utf8))
      return
    }
    guard let clientInstance = unmanaged.takeUnretainedValue() as? NSObject else {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: accessibilityInterface returned unexpected type\n".utf8))
      return
    }
    FileHandle.standardError.write(
      Data("smix-runner: EventRecorder: got XCAXClient_iOS via XCUIDevice.sharedDevice.accessibilityInterface → \(NSStringFromClass(type(of: clientInstance)))\n".utf8))

    // Register: `BOOL _registerForAXNotification:(int)code error:(NSError**)`
    let regSel = NSSelectorFromString("_registerForAXNotification:error:")
    guard let regMethod = class_getInstanceMethod(target, regSel) else {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: _registerForAXNotification:error: not found\n".utf8))
      return
    }
    let regImp = method_getImplementation(regMethod)
    typealias RegFn = @convention(c) (NSObject, Selector, Int32, UnsafeMutablePointer<NSError?>?) -> Bool
    let regFn = unsafeBitCast(regImp, to: RegFn.self)

    var registered = 0
    var failed = 0
    for code in 1...9999 {
      guard let name = decodeNotificationName(code: Int32(code)),
            !name.hasPrefix("AXNotification ") else { continue }
      var err: NSError?
      let ok = withUnsafeMutablePointer(to: &err) { errPtr in
        regFn(clientInstance, regSel, Int32(code), errPtr)
      }
      if ok {
        registered += 1
        FileHandle.standardError.write(
          Data("smix-runner: EventRecorder: registered \(name) (code=\(code))\n".utf8))
      } else {
        failed += 1
        if failed <= 5 {
          FileHandle.standardError.write(
            Data("smix-runner: EventRecorder: register \(name) (code=\(code)) failed: \(String(describing: err))\n".utf8))
        }
      }
    }
    FileHandle.standardError.write(
      Data("smix-runner: EventRecorder: registered \(registered) AX notifications (failed=\(failed))\n".utf8))
  }

  // Class method list dump (sweep instance methods only finds
  // `-` methods, not `+`). Apple's singleton accessors are usually class
  // methods (`+sharedFoo`); they live on the meta-class.
  static func dumpClassMethods(of cls: AnyClass, label: String) {
    var count: UInt32 = 0
    guard let methodList = class_copyMethodList(cls, &count) else {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: \(label) has no class methods\n".utf8))
      return
    }
    defer { free(methodList) }
    FileHandle.standardError.write(
      Data("smix-runner: EventRecorder: \(label) class methods (count=\(count)):\n".utf8))
    let buf = UnsafeBufferPointer(start: methodList, count: Int(count))
    var lines: [String] = []
    for i in 0..<Int(count) {
      let sel = method_getName(buf[i])
      lines.append(NSStringFromSelector(sel))
    }
    lines.sort()
    for name in lines {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder:   \(label) :: +\(name)\n".utf8))
    }
  }

  // Flip the active flag + reset the buffer. The swizzle stays installed
  // (cheap to leave on; only `active` gates the append).
  func start() {
    lock.lock()
    active = true
    events = []
    lock.unlock()
  }

  // Flip inactive + return the accumulated events. The caller
  // (RecordHandlers' stop closure) emits them over the wire.
  func stop() -> [RecordedEvent] {
    lock.lock()
    active = false
    let out = events
    events = []
    lock.unlock()
    return out
  }

  // Snapshot current events, clear, leave `active` unchanged. Used by
  // the host poller (GET /record/poll) for streaming reads during a
  // long record session without losing in-flight events between polls.
  func drain() -> [RecordedEvent] {
    lock.lock()
    let out = events
    events = []
    lock.unlock()
    return out
  }

  fileprivate func captureFromSwizzle(code: Int, element: Any?, payload: Any?) {
    lock.lock()
    let recording = active
    let bundleId = appBundleIdHint
    lock.unlock()
    if !recording { return }
    let event = Self.makeEvent(code: code, element: element, payload: payload, appBundleId: bundleId)
    lock.lock()
    events.append(event)
    lock.unlock()
  }

  // Build a RecordedEvent with kind / selectorHints / frame / elementType /
  // location all best-effort filled via:
  //   1. `XCTStringFromAXNotification(code)` → notif name → `kind` classify
  //   2. `XCGetWrappedAXUIElementFromNotificationPayload(payload)` →
  //      XCAccessibilityElement wrapper (binary `_NSInlineData` decoded)
  //   3. Introspection on XCAccessibilityElement (or the directly-passed
  //      `element` param) for the 5 selector hint keys + frame + elementType
  // All steps fail-safe: dlsym miss / selector miss → the field stays nil,
  // the wire shape is preserved, and the host IR layer's fallback path
  // handles partially-filled events.
  private static func makeEvent(code: Int, element: Any?, payload: Any?, appBundleId: String?) -> RecordedEvent {
    let ts = Int64(Date().timeIntervalSince1970 * 1000)
    var className: String?
    var description: String?

    // Decode notification name + classify kind. The decoded name is also
    // attached to payloadDescription for host-side visibility / debug.
    let notifName = decodeNotificationName(code: Int32(code))
    let kind = classifyKind(notificationName: notifName)

    // Resolve introspect target: prefer the directly-passed `element`
    // (3-arg swizzle gives it as XCAccessibilityElement); fall back to
    // wrapping the binary payload via `XCGet...FromNotificationPayload`.
    var introspectTarget: AnyObject?
    if let element = element as AnyObject? {
      introspectTarget = element
    } else if let payload = payload as AnyObject? {
      introspectTarget = wrapPayloadAsElement(payload: payload)
    }

    // The payload is an `_NSInlineData` (a private NSData subclass) whose
    // bytes start with `0x62706c69 73743030` = ASCII "bplist00": Apple
    // serializes the attribute dict as a binary plist. That means it can be
    // decoded with `NSPropertyListSerialization` directly — no CF API and no
    // async round trip to XCAXClient is required.
    let payloadDict: [String: Any]? = decodePayloadPlist(payload: payload, code: code)

    // Hints / frame / elementType extraction:
    //  1. Prefer payloadDict (plist-decoded attribute dict — Apple's
    //     own internal representation).
    //  2. Fall back to ObjC method invocation on the wrapped
    //     XCAccessibilityElement (frame works via `responds(to:)` + a direct
    //     IMP call; identifier / label etc miss the selector probe).
    let hints = payloadDict.flatMap(extractHintsFromDict)
      ?? introspectTarget.flatMap(extractHintsFromElement)
    let frame = payloadDict.flatMap(extractFrameFromDict)
      ?? introspectTarget.flatMap(extractFrameFromElement)
    let elementType = payloadDict.flatMap(extractElementTypeFromDict)
      ?? introspectTarget.flatMap(extractElementTypeFromElement)
    let location: Point? = {
      // For tap-like events, record the element's center as the action
      // location. The host may use it as a coordinate fallback if selector
      // resolution misses.
      guard kind == "tap", let f = frame else { return nil }
      return Point(x: f.x + f.width / 2.0, y: f.y + f.height / 2.0)
    }()

    if let payload = payload as AnyObject? {
      let cls = type(of: payload)
      className = NSStringFromClass(cls)
    } else if let element = element as AnyObject? {
      let cls = type(of: element)
      className = NSStringFromClass(cls)
    }

    // Truncated debug description — payloadClassName + notif name + raw
    // payload `description`. Useful for the host IR layer's fallback path
    // when hints / frame all came back nil.
    var descParts: [String] = []
    if let notifName { descParts.append("notif=\(notifName)") }
    if let payload = payload as AnyObject? {
      let raw = String(describing: payload)
      descParts.append("payload=\(raw.count > 256 ? String(raw.prefix(256)) + "…" : raw)")
    }
    if let element = element as AnyObject? {
      let raw = String(describing: element)
      descParts.append("element=\(raw.count > 256 ? String(raw.prefix(256)) + "…" : raw)")
    }
    if !descParts.isEmpty {
      description = descParts.joined(separator: " | ")
    }

    return RecordedEvent(
      timestampMs: ts,
      rawCode: code,
      kind: kind,
      location: location,
      endLocation: nil,
      durationMs: nil,
      text: nil,
      selectorHints: hints,
      elementType: elementType,
      frame: frame,
      appBundleId: appBundleId,
      payloadClassName: className,
      payloadDescription: description
    )
  }

  // MARK: - dlsym shims for XCTAutomationSupport C functions

  // RTLD_DEFAULT sentinel — same pattern as SmixIndigoHID/IOHIDDigitizerTap
  // (`UnsafeMutableRawPointer(bitPattern: -2)!`). XCTAutomationSupport is
  // already loaded in the runner process (XCUITest framework dependency),
  // so dlsym via the global sentinel finds the symbols without an explicit
  // dlopen.
  private static let rtldDefault: UnsafeMutableRawPointer = UnsafeMutableRawPointer(bitPattern: -2)!

  // Lazy one-time dlsym lookup. nil = symbol not present (older Xcode /
  // future SDK rename) → makeEvent skips kind / element wrap gracefully.
  //
  // Mach-O ABI artifact: C function symbols in `nm` output carry a leading
  // underscore (e.g. `_XCTStringFromAXNotification`), but `dlsym(...)`
  // expects the API name WITHOUT the underscore — dyld strips it internally
  // before lookup. Passing the underscore-prefixed name returns NULL
  // ("symbol not found"), which silently degrades kind classification to nil
  // for every event.
  private static let stringFromAXNotificationPtr: UnsafeMutableRawPointer? = {
    let p = dlsym(rtldDefault, "XCTStringFromAXNotification")
    if p == nil {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: dlsym XCTStringFromAXNotification miss\n".utf8))
    }
    return p
  }()

  private static let wrappedAXElementFromPayloadPtr: UnsafeMutableRawPointer? = {
    let p = dlsym(rtldDefault, "XCGetWrappedAXUIElementFromNotificationPayload")
    if p == nil {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: dlsym XCGetWrappedAXUIElementFromNotificationPayload miss\n".utf8))
    }
    return p
  }()

  private static func decodeNotificationName(code: Int32) -> String? {
    guard let ptr = stringFromAXNotificationPtr else { return nil }
    typealias Fn = @convention(c) (Int32) -> Unmanaged<NSString>?
    let fn = unsafeBitCast(ptr, to: Fn.self)
    guard let unmanaged = fn(code) else { return nil }
    let name = unmanaged.takeUnretainedValue() as String
    // Log unique code→name pairs so we can see which notifications Apple's
    // runtime is firing. Single dump per code.
    dumpCodeNameOnce(code: Int(code), name: name)
    return name
  }

  private static var codeNameDumped: Set<Int> = []
  private static let codeNameDumpLock = NSLock()
  private static func dumpCodeNameOnce(code: Int, name: String) {
    codeNameDumpLock.lock()
    let already = codeNameDumped.contains(code)
    codeNameDumped.insert(code)
    codeNameDumpLock.unlock()
    if already { return }
    FileHandle.standardError.write(
      Data("smix-runner: EventRecorder: code=\(code) → name=\(name)\n".utf8))
  }

  private static func wrapPayloadAsElement(payload: AnyObject) -> AnyObject? {
    guard let ptr = wrappedAXElementFromPayloadPtr else { return nil }
    typealias Fn = @convention(c) (AnyObject) -> Unmanaged<AnyObject>?
    let fn = unsafeBitCast(ptr, to: Fn.self)
    guard let unmanaged = fn(payload) else { return nil }
    return unmanaged.takeUnretainedValue()
  }

  // MARK: - kind classification

  // Apple notification name → smix event kind. The known names are read off
  // the live runtime via the stderr dump above; unknown names map to nil so
  // the host IR's fallback (payloadDescription-based selector) kicks in.
  private static func classifyKind(notificationName name: String?) -> String? {
    guard let name else { return nil }
    let lower = name.lowercased()
    if lower.contains("hidevent") { return "tap" }
    if lower.contains("firstresponder") { return "focus" }
    if lower.contains("usertesting") { return "snapshot" }
    if lower.contains("orientation") { return "orientation" }
    return nil
  }

  // MARK: - KVC introspect on XCAccessibilityElement

  // 5-field OR-match. XCAccessibilityElement is NOT KVC-compliant —
  // `valueForKey:identifier` throws an NSUnknownKey exception on the live
  // class. Instead, probe via `responds(to:)` + `perform(_:)` per selector:
  // Apple's private ObjC class only exposes specific selectors, not generic
  // KVC. The first-call dump (see `dumpElementMethodsOnce`) lists every
  // instance method so the candidate selector names below can be adjusted
  // against whatever the running OS actually exposes. The probe tries each
  // candidate via `responds(to:)` and returns the first hit.
  private static func extractHintsFromElement(_ element: AnyObject) -> SelectorHints? {
    dumpElementMethodsOnce(element)
    let id = stringValueViaSelector(element,
      ["identifier", "accessibilityIdentifier", "_axIdentifier", "axIdentifier"])
    let lbl = stringValueViaSelector(element,
      ["label", "accessibilityLabel", "_axLabel", "axLabel"])
    let title = stringValueViaSelector(element,
      ["title", "accessibilityTitle", "_axTitle"])
    let ph = stringValueViaSelector(element,
      ["placeholderValue", "accessibilityPlaceholderValue", "_axPlaceholderValue"])
    let val = stringValueViaSelector(element,
      ["value", "accessibilityValue", "stringValue", "_axValue"])
    if id == nil && lbl == nil && title == nil && ph == nil && val == nil {
      return nil
    }
    return SelectorHints(
      identifier: id,
      label: lbl,
      title: title,
      placeholderValue: ph,
      valueString: val
    )
  }

  private static func extractFrameFromElement(_ element: AnyObject) -> Rect? {
    guard let ns = element as? NSObject else { return nil }
    for name in ["frame", "axFrame", "accessibilityFrame", "_frame"] {
      let sel = NSSelectorFromString(name)
      guard ns.responds(to: sel) else { continue }
      // CGRect returns require manual decode via NSInvocation or
      // NSMethodSignature — perform(_:) returns Unmanaged<AnyObject>
      // which can't hold a CGRect (struct return). Use a manual ObjC-
      // style typed shim via method_getImplementation + unsafeBitCast.
      guard let method = class_getInstanceMethod(type(of: ns), sel) else { continue }
      let imp = method_getImplementation(method)
      typealias Fn = @convention(c) (NSObject, Selector) -> CGRect
      let fn = unsafeBitCast(imp, to: Fn.self)
      let rect = fn(ns, sel)
      // Sanity: zero-frame events are common (focus-only notif) — return
      // them rather than discarding so the host still sees the field.
      return Rect(
        x: Double(rect.origin.x), y: Double(rect.origin.y),
        width: Double(rect.size.width), height: Double(rect.size.height)
      )
    }
    return nil
  }

  private static func extractElementTypeFromElement(_ element: AnyObject) -> Int? {
    guard let ns = element as? NSObject else { return nil }
    for name in ["elementType", "axElementType", "type"] {
      let sel = NSSelectorFromString(name)
      guard ns.responds(to: sel) else { continue }
      guard let method = class_getInstanceMethod(type(of: ns), sel) else { continue }
      let imp = method_getImplementation(method)
      // elementType is `unsigned long long` (XCUIElementType raw) — match
      // ObjC `unsigned long` (NSUInteger). Use UInt64 to be safe across
      // both 32/64-bit ABI on simulator.
      typealias Fn = @convention(c) (NSObject, Selector) -> UInt64
      let fn = unsafeBitCast(imp, to: Fn.self)
      let raw = fn(ns, sel)
      return Int(raw)
    }
    return nil
  }

  private static func stringValueViaSelector(_ element: AnyObject, _ candidates: [String]) -> String? {
    guard let ns = element as? NSObject else { return nil }
    for name in candidates {
      let sel = NSSelectorFromString(name)
      guard ns.responds(to: sel) else { continue }
      guard let unmanaged = ns.perform(sel) else { continue }
      // perform(_:) returns Unmanaged<AnyObject>; take and cast to NSString.
      let any = unmanaged.takeUnretainedValue()
      if let str = any as? String {
        return str.isEmpty ? nil : str
      }
    }
    return nil
  }

  // MARK: - payload binary plist decode

  // Payload shape, from the outside in:
  //  1. The payload is `_NSInlineData`, a private NSData subclass.
  //  2. Its bytes start with the `bplist00` magic — binary plist format.
  //  3. `PropertyListSerialization.propertyList(...)` parses it, but the
  //     top-level dict is `{$archiver, $objects, $top, $version}` — an
  //     NSKeyedArchiver envelope, NOT a flat attribute dict.
  //  4. NSKeyedUnarchiver walks the $objects table and reconstructs the real
  //     Apple object graph. The root is typically an NSDictionary keyed by
  //     Apple's internal attribute keys (`identifier` / `label` / `value` /
  //     `frame` / `elementType` etc).
  private static func decodePayloadPlist(payload: Any?, code: Int) -> [String: Any]? {
    guard let data = payload as? Data else { return nil }
    // Try NSKeyedUnarchiver first (the observed envelope). Allow any
    // class — the payload may carry XCAccessibilityElement / NSDictionary
    // / NSValue / private Apple classes. `secureCoding` opt-in is off
    // because we don't pre-know the class graph; dev-time tool only.
    if let unarchiver = try? NSKeyedUnarchiver(forReadingFrom: data) {
      unarchiver.requiresSecureCoding = false
      if let root = unarchiver.decodeObject(forKey: NSKeyedArchiveRootObjectKey) {
        let dict = normalizeToStringKeyedDict(root)
        dumpPayloadDictByCode(dict, code: code)
        return dict
      }
    }
    // Fall back to raw plist parse (in case payload is a non-archived dict).
    if let parsed = try? PropertyListSerialization.propertyList(
      from: data, options: [], format: nil
    ) {
      let dict = normalizeToStringKeyedDict(parsed)
      dumpPayloadDictByCode(dict, code: code)
      return dict
    }
    return nil
  }

  private static func normalizeToStringKeyedDict(_ value: Any) -> [String: Any]? {
    if let d = value as? [String: Any] { return d }
    if let d = value as? [AnyHashable: Any] {
      return Dictionary(uniqueKeysWithValues: d.compactMap {
        (k, v) -> (String, Any)? in ("\(k)", v)
      })
    }
    // Some payloads root at an array containing one dict (e.g. tap event
    // delivers `[[String:Any]]`); peel one level if homogeneous.
    if let arr = value as? [Any], let first = arr.first as? [String: Any] {
      return first
    }
    return nil
  }

  // Dump the decoded payload plist dict's keys once per notification code.
  // Different notification codes carry different payload dict shapes, so a
  // single-shot dump would miss the variation.
  private static var payloadDumpedCodes: Set<Int> = []
  private static let payloadDumpLock = NSLock()
  private static func dumpPayloadDictByCode(_ dict: [String: Any]?, code: Int) {
    payloadDumpLock.lock()
    let already = payloadDumpedCodes.contains(code)
    if dict != nil { payloadDumpedCodes.insert(code) }
    payloadDumpLock.unlock()
    if already { return }
    guard let dict else { return }
    let keys = dict.keys.sorted()
    FileHandle.standardError.write(
      Data("smix-runner: EventRecorder: payload dict for code=\(code) (count=\(keys.count)):\n".utf8))
    for k in keys {
      let v = dict[k]
      let preview = String(describing: v ?? "nil").prefix(120)
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder:   [code=\(code)] \(k) :: \(preview)\n".utf8))
    }
  }

  private static func extractHintsFromDict(_ dict: [String: Any]) -> SelectorHints? {
    let id = stringFromDict(dict, candidates: [
      "identifier", "axIdentifier", "_XCElementIdentifier", "XCElementIdentifier",
      "XCAttributeIdentifier", "AXIdentifier",
    ])
    let lbl = stringFromDict(dict, candidates: [
      "label", "axLabel", "XCElementLabel", "AXLabel", "_XCElementLabel",
    ])
    let title = stringFromDict(dict, candidates: [
      "title", "axTitle", "AXTitle",
    ])
    let ph = stringFromDict(dict, candidates: [
      "placeholderValue", "axPlaceholderValue", "AXPlaceholderValue",
    ])
    let val = stringFromDict(dict, candidates: [
      "value", "axValue", "stringValue", "AXValue",
    ])
    if id == nil && lbl == nil && title == nil && ph == nil && val == nil {
      return nil
    }
    return SelectorHints(
      identifier: id, label: lbl, title: title,
      placeholderValue: ph, valueString: val
    )
  }

  private static func extractFrameFromDict(_ dict: [String: Any]) -> Rect? {
    for key in ["frame", "axFrame", "XCElementFrame", "AXFrame"] {
      guard let raw = dict[key] else { continue }
      // Could be NSValue-wrapped CGRect, or a [String:Double] dict, or
      // an array of 4 numbers — try each.
      if let nsValue = raw as? NSValue {
        let r = nsValue.cgRectValue
        return Rect(
          x: Double(r.origin.x), y: Double(r.origin.y),
          width: Double(r.size.width), height: Double(r.size.height)
        )
      }
      if let coords = raw as? [String: Double] {
        return Rect(
          x: coords["X"] ?? coords["x"] ?? 0,
          y: coords["Y"] ?? coords["y"] ?? 0,
          width: coords["Width"] ?? coords["width"] ?? 0,
          height: coords["Height"] ?? coords["height"] ?? 0
        )
      }
      if let arr = raw as? [Double], arr.count == 4 {
        return Rect(x: arr[0], y: arr[1], width: arr[2], height: arr[3])
      }
    }
    return nil
  }

  private static func extractElementTypeFromDict(_ dict: [String: Any]) -> Int? {
    for key in ["elementType", "axElementType", "XCElementType", "AXElementType"] {
      if let raw = dict[key] as? Int {
        return raw
      }
      if let raw = dict[key] as? NSNumber {
        return raw.intValue
      }
    }
    return nil
  }

  private static func stringFromDict(_ dict: [String: Any], candidates: [String]) -> String? {
    for key in candidates {
      if let s = dict[key] as? String, !s.isEmpty {
        return s
      }
    }
    return nil
  }

  // One-time dump of XCAccessibilityElement (or whatever class the wrapped
  // element resolves to) instance methods. Helps narrow the candidate
  // selector list above to what the running OS really exposes.
  private static var elementDumpOnce: Bool = false
  private static let elementDumpLock = NSLock()
  private static func dumpElementMethodsOnce(_ element: AnyObject) {
    elementDumpLock.lock()
    let already = elementDumpOnce
    elementDumpOnce = true
    elementDumpLock.unlock()
    if already { return }
    let cls: AnyClass = type(of: element)
    dumpInstanceMethods(of: cls, label: "Element(\(NSStringFromClass(cls)))")
    if let superCls = class_getSuperclass(cls) {
      dumpInstanceMethods(of: superCls, label: "Element-super(\(NSStringFromClass(superCls)))")
    }
  }

  // When `handleAccessibilityNotification:withPayload:` (as named in
  // maestro's copied XCAXClient_iOS.h) isn't found on the live class, dump
  // every registered instance method's selector + signature to stderr. This
  // is the cheapest path to identifying the actual selector name on the OS
  // we run against versus whatever iOS version that header dump came from.
  static func dumpInstanceMethods(of cls: AnyClass, label: String) {
    var count: UInt32 = 0
    guard let methodList = class_copyMethodList(cls, &count) else {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder: \(label) has no instance methods\n".utf8))
      return
    }
    defer { free(methodList) }
    FileHandle.standardError.write(
      Data("smix-runner: EventRecorder: \(label) instance methods (count=\(count)):\n".utf8))
    let buf = UnsafeBufferPointer(start: methodList, count: Int(count))
    var lines: [String] = []
    for i in 0..<Int(count) {
      let method = buf[i]
      let sel = method_getName(method)
      let name = NSStringFromSelector(sel)
      // Filter to selectors plausibly related to notifications / events /
      // handling, plus everything containing "notif" or "handle" anywhere
      // — keep the dump targeted but not so narrow we miss a renamed
      // form (e.g. `_didReceiveAccessibilityNotification:`).
      let lower = name.lowercased()
      if lower.contains("notif") || lower.contains("handle") ||
         lower.contains("event") || lower.contains("payload") ||
         lower.contains("ax") {
        lines.append(name)
      }
    }
    lines.sort()
    for name in lines {
      FileHandle.standardError.write(
        Data("smix-runner: EventRecorder:   \(label) :: \(name)\n".utf8))
    }
  }

}

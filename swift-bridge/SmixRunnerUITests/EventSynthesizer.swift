// v1.8 c2 — XCSynthesizedEventRecord + XCPointerEventPath + XCTRunnerDaemonSession
// 私有 ObjC class 调用, 合成 raw IOKit-level touch event (无 XCUIElement-owner 元数据),
// 经 UIKit `UIApplication.sendEvent:` 标准 hit-test → RN Pressable 触 onPress.
//
// 跟 maestro `cli-2.2.0`:
//   - maestro-driver-iosUITests/Routes/Handlers/TouchRouteHandler.swift:34-44
//   - maestro-driver-iosUITests/Routes/XCTest/EventRecord.swift
//   - maestro-driver-iosUITests/Routes/XCTest/PointerEventPath.swift
//   - maestro-driver-iosUITests/Routes/XCTest/RunnerDaemonProxy.swift
// 1:1 同源 — smix 合并为单文件 (smix swift-bridge 单 target 不分目录).
//
// CLAUDE.md §9 #6 合规: 全用 `NSClassFromString` / `objc_lookUpClass` +
// `unsafeBitCast(method(for:), to: ...)` 动态加载, 不硬链接私有符号. 跟 smix
// 既有 `DaemonKeyboard.sendString` (SmixRunnerUITests.swift:95-135) 同 pattern.

import Foundation
import ObjectiveC
import UIKit

/// alloc helper — Swift 不允许 `AnyClass.alloc()` 直接调用 (Swift 5.x 后弃用),
/// 用 ObjC runtime class_getClassMethod + method_getImplementation 拿 alloc IMP
/// 再 unsafeBitCast 调.
private func ocAlloc(_ className: String) -> NSObject? {
  guard let cls = NSClassFromString(className) else { return nil }
  let sel = NSSelectorFromString("alloc")
  guard let m = class_getClassMethod(cls, sel) else { return nil }
  let imp = method_getImplementation(m)
  typealias AllocFn = @convention(c) (AnyClass, Selector) -> NSObject
  let fn = unsafeBitCast(imp, to: AllocFn.self)
  return fn(cls, sel)
}

/// Wraps `XCPointerEventPath` private ObjC class. Constructed via
/// `initForTouchAtPoint:offset:`; lifted via `liftUpAtOffset:`; intermediate
/// `moveToPoint:atOffset:` calls compose swipe / drag gestures (v5.2 c1).
final class SmixPointerEventPath {
  let path: NSObject
  var offset: TimeInterval

  static func forTouch(at point: CGPoint, offset: TimeInterval = 0) -> SmixPointerEventPath? {
    guard let alloced = ocAlloc("XCPointerEventPath") else { return nil }
    let selector = NSSelectorFromString("initForTouchAtPoint:offset:")
    let imp = alloced.method(for: selector)
    typealias InitFn = @convention(c) (NSObject, Selector, CGPoint, TimeInterval) -> NSObject
    let initFn = unsafeBitCast(imp, to: InitFn.self)
    let path = initFn(alloced, selector, point, offset)
    return SmixPointerEventPath(path: path, offset: offset)
  }

  /// v5.2 c1 — swipe path: touch-down at `from`, drag to `to` over `duration`,
  /// then `liftUp` at the same offset. The caller adds the path to a
  /// SmixEventRecord and dispatches via daemonProxy.
  static func forSwipe(
    from start: CGPoint, to end: CGPoint, duration: TimeInterval = 0.3
  ) -> SmixPointerEventPath? {
    guard let p = forTouch(at: start, offset: 0) else { return nil }
    p.moveTo(point: end, atOffset: duration)
    p.offset = duration
    return p
  }

  private init(path: NSObject, offset: TimeInterval) {
    self.path = path
    self.offset = offset
  }

  /// v5.2 c1 — add an intermediate path point at `offset`; used to compose
  /// swipe / drag gestures between `forTouch` (touch-down) and `liftUp`.
  func moveTo(point: CGPoint, atOffset offset: TimeInterval) {
    let selector = NSSelectorFromString("moveToPoint:atOffset:")
    let imp = path.method(for: selector)
    typealias Method = @convention(c) (NSObject, Selector, CGPoint, TimeInterval) -> Void
    let method = unsafeBitCast(imp, to: Method.self)
    method(path, selector, point, offset)
  }

  func liftUp() {
    let selector = NSSelectorFromString("liftUpAtOffset:")
    let imp = path.method(for: selector)
    typealias Method = @convention(c) (NSObject, Selector, TimeInterval) -> Void
    let method = unsafeBitCast(imp, to: Method.self)
    method(path, selector, offset)
  }
}

/// Wraps `XCSynthesizedEventRecord` private ObjC class. Records one or more
/// `XCPointerEventPath` instances + dispatches them via daemonProxy.
final class SmixEventRecord {
  let record: NSObject
  static let defaultTapDuration: TimeInterval = 0.1

  init?(orientation: UIInterfaceOrientation) {
    guard let alloced = ocAlloc("XCSynthesizedEventRecord") else { return nil }
    let selector = NSSelectorFromString("initWithName:interfaceOrientation:")
    let imp = alloced.method(for: selector)
    typealias InitFn = @convention(c) (
      NSObject, Selector, NSString, UIInterfaceOrientation
    ) -> NSObject
    let initFn = unsafeBitCast(imp, to: InitFn.self)
    self.record = initFn(
      alloced, selector,
      "Single-Finger Touch Action" as NSString,
      orientation
    )
  }

  /// Build a tap event path at `point` (touch-down + liftUp after
  /// `defaultTapDuration`) and add to this record.
  func addPointerTouchEvent(at point: CGPoint) -> Bool {
    guard let path = SmixPointerEventPath.forTouch(at: point) else { return false }
    path.offset = SmixEventRecord.defaultTapDuration
    path.liftUp()
    let selector = NSSelectorFromString("addPointerEventPath:")
    let imp = record.method(for: selector)
    typealias Method = @convention(c) (NSObject, Selector, NSObject) -> Void
    let method = unsafeBitCast(imp, to: Method.self)
    method(record, selector, path.path)
    return true
  }

  /// v5.2 c1 — build a swipe event path: touch-down at `from`, drag to `to`
  /// over `duration`, then liftUp. Returns false if `XCPointerEventPath`
  /// allocation failed (Apple bumped the private API on this OS).
  /// Default duration mirrors maestro `cli-2.2.0` swipe handler (0.3s).
  static let defaultSwipeDuration: TimeInterval = 0.3
  func addPointerSwipeEvent(
    from start: CGPoint, to end: CGPoint, duration: TimeInterval = 0.3
  ) -> Bool {
    guard let path = SmixPointerEventPath.forSwipe(
      from: start, to: end, duration: duration)
    else { return false }
    path.liftUp()
    let selector = NSSelectorFromString("addPointerEventPath:")
    let imp = record.method(for: selector)
    typealias Method = @convention(c) (NSObject, Selector, NSObject) -> Void
    let method = unsafeBitCast(imp, to: Method.self)
    method(record, selector, path.path)
    return true
  }
}

/// Wraps `XCTRunnerDaemonSession.sharedSession.daemonProxy` for raw event
/// synthesis. Same proxy as `DaemonKeyboard` (which uses `_XCT_sendString:...`);
/// this class uses `_XCT_synthesizeEvent:completion:`.
final class SmixRunnerDaemonProxy: @unchecked Sendable {
  static let shared = SmixRunnerDaemonProxy()
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

  /// Dispatch a synthesized event record via daemonProxy. Awaits the
  /// completion callback. Throws on missing proxy / daemon error.
  func synthesize(record: SmixEventRecord) async throws {
    guard let proxy = self.proxy else {
      throw NSError(
        domain: "SmixRunnerDaemonProxy", code: 1,
        userInfo: [NSLocalizedDescriptionKey:
          "XCTRunnerDaemonSession daemonProxy unavailable"])
    }
    let selector = NSSelectorFromString("_XCT_synthesizeEvent:completion:")
    let imp = proxy.method(for: selector)
    typealias Method = @convention(c) (
      NSObject, Selector, NSObject, @escaping (Error?) -> Void
    ) -> Void
    let method = unsafeBitCast(imp, to: Method.self)
    return try await withCheckedThrowingContinuation { continuation in
      method(proxy, selector, record.record) { error in
        if let error = error {
          continuation.resume(throwing: error)
        } else {
          continuation.resume(returning: ())
        }
      }
    }
  }
}

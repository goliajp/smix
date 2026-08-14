// Calls into the private ObjC classes XCSynthesizedEventRecord,
// XCPointerEventPath and XCTRunnerDaemonSession to synthesize a raw
// IOKit-level touch event (one with no XCUIElement-owner metadata). The
// event goes through UIKit's standard `UIApplication.sendEvent:` hit-test,
// which is what makes a React Native Pressable actually fire onPress.
//
// The approach mirrors maestro `cli-2.2.0`:
//   - maestro-driver-iosUITests/Routes/Handlers/TouchRouteHandler.swift
//   - maestro-driver-iosUITests/Routes/XCTest/EventRecord.swift
//   - maestro-driver-iosUITests/Routes/XCTest/PointerEventPath.swift
//   - maestro-driver-iosUITests/Routes/XCTest/RunnerDaemonProxy.swift
// Those four are merged into this single file because the swift-bridge
// target is flat.
//
// Private-symbol policy: private symbols must be reached dynamically and
// never hard-linked. Everything here goes through `NSClassFromString` /
// `objc_lookUpClass` plus `unsafeBitCast(method(for:), to: ...)`, the same
// pattern as the existing `DaemonKeyboard.sendString`.

import Foundation
import SmixRunnerCore
import ObjectiveC
import UIKit

/// alloc helper. Swift does not allow calling `AnyClass.alloc()` directly
/// (deprecated since Swift 5.x), so fetch the `alloc` IMP through the ObjC
/// runtime with class_getClassMethod + method_getImplementation and call it
/// via unsafeBitCast.
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
/// `moveToPoint:atOffset:` calls compose swipe / drag gestures.
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

  /// Swipe path: touch-down at `from`, drag to `to` over `duration`,
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

  /// Add an intermediate path point at `offset`; used to compose
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

  /// Add `times` touches at one point, spaced by `intervalMs`.
  ///
  /// One record, several paths, one synthesise. The alternative — a
  /// request per tap — costs a ~400 ms round trip each (measured on
  /// iOS 26.5) and leaves the interval as whatever that round trip
  /// happened to be, which is why a flow could not drive a gesture
  /// gated on a 500 ms window: it could not tell a slow harness from a
  /// broken app.
  ///
  /// Here the spacing is a number the caller states, carried on the
  /// event timeline itself.
  func addPointerTapBurst(
    at point: CGPoint, times: Int, intervalMs: Int, holdMs: Int
  ) -> Bool {
    let downs = TouchTimeline.downOffsets(times: times, intervalMs: intervalMs)
    let selector = NSSelectorFromString("addPointerEventPath:")
    for down in downs {
      guard let path = SmixPointerEventPath.forTouch(at: point, offset: down) else {
        return false
      }
      path.offset = TouchTimeline.upOffset(downOffset: down, holdMs: holdMs)
      path.liftUp()
      let imp = record.method(for: selector)
      typealias Method = @convention(c) (NSObject, Selector, NSObject) -> Void
      let method = unsafeBitCast(imp, to: Method.self)
      method(record, selector, path.path)
    }
    return true
  }

  /// Build a swipe event path: touch-down at `from`, drag to `to`
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

/// The interface orientation every synthesised event is stamped with.
///
/// It was written out at six call sites as a literal `.portrait`. That
/// is the value `XCSynthesizedEventRecord` uses to decide how the
/// points in an event path are read, so a landscape app receives
/// touches computed in its own frame and interpreted as though the
/// screen were portrait — which is the defect under investigation in
/// v5.1. Naming it once does not change it; it makes
/// `GET /coordinate-space` able to report the same value the touches
/// actually carry, instead of a comment claiming what they carry.
let smixEventRecordOrientation: UIInterfaceOrientation = .portrait

func describeInterfaceOrientation(_ o: UIInterfaceOrientation) -> String {
  switch o {
  case .portrait: return "portrait"
  case .portraitUpsideDown: return "portraitUpsideDown"
  case .landscapeLeft: return "landscapeLeft"
  case .landscapeRight: return "landscapeRight"
  case .unknown: return "unknown"
  @unknown default: return "unknown"
  }
}

func describeDeviceOrientation(_ o: UIDeviceOrientation) -> String {
  switch o {
  case .portrait: return "portrait"
  case .portraitUpsideDown: return "portraitUpsideDown"
  // Named as the interface would name them, so the two strings this
  // route returns are comparable at all. UIDeviceOrientation's
  // landscapeLeft is the interface's landscapeRight — comparing the
  // raw names would report a mismatch that is only a vocabulary
  // difference, and hide a real one behind it.
  case .landscapeLeft: return "landscapeRight"
  case .landscapeRight: return "landscapeLeft"
  case .faceUp: return "faceUp"
  case .faceDown: return "faceDown"
  case .unknown: return "unknown"
  @unknown default: return "unknown"
  }
}

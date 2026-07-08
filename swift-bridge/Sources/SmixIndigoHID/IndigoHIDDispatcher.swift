import Foundation
import CoreGraphics
#if canImport(ObjectiveC)
import ObjectiveC.runtime
#endif

/// 9-arg `IndigoHIDMessageForMouseNSEvent` ABI (iOS 26 / Xcode 26).
/// Layout matches baguette `IndigoHIDInput.swift` L17-23 verbatim:
///   - 1: CGPoint (location, normalized 0-1 when sizeX=sizeY=1.0)
///   - 2: optional CGPoint (second pointer / hover, nil for single tap)
///   - 3: UInt32 target (0x32 = touch digitizer)
///   - 4: UInt32 nsEventType (1 = down, 2 = up)
///   - 5: UInt32 direction (1 = down, 2 = up)
///   - 6/7/8/9: Double timestamp / sizeX / sizeY / pressure
public typealias IndigoMouseFn = @convention(c) (
  UnsafePointer<CGPoint>, UnsafePointer<CGPoint>?,
  UInt32, UInt32, UInt32,
  Double, Double,
  Double, Double
) -> UnsafeMutableRawPointer?

/// `IndigoHIDMessageToCreatePointerService` / `...ToCreateMouseService`:
/// no-arg warm-up factories.
public typealias IndigoServiceFn = @convention(c) () -> UnsafeMutableRawPointer?

/// `IndigoHIDMessageForButton` — reserved for C4 (home/lock); kept here so
/// `unsafeBitCast` site lives in one file.
public typealias IndigoButtonFn = @convention(c) (UInt32, UInt32, UInt32) -> UnsafeMutableRawPointer?

/// Wire-ABI constants — must NEVER drift; pinned by `IndigoConstantsTests`.
public enum IndigoConstants {
  /// `IndigoHIDTarget.touch`. iOS 26 routes touch events through this id.
  public static let touchDigitizer: UInt32 = 0x32
  /// NSEventType: mouse-down.
  public static let nsEventDown: UInt32 = 1
  /// NSEventType: mouse-up.
  public static let nsEventUp: UInt32 = 2
  /// Direction enum: down.
  public static let dirDown: UInt32 = 1
  /// Direction enum: up.
  public static let dirUp: UInt32 = 2
}

/// Host-side Indigo dispatcher. Caller has already done `dlopen` on
/// SimulatorKit + CoreSimulator and resolved the 6 C symbols; this layer
/// is the ObjC-runtime glue + actual mouseFn invocation.
public enum IndigoHIDDispatcher {
  /// Tap once at normalized coordinates `(x, y)` ∈ [0, 1]^2. Default
  /// hold = 100 ms (down → sleep → up). Blocks for the full hold + a
  /// small post-up settle.
  public static func tapNormalized(
    udid: String,
    x: Double, y: Double,
    symbols: IndigoSymbols,
    developerDir: String,
    coreSimulatorHandle: UnsafeMutableRawPointer
  ) throws {
    let device = try CoreSimulatorBridge.resolveSimDevice(
      udid: udid,
      developerDir: developerDir,
      coreSimulatorHandle: coreSimulatorHandle
    )
    let client = try buildLegacyHIDClient(device: device)
    try warmUp(client: client, symbols: symbols)
    try sendTap(client: client, symbols: symbols, x: x, y: y)
  }

  // MARK: - private

  private static func buildLegacyHIDClient(device: AnyObject) throws -> AnyObject {
    let className = "_TtC12SimulatorKit24SimDeviceLegacyHIDClient"
    guard let cls = NSClassFromString(className) else {
      throw HostHIDError.classLookupFailed(name: className)
    }

    // Use alloc + initWithDevice:error: — baguette-equivalent pattern. Going
    // through `class_getMethodImplementation` + `unsafeBitCast` keeps us free
    // of any `@objc` protocol declaration for the private class.
    typealias AllocIMP = @convention(c) (AnyClass, Selector) -> AnyObject
    let allocSel = NSSelectorFromString("alloc")
    guard let allocImpRaw = class_getMethodImplementation(object_getClass(cls), allocSel) else {
      throw HostHIDError.initFailed(detail: "no +alloc IMP on \(className)")
    }
    let allocFn = unsafeBitCast(allocImpRaw, to: AllocIMP.self)
    let raw = allocFn(cls, allocSel)

    typealias InitIMP = @convention(c) (
      AnyObject, Selector, AnyObject, AutoreleasingUnsafeMutablePointer<NSError?>?
    ) -> AnyObject?
    let initSel = NSSelectorFromString("initWithDevice:error:")
    guard let initImpRaw = class_getMethodImplementation(cls, initSel) else {
      throw HostHIDError.initFailed(detail: "no -initWithDevice:error: IMP on \(className)")
    }
    let initFn = unsafeBitCast(initImpRaw, to: InitIMP.self)
    var errPtr: NSError?
    let client = withUnsafeMutablePointer(to: &errPtr) { ptr -> AnyObject? in
      return initFn(raw, initSel, device, AutoreleasingUnsafeMutablePointer(ptr))
    }
    if let e = errPtr {
      throw HostHIDError.initFailed(detail: "initWithDevice: \(e.localizedDescription)")
    }
    guard let c = client else {
      throw HostHIDError.initFailed(detail: "initWithDevice: returned nil")
    }
    return c
  }

  private static func warmUp(client: AnyObject, symbols: IndigoSymbols) throws {
    let createPointer = unsafeBitCast(symbols.createPointerSvc, to: IndigoServiceFn.self)
    let createMouse = unsafeBitCast(symbols.createMouseSvc, to: IndigoServiceFn.self)

    guard let pointerMsg = createPointer() else {
      throw HostHIDError.sendFailed(stage: "createPointerService returned nil")
    }
    try LegacyHIDClientSender.send(
      message: pointerMsg, to: client, stage: "createPointerService"
    )
    usleep(20_000)

    guard let mouseMsg = createMouse() else {
      throw HostHIDError.sendFailed(stage: "createMouseService returned nil")
    }
    try LegacyHIDClientSender.send(
      message: mouseMsg, to: client, stage: "createMouseService"
    )
    usleep(20_000)
  }

  private static func sendTap(
    client: AnyObject,
    symbols: IndigoSymbols,
    x: Double, y: Double
  ) throws {
    let mouseFn = unsafeBitCast(symbols.mouseFn, to: IndigoMouseFn.self)

    var point = CGPoint(x: x, y: y)

    // Down message.
    let down: UnsafeMutableRawPointer? = withUnsafePointer(to: &point) { p in
      mouseFn(
        p, nil,
        IndigoConstants.touchDigitizer,
        IndigoConstants.nsEventDown,
        IndigoConstants.dirDown,
        1.0, 1.0, 1.0, 1.0
      )
    }
    guard let downMsg = down else {
      throw HostHIDError.sendFailed(stage: "mouseFn(down) returned nil")
    }
    try LegacyHIDClientSender.send(message: downMsg, to: client, stage: "tap-down")

    usleep(100_000)  // 100 ms hold

    let up: UnsafeMutableRawPointer? = withUnsafePointer(to: &point) { p in
      mouseFn(
        p, nil,
        IndigoConstants.touchDigitizer,
        IndigoConstants.nsEventUp,
        IndigoConstants.dirUp,
        1.0, 1.0, 1.0, 1.0
      )
    }
    guard let upMsg = up else {
      throw HostHIDError.sendFailed(stage: "mouseFn(up) returned nil")
    }
    try LegacyHIDClientSender.send(message: upMsg, to: client, stage: "tap-up")

    usleep(20_000)  // brief settle so the call returns after delivery starts
  }
}

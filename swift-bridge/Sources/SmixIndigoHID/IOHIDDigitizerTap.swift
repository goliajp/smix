import Foundation
import CoreGraphics
#if canImport(Darwin)
import Darwin
#endif

/// IOHIDEvent-tree digitizer tap — the default HID dispatch path.
///
/// Mirrors the architecture of baguette's `IOHIDDigitizerDispatch`, but the
/// *function bodies* here are written independently from documented fact
/// tables (byte offsets, constants, typealias shapes, event-construction
/// sequence, phase masks, edge bits). The Apache-2.0 baguette source serves
/// as an architecture reference only.
///
/// Why this path: iOS 26.4 reroutes 9-arg `IndigoHIDMessageForMouseNSEvent`
/// taps to the Home gesture path or drops them; the IOHIDEvent-tree path is
/// the supported route on modern iOS (baguette ships this as production).
public enum IOHIDDigitizerTap {

  // MARK: - Wire-ABI types (pinned by `IOHIDDigitizerTapTests`)

  public enum Edge: Equatable {
    case none, left, top, right, bottom

    /// Edge bitmap byte written into the patched slots at
    /// `kEdgeBitByteOffsetA` (and `kEdgeBitByteOffsetB` when size permits).
    public var bit: UInt8 {
      switch self {
      case .none:   return 0x00
      case .left:   return 0x02
      case .top:    return 0x08
      case .right:  return 0x04
      case .bottom: return 0x01
      }
    }
  }

  public enum Phase: Equatable {
    case down, move, up

    /// IOHIDDigitizerEventMask bits OR-ed: 0x01 Range | 0x02 Touch | 0x04 Position.
    /// down/move = 0x07, up = 0x06 (Range bit drops on up — baguette pin L63-68).
    public var eventMask: UInt32 {
      switch self {
      case .down, .move: return 0x07
      case .up:          return 0x06
      }
    }

    /// `range` = (phase != .up); baguette pin L70.
    public var range: Bool { self != .up }

    /// `touch` = (phase != .up); baguette pin L71.
    public var touch: Bool { self != .up }
  }

  // MARK: - Public entry

  /// Tap once at normalized coordinates `(x, y)` ∈ [0, 1]^2 via the IOHIDEvent
  /// digitizer-tree path.
  ///
  /// Steps:
  ///   1. Resolve `SimDevice` for `udid` via `CoreSimulatorBridge`.
  ///   2. Build `SimDeviceLegacyHIDClient`.
  ///   3. For down then up phases:
  ///        a. Build parent digitizer event + finger child event +
  ///           append child → parent (IOKit C functions).
  ///        b. Wrap with `IndigoHIDMessageForTrackpadEventFromHIDEventRef`
  ///           (SimulatorKit private).
  ///        c. Byte-patch target tag + edge bits at the offsets locked
  ///           in `DigitizerConstants`.
  ///        d. Send via `LegacyHIDClientSender.send(...)` (shared with the
  ///           9-arg mouseFn path).
  ///   4. Sleep for `holdMicros` between down and up.
  ///
  /// `symbols` (IndigoSymbols) is resolved by the caller but not invoked on
  /// this path — the "digitizer-required" and "mouseFn-required" symbol bags
  /// are not split yet.
  public static func tapNormalized(
    udid: String,
    x: Double, y: Double,
    symbols: IndigoSymbols,
    digitizerSymbols: DigitizerSymbols,
    developerDir: String,
    coreSimulatorHandle: UnsafeMutableRawPointer,
    edge: Edge = .none,
    // Apple's gesture recognizer only needs a down/up pair within ~50 ms to
    // register a tap.
    holdMicros: UInt32 = 50_000
  ) throws {
    _ = symbols  // resolved by caller, unused here — see the split note above.

    let device = try CoreSimulatorBridge.resolveSimDevice(
      udid: udid,
      developerDir: developerDir,
      coreSimulatorHandle: coreSimulatorHandle
    )
    let client = try buildLegacyHIDClient(device: device)

    try sendPhase(
      client: client, symbols: digitizerSymbols,
      x: x, y: y, phase: .down, edge: edge
    )
    usleep(holdMicros)
    try sendPhase(
      client: client, symbols: digitizerSymbols,
      x: x, y: y, phase: .up, edge: edge
    )
    // Settle so event delivery starts before the sidecar IPC returns; 5 ms is
    // enough for the queue handoff. The caller's next action provides natural
    // synchronisation beyond that.
    usleep(5_000)
  }

  // MARK: - Pure byte-patch (testable)

  /// In-place patch of a SimulatorKit trackpad-wrapped message blob.
  ///
  /// Layout (verified against baguette L210-225):
  ///   - `[0x6c..0x70)` ← UInt32 LE `kIndigoHIDTouchTarget = 0x32` (always written).
  ///   - `[0x10c..0x110)` ← UInt32 LE `kIndigoHIDTouchTarget = 0x32`
  ///     **only when `size >= 0x110`** (the wrapper produces a 0x180-byte
  ///     dual-record message for tap; 5-arg legacy returns a smaller blob).
  ///   - `[0x3a]` ← UInt8 1 (edge present) or 0 (edge absent).
  ///   - `[0x3b]` ← UInt8 edge bit (0x00 / 0x01 / 0x02 / 0x04 / 0x08).
  ///   - `[0xda]` and `[0xdb]` ← same edge present + edge bit pair,
  ///     **only when `size >= 0xdc`** (second touch-record slot).
  ///
  /// `size` is injected for unit-test reproducibility (production passes the
  /// `malloc_size(msg)` of the C-allocated blob).
  internal static func patch(
    message: UnsafeMutableRawPointer,
    edge: Edge,
    size: Int
  ) {
    // Target tag (always at 0x6c if blob is at least that big).
    if size >= DigitizerConstants.kTargetByteOffset0 + 4 {
      let p = message
        .advanced(by: DigitizerConstants.kTargetByteOffset0)
        .assumingMemoryBound(to: UInt32.self)
      p.pointee = DigitizerConstants.indigoHIDTouchTarget
    }
    if size >= DigitizerConstants.kMinSizeForSecondTargetSlot {
      let p = message
        .advanced(by: DigitizerConstants.kTargetByteOffset1)
        .assumingMemoryBound(to: UInt32.self)
      p.pointee = DigitizerConstants.indigoHIDTouchTarget
    }

    let edgePresent: UInt8 = (edge == .none) ? 0x00 : 0x01

    if size >= DigitizerConstants.kEdgeBitByteOffsetA + 1 {
      message
        .advanced(by: DigitizerConstants.kEdgePresentByteOffsetA)
        .assumingMemoryBound(to: UInt8.self).pointee = edgePresent
      message
        .advanced(by: DigitizerConstants.kEdgeBitByteOffsetA)
        .assumingMemoryBound(to: UInt8.self).pointee = edge.bit
    }
    if size >= DigitizerConstants.kMinSizeForSecondEdgeSlot {
      message
        .advanced(by: DigitizerConstants.kEdgePresentByteOffsetB)
        .assumingMemoryBound(to: UInt8.self).pointee = edgePresent
      message
        .advanced(by: DigitizerConstants.kEdgeBitByteOffsetB)
        .assumingMemoryBound(to: UInt8.self).pointee = edge.bit
    }
  }

  // MARK: - private

  private static func buildLegacyHIDClient(device: AnyObject) throws -> AnyObject {
    let className = "_TtC12SimulatorKit24SimDeviceLegacyHIDClient"
    guard let cls = NSClassFromString(className) else {
      throw HostHIDError.classLookupFailed(name: className)
    }

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

  /// Builds one phase's IOHIDEvent (parent digitizer + finger child + append),
  /// wraps it via SimulatorKit's trackpad-event wrapper, byte-patches the
  /// target tag + edge bits, then ships through `LegacyHIDClientSender`.
  private static func sendPhase(
    client: AnyObject,
    symbols: DigitizerSymbols,
    x: Double, y: Double,
    phase: Phase,
    edge: Edge
  ) throws {
    let createDigitizer = unsafeBitCast(
      symbols.createDigitizerEvent, to: CreateDigitizerEventFn.self
    )
    let createFinger = unsafeBitCast(
      symbols.createFingerEvent, to: CreateFingerEventFn.self
    )
    let appendEvent = unsafeBitCast(
      symbols.appendEvent, to: AppendEventFn.self
    )
    let trackpadWrap = unsafeBitCast(
      symbols.trackpadWrap, to: TrackpadWrapFn.self
    )

    let now = mach_absolute_time()
    let mask = phase.eventMask
    let range = phase.range
    let touch = phase.touch
    let identifier: UInt32 = 1  // fixed for a single-tap session; swipe would auto-increment

    // Parent digitizer event (15-arg). transducer=2 (Finger), index=0,
    // identifier=1, mask=phase mask, button=0,
    // x, y, z=0.0, pressure=0.0, twist=0.0, range, touch, options=0.
    guard let parentRaw = createDigitizer(
      nil, now,
      DigitizerConstants.transducerFinger,  // transducer type (Finger)
      0,                                    // index
      identifier,                           // identifier
      mask,                                 // event mask
      0,                                    // button
      x, y,                                 // position (normalized)
      0.0,                                  // z
      0.0,                                  // pressure (non-zero crashes early — baguette note)
      0.0,                                  // twist
      range, touch,                         // booleans
      0                                     // options
    ) else {
      throw HostHIDError.sendFailed(stage: "createDigitizerEvent returned nil (phase=\(phase))")
    }
    let parent = parentRaw.takeRetainedValue()

    // Finger child event (13-arg). index=0, identifier=1, mask=phase mask,
    // x, y, z=0.0, pressure=0.0, twist=0.0, range, touch, options=0.
    guard let fingerRaw = createFinger(
      nil, now,
      0,             // index
      identifier,    // identifier
      mask,          // event mask
      x, y, 0.0,     // position
      0.0,           // pressure
      0.0,           // twist
      range, touch,
      0
    ) else {
      throw HostHIDError.sendFailed(stage: "createFingerEvent returned nil (phase=\(phase))")
    }
    let finger = fingerRaw.takeRetainedValue()

    appendEvent(parent, finger, 0)

    // Pass the parent CFTypeRef through the trackpad-event wrapper (only
    // *FromHIDEventRef function that accepts digitizer-typed IOHIDEvent).
    let parentRef = Unmanaged.passUnretained(parent).toOpaque()
    guard let message = trackpadWrap(parentRef) else {
      throw HostHIDError.sendFailed(
        stage: "IndigoHIDMessageForTrackpadEventFromHIDEventRef returned nil (phase=\(phase))"
      )
    }

    let sz = malloc_size(message)
    patch(message: message, edge: edge, size: Int(sz))

    try LegacyHIDClientSender.send(
      message: message, to: client, stage: "digitizer-\(phase)"
    )
  }
}

// MARK: - DigitizerSymbols

/// Four C symbols required by the IOHIDEvent digitizer path.
///
/// - Three live in IOKit (public framework, already in dyld shared cache —
///   resolvable through `dlsym(RTLD_DEFAULT)` sentinel).
/// - One lives in SimulatorKit (private — requires explicit `dlopen` on
///   `<dev>/Library/PrivateFrameworks/SimulatorKit.framework/SimulatorKit`).
public struct DigitizerSymbols: Equatable {
  public let createDigitizerEvent: UnsafeMutableRawPointer
  public let createFingerEvent:    UnsafeMutableRawPointer
  public let appendEvent:          UnsafeMutableRawPointer
  public let trackpadWrap:         UnsafeMutableRawPointer

  public init(
    createDigitizerEvent: UnsafeMutableRawPointer,
    createFingerEvent: UnsafeMutableRawPointer,
    appendEvent: UnsafeMutableRawPointer,
    trackpadWrap: UnsafeMutableRawPointer
  ) {
    self.createDigitizerEvent = createDigitizerEvent
    self.createFingerEvent = createFingerEvent
    self.appendEvent = appendEvent
    self.trackpadWrap = trackpadWrap
  }

  /// Resolves the four symbols.
  ///
  /// Private-symbol invariant: `dlsym` access only. IOKit (public) goes
  /// through the `RTLD_DEFAULT` sentinel; SimulatorKit (private) is loaded
  /// with explicit `dlopen` by the caller and the handle passed in.
  public static func resolve(
    ioKitHandle: UnsafeMutableRawPointer,
    simulatorKitHandle: UnsafeMutableRawPointer,
    via resolver: DlsymResolver
  ) throws -> DigitizerSymbols {
    func ioKit(_ name: String) throws -> UnsafeMutableRawPointer {
      guard let p = resolver.sym(ioKitHandle, name) else {
        throw HostHIDError.dlsymFailed(name: name)
      }
      return p
    }
    func sk(_ name: String) throws -> UnsafeMutableRawPointer {
      guard let p = resolver.sym(simulatorKitHandle, name) else {
        throw HostHIDError.dlsymFailed(name: name)
      }
      return p
    }

    let cde = try ioKit(DigitizerSymbolNames.createDigitizerEvent)
    let cfe = try ioKit(DigitizerSymbolNames.createFingerEvent)
    let app = try ioKit(DigitizerSymbolNames.appendEvent)
    let tw  = try sk(DigitizerSymbolNames.trackpadWrap)
    return DigitizerSymbols(
      createDigitizerEvent: cde,
      createFingerEvent: cfe,
      appendEvent: app,
      trackpadWrap: tw
    )
  }

  /// `RTLD_DEFAULT` sentinel handle (`-2`). The libc/dyld convention for
  /// "search all loaded images in default load order". Used for IOKit
  /// (public framework, already in dyld shared cache).
  ///
  /// The dlsym-only invariant still holds: this is a dlsym handle, with no
  /// IOKit module import and no `linkedFramework` in Package.swift.
  public static let rtldDefault: UnsafeMutableRawPointer =
    UnsafeMutableRawPointer(bitPattern: -2)!
}

/// Wire-stable symbol names for the digitizer path.
public struct DigitizerSymbolNames {
  public static let createDigitizerEvent =
    "IOHIDEventCreateDigitizerEvent"
  public static let createFingerEvent    =
    "IOHIDEventCreateDigitizerFingerEvent"
  public static let appendEvent          = "IOHIDEventAppendEvent"
  public static let trackpadWrap         =
    "IndigoHIDMessageForTrackpadEventFromHIDEventRef"

  public static let allRequired: [String] = [
    createDigitizerEvent, createFingerEvent, appendEvent, trackpadWrap,
  ]
}

/// Byte-level ABI constants — pinned by `IOHIDDigitizerTapTests`.
/// Any "drift while tidying" silently breaks the patched event delivery
/// (same locking rationale as `IndigoConstants`).
public enum DigitizerConstants {
  /// `IndigoHIDTarget.touch` = 0x32 (same target id as `IndigoConstants.touchDigitizer`
  /// — the digitizer subsystem on the wire-ABI side; identical routing).
  public static let indigoHIDTouchTarget:        UInt32 = 0x32

  /// `kIOHIDDigitizerTransducerTypeFinger`.
  public static let transducerFinger:            UInt32 = 2

  // Byte offsets into the trackpad-wrapped message blob.
  public static let kTargetByteOffset0:          Int    = 0x6c
  public static let kTargetByteOffset1:          Int    = 0x10c
  public static let kEdgePresentByteOffsetA:     Int    = 0x3a
  public static let kEdgeBitByteOffsetA:         Int    = 0x3b
  public static let kEdgePresentByteOffsetB:     Int    = 0xda
  public static let kEdgeBitByteOffsetB:         Int    = 0xdb

  /// Min message size at which we patch the second target slot at 0x10c.
  public static let kMinSizeForSecondTargetSlot: Int    = 0x110
  /// Min message size at which we patch the second edge slot at 0xda/0xdb.
  public static let kMinSizeForSecondEdgeSlot:   Int    = 0xdc
}

// MARK: - typealiases (exact arg shapes from baguette / IOKit headers)

/// `IOHIDEventCreateDigitizerEvent` (CoreFoundation-style allocator + abs
/// time + 5 UInt32 enums + 5 Doubles + 2 Bool flags + 1 UInt32 options).
public typealias CreateDigitizerEventFn = @convention(c) (
  CFAllocator?, UInt64,
  UInt32, UInt32, UInt32, UInt32, UInt32,
  Double, Double, Double, Double, Double,
  Bool, Bool, UInt32
) -> Unmanaged<CFTypeRef>?

/// `IOHIDEventCreateDigitizerFingerEvent` (no transducer arg — implied finger).
public typealias CreateFingerEventFn = @convention(c) (
  CFAllocator?, UInt64,
  UInt32, UInt32, UInt32,
  Double, Double, Double, Double, Double,
  Bool, Bool, UInt32
) -> Unmanaged<CFTypeRef>?

/// `IOHIDEventAppendEvent(parent, child, options=0)`.
public typealias AppendEventFn  = @convention(c) (CFTypeRef, CFTypeRef, UInt32) -> Void

/// `IndigoHIDMessageForTrackpadEventFromHIDEventRef(eventRef)`.
/// Returns a malloc'd C blob (the SimDeviceLegacyHIDClient takes ownership
/// via `freeWhenDone:true`).
public typealias TrackpadWrapFn = @convention(c) (UnsafeRawPointer) -> UnsafeMutableRawPointer?

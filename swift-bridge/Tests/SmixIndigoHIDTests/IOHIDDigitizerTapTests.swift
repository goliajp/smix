import XCTest
@testable import SmixIndigoHID

/// C4 unit coverage for `IOHIDDigitizerTap`. Targets host-side (zero
/// simulator) reach: byte-patch pure function + Phase/Edge enums + dlsym
/// failure paths. End-to-end IOHIDEvent + trackpad wrap + send is covered
/// by `scripts/smix-hostid-digitizer-smoke.sh` (real dev-sim).
final class IOHIDDigitizerTapTests: XCTestCase {

  // MARK: - Phase / Edge constants

  func test_phase_eventMask_downAndMove_are_0x07_up_is_0x06() {
    XCTAssertEqual(IOHIDDigitizerTap.Phase.down.eventMask, UInt32(0x07))
    XCTAssertEqual(IOHIDDigitizerTap.Phase.move.eventMask, UInt32(0x07))
    XCTAssertEqual(IOHIDDigitizerTap.Phase.up.eventMask, UInt32(0x06))
  }

  func test_phase_range_touch_falseOnUp_trueOtherwise() {
    XCTAssertTrue(IOHIDDigitizerTap.Phase.down.range)
    XCTAssertTrue(IOHIDDigitizerTap.Phase.down.touch)
    XCTAssertTrue(IOHIDDigitizerTap.Phase.move.range)
    XCTAssertTrue(IOHIDDigitizerTap.Phase.move.touch)
    XCTAssertFalse(IOHIDDigitizerTap.Phase.up.range)
    XCTAssertFalse(IOHIDDigitizerTap.Phase.up.touch)
  }

  func test_edge_bits_all_5_cases() {
    XCTAssertEqual(IOHIDDigitizerTap.Edge.none.bit,   UInt8(0x00))
    XCTAssertEqual(IOHIDDigitizerTap.Edge.left.bit,   UInt8(0x02))
    XCTAssertEqual(IOHIDDigitizerTap.Edge.top.bit,    UInt8(0x08))
    XCTAssertEqual(IOHIDDigitizerTap.Edge.right.bit,  UInt8(0x04))
    XCTAssertEqual(IOHIDDigitizerTap.Edge.bottom.bit, UInt8(0x01))
  }

  // MARK: - Byte-offset constants

  func test_byteOffsets_constants_match_baguette() {
    XCTAssertEqual(DigitizerConstants.indigoHIDTouchTarget,       UInt32(0x32))
    XCTAssertEqual(DigitizerConstants.transducerFinger,           UInt32(2))
    XCTAssertEqual(DigitizerConstants.kTargetByteOffset0,         0x6c)
    XCTAssertEqual(DigitizerConstants.kTargetByteOffset1,         0x10c)
    XCTAssertEqual(DigitizerConstants.kEdgePresentByteOffsetA,    0x3a)
    XCTAssertEqual(DigitizerConstants.kEdgeBitByteOffsetA,        0x3b)
    XCTAssertEqual(DigitizerConstants.kEdgePresentByteOffsetB,    0xda)
    XCTAssertEqual(DigitizerConstants.kEdgeBitByteOffsetB,        0xdb)
    XCTAssertEqual(DigitizerConstants.kMinSizeForSecondTargetSlot, 0x110)
    XCTAssertEqual(DigitizerConstants.kMinSizeForSecondEdgeSlot,   0xdc)
  }

  // MARK: - patch() pure function

  /// Allocate a zeroed buffer of `size` bytes; auto-frees on test teardown.
  private func allocZeroed(_ size: Int) -> UnsafeMutableRawPointer {
    let p = UnsafeMutableRawPointer.allocate(byteCount: size, alignment: 8)
    p.initializeMemory(as: UInt8.self, repeating: 0, count: size)
    self.addTeardownBlock { p.deallocate() }
    return p
  }

  private func u32(_ p: UnsafeMutableRawPointer, at offset: Int) -> UInt32 {
    return p.advanced(by: offset).assumingMemoryBound(to: UInt32.self).pointee
  }
  private func u8(_ p: UnsafeMutableRawPointer, at offset: Int) -> UInt8 {
    return p.advanced(by: offset).assumingMemoryBound(to: UInt8.self).pointee
  }

  func test_patchMessage_writesTargetAtBothOffsets_when_size_ge_0x110() {
    let buf = allocZeroed(0x200)
    IOHIDDigitizerTap.patch(message: buf, edge: .none, size: 0x200)
    XCTAssertEqual(u32(buf, at: 0x6c),  UInt32(0x32),
                   "target tag must be written at 0x6c")
    XCTAssertEqual(u32(buf, at: 0x10c), UInt32(0x32),
                   "target tag must be written at 0x10c when size >= 0x110")
    XCTAssertEqual(u8(buf, at: 0x3a), UInt8(0x00), "edge present = 0 for .none")
    XCTAssertEqual(u8(buf, at: 0x3b), UInt8(0x00), "edge bit = 0 for .none")
  }

  func test_patchMessage_skipsSecondTargetSlot_when_size_lt_0x110() {
    let buf = allocZeroed(0xa0)
    IOHIDDigitizerTap.patch(message: buf, edge: .none, size: 0xa0)
    XCTAssertEqual(u32(buf, at: 0x6c), UInt32(0x32),
                   "first target tag still written at 0x6c")
    // Buffer is only 0xa0 bytes; we must not have written past it. We can't
    // safely read 0x10c (out-of-bounds) so the safety is implicit — the
    // patch() function's size check is what we exercise here. To verify no
    // wild-write happened earlier, sanity-check 0x9c (a stride boundary
    // before the buffer ends).
    XCTAssertEqual(u32(buf, at: 0x9c), UInt32(0),
                   "byte region before buffer end must remain zero")
  }

  func test_patchMessage_writesEdgeBottomBits() {
    let buf = allocZeroed(0x200)
    IOHIDDigitizerTap.patch(message: buf, edge: .bottom, size: 0x200)
    // Edge slot A
    XCTAssertEqual(u8(buf, at: 0x3a), UInt8(0x01),
                   "edge present byte must be 1 when edge != .none")
    XCTAssertEqual(u8(buf, at: 0x3b), UInt8(0x01),
                   ".bottom edge bit = 0x01")
    // Edge slot B (second touch record)
    XCTAssertEqual(u8(buf, at: 0xda), UInt8(0x01),
                   "edge present byte at 0xda must be 1 when size >= 0xdc")
    XCTAssertEqual(u8(buf, at: 0xdb), UInt8(0x01),
                   ".bottom edge bit must be replicated at 0xdb")
  }

  // MARK: - DigitizerSymbolNames

  func test_digitizerSymbolNames_constant_values() {
    XCTAssertEqual(DigitizerSymbolNames.createDigitizerEvent,
                   "IOHIDEventCreateDigitizerEvent")
    XCTAssertEqual(DigitizerSymbolNames.createFingerEvent,
                   "IOHIDEventCreateDigitizerFingerEvent")
    XCTAssertEqual(DigitizerSymbolNames.appendEvent,
                   "IOHIDEventAppendEvent")
    XCTAssertEqual(DigitizerSymbolNames.trackpadWrap,
                   "IndigoHIDMessageForTrackpadEventFromHIDEventRef")
    XCTAssertEqual(
      DigitizerSymbolNames.allRequiredForC4,
      [
        "IOHIDEventCreateDigitizerEvent",
        "IOHIDEventCreateDigitizerFingerEvent",
        "IOHIDEventAppendEvent",
        "IndigoHIDMessageForTrackpadEventFromHIDEventRef",
      ]
    )
  }

  // MARK: - DigitizerSymbols.resolve via fake resolver

  func test_digitizerSymbols_resolve_missing_throws_dlsymFailed() {
    let ioKitHandle = UnsafeMutableRawPointer(bitPattern: 0x1ABCDEF0)!
    let skHandle    = UnsafeMutableRawPointer(bitPattern: 0x2ABCDEF0)!

    // Case A: SimulatorKit trackpad wrap missing.
    let r1 = FakeDigitizerResolver()
    r1.set(handle: ioKitHandle, name: "IOHIDEventCreateDigitizerEvent",
           ptr: UnsafeMutableRawPointer(bitPattern: 0x10)!)
    r1.set(handle: ioKitHandle, name: "IOHIDEventCreateDigitizerFingerEvent",
           ptr: UnsafeMutableRawPointer(bitPattern: 0x20)!)
    r1.set(handle: ioKitHandle, name: "IOHIDEventAppendEvent",
           ptr: UnsafeMutableRawPointer(bitPattern: 0x30)!)
    // SimulatorKit trackpadWrap absent.
    XCTAssertThrowsError(
      try DigitizerSymbols.resolve(
        ioKitHandle: ioKitHandle,
        simulatorKitHandle: skHandle,
        via: r1
      )
    ) { err in
      XCTAssertEqual(
        err as? HostHIDError,
        .dlsymFailed(name: "IndigoHIDMessageForTrackpadEventFromHIDEventRef")
      )
    }

    // Case B: IOKit createDigitizerEvent missing — must throw on the first
    // missing IOKit symbol (resolution is short-circuit).
    let r2 = FakeDigitizerResolver()
    // IOHIDEventCreateDigitizerEvent absent.
    r2.set(handle: ioKitHandle, name: "IOHIDEventCreateDigitizerFingerEvent",
           ptr: UnsafeMutableRawPointer(bitPattern: 0x20)!)
    r2.set(handle: ioKitHandle, name: "IOHIDEventAppendEvent",
           ptr: UnsafeMutableRawPointer(bitPattern: 0x30)!)
    r2.set(handle: skHandle, name: "IndigoHIDMessageForTrackpadEventFromHIDEventRef",
           ptr: UnsafeMutableRawPointer(bitPattern: 0x40)!)
    XCTAssertThrowsError(
      try DigitizerSymbols.resolve(
        ioKitHandle: ioKitHandle,
        simulatorKitHandle: skHandle,
        via: r2
      )
    ) { err in
      XCTAssertEqual(
        err as? HostHIDError,
        .dlsymFailed(name: "IOHIDEventCreateDigitizerEvent")
      )
    }
  }
}

/// Fake resolver scoped to digitizer-symbol tests; keyed by (handle, name)
/// because IOKit + SimulatorKit symbols use distinct handles. Class (not
/// struct) so the test can mutate the symbol bag after `init()`.
private final class FakeDigitizerResolver: DlsymResolver {
  private struct Key: Hashable {
    let handle: UnsafeMutableRawPointer
    let name: String
  }

  private var symbols: [Key: UnsafeMutableRawPointer] = [:]
  private var lastError: String = "fake: no error"

  init() {}

  func set(handle: UnsafeMutableRawPointer, name: String, ptr: UnsafeMutableRawPointer) {
    symbols[Key(handle: handle, name: name)] = ptr
  }

  func open(_ path: String) -> UnsafeMutableRawPointer? {
    // Digitizer-resolve doesn't open — caller supplies handles directly.
    return UnsafeMutableRawPointer(bitPattern: 0xDEADBEEF)
  }

  func sym(_ handle: UnsafeMutableRawPointer, _ name: String) -> UnsafeMutableRawPointer? {
    return symbols[Key(handle: handle, name: name)]
  }

  func lastErrorDescription() -> String { lastError }
}

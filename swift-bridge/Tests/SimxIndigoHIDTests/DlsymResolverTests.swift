import XCTest
@testable import SimxIndigoHID

/// Test double — backing map of (handle, symbolName) → pointer.
/// Production code goes through `DlsymResolver` protocol, so these tests cover
/// the symbol-resolution flow without touching real SimulatorKit.
private final class FakeDlsymResolver: DlsymResolver {
  /// All symbols are looked up on the same fake handle.
  static let fakeHandle = UnsafeMutableRawPointer(bitPattern: 0xFADE_F00D)!

  var openable: Set<String>
  /// Map sym name → pointer (nil means "missing").
  var symbols: [String: UnsafeMutableRawPointer?]
  var error: String = "fake: no error"

  init(openable: Set<String> = ["/fake/SimulatorKit"],
       symbols: [String: UnsafeMutableRawPointer?] = [:]) {
    self.openable = openable
    self.symbols = symbols
  }

  func open(_ path: String) -> UnsafeMutableRawPointer? {
    if openable.contains(path) {
      return Self.fakeHandle
    }
    error = "fake: cannot open \(path)"
    return nil
  }

  func sym(_ handle: UnsafeMutableRawPointer, _ name: String) -> UnsafeMutableRawPointer? {
    guard handle == Self.fakeHandle else {
      error = "fake: unknown handle"
      return nil
    }
    if let p = symbols[name] {
      // p may itself be nil (i.e. symbol explicitly absent)
      if p == nil { error = "fake: symbol not found: \(name)" }
      return p
    }
    error = "fake: symbol not found: \(name)"
    return nil
  }

  func lastErrorDescription() -> String { error }
}

final class DlsymResolverTests: XCTestCase {
  private let p1 = UnsafeMutableRawPointer(bitPattern: 0x1111)!
  private let p2 = UnsafeMutableRawPointer(bitPattern: 0x2222)!
  private let p3 = UnsafeMutableRawPointer(bitPattern: 0x3333)!
  private let p4 = UnsafeMutableRawPointer(bitPattern: 0x4444)!
  private let p5 = UnsafeMutableRawPointer(bitPattern: 0x5555)!
  private let p6 = UnsafeMutableRawPointer(bitPattern: 0x6666)!

  private func allRequiredPresent(
    optionalsPresent: Bool = true
  ) -> [String: UnsafeMutableRawPointer?] {
    var m: [String: UnsafeMutableRawPointer?] = [
      IndigoSymbolNames.mouseNSEvent: p1,
      IndigoSymbolNames.button: p2,
      IndigoSymbolNames.createPointerSvc: p3,
      IndigoSymbolNames.createMouseSvc: p4,
    ]
    if optionalsPresent {
      m[IndigoSymbolNames.removePointerSvc] = p5
      m[IndigoSymbolNames.hidArbitrary] = p6
    }
    return m
  }

  func test_resolve_allRequiredSymbolsPresent_returnsIndigoSymbols() throws {
    let resolver = FakeDlsymResolver(symbols: allRequiredPresent())
    let handle = try XCTUnwrap(resolver.open("/fake/SimulatorKit"))
    let syms = try IndigoSymbols.resolve(handle: handle, via: resolver)
    XCTAssertEqual(syms.mouseFn, p1)
    XCTAssertEqual(syms.buttonFn, p2)
    XCTAssertEqual(syms.createPointerSvc, p3)
    XCTAssertEqual(syms.createMouseSvc, p4)
    XCTAssertEqual(syms.removePointerSvc, p5)
    XCTAssertEqual(syms.hidArbitrary, p6)
  }

  func test_resolve_missingMouseNSEvent_throwsDlsymFailed() {
    var m = allRequiredPresent()
    m[IndigoSymbolNames.mouseNSEvent] = nil as UnsafeMutableRawPointer?
    let resolver = FakeDlsymResolver(symbols: m)
    guard let handle = resolver.open("/fake/SimulatorKit") else {
      return XCTFail("fake open failed")
    }
    XCTAssertThrowsError(try IndigoSymbols.resolve(handle: handle, via: resolver)) { err in
      XCTAssertEqual(
        err as? HostHIDError,
        .dlsymFailed(name: IndigoSymbolNames.mouseNSEvent)
      )
    }
  }

  func test_resolve_missingCreatePointerService_throwsDlsymFailed() {
    var m = allRequiredPresent()
    m[IndigoSymbolNames.createPointerSvc] = nil as UnsafeMutableRawPointer?
    let resolver = FakeDlsymResolver(symbols: m)
    guard let handle = resolver.open("/fake/SimulatorKit") else {
      return XCTFail("fake open failed")
    }
    XCTAssertThrowsError(try IndigoSymbols.resolve(handle: handle, via: resolver)) { err in
      XCTAssertEqual(
        err as? HostHIDError,
        .dlsymFailed(name: IndigoSymbolNames.createPointerSvc)
      )
    }
  }

  func test_resolve_optionalSymbolMissing_doesNotThrow() throws {
    // required all present; remove/arbitrary absent
    let m = allRequiredPresent(optionalsPresent: false)
    let resolver = FakeDlsymResolver(symbols: m)
    let handle = try XCTUnwrap(resolver.open("/fake/SimulatorKit"))
    let syms = try IndigoSymbols.resolve(handle: handle, via: resolver)
    XCTAssertNil(syms.removePointerSvc)
    XCTAssertNil(syms.hidArbitrary)
    // required still populated
    XCTAssertEqual(syms.mouseFn, p1)
    XCTAssertEqual(syms.buttonFn, p2)
  }
}

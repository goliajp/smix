import Foundation
#if canImport(Darwin)
import Darwin
#endif

/// Indirection around libc `dlopen` / `dlsym` so unit tests can inject a
/// fake resolver (zero real-SimulatorKit dependency for the bulk of error
/// paths). Production path is `SystemDlsymResolver` below.
public protocol DlsymResolver {
  func open(_ path: String) -> UnsafeMutableRawPointer?
  func sym(_ handle: UnsafeMutableRawPointer, _ name: String) -> UnsafeMutableRawPointer?
  func lastErrorDescription() -> String
}

/// Production resolver: thin wrapper over libc `dlopen` / `dlsym` / `dlerror`.
/// Uses `RTLD_NOW` so missing symbols surface immediately at open time
/// (matches baguette's `IndigoHIDInput.resolveFunctions`).
public struct SystemDlsymResolver: DlsymResolver {
  public init() {}

  public func open(_ path: String) -> UnsafeMutableRawPointer? {
    return path.withCString { cPath in
      dlopen(cPath, RTLD_NOW)
    }
  }

  public func sym(_ handle: UnsafeMutableRawPointer, _ name: String) -> UnsafeMutableRawPointer? {
    return name.withCString { cName in
      dlsym(handle, cName)
    }
  }

  public func lastErrorDescription() -> String {
    guard let err = dlerror() else { return "(no dlerror)" }
    return String(cString: err)
  }
}

/// Wire-stable symbol names. Kept as `public static let` (not enum cases) so
/// they can be passed around as `String` without unwrapping ceremony.
public struct IndigoSymbolNames {
  public static let mouseNSEvent       = "IndigoHIDMessageForMouseNSEvent"
  public static let button             = "IndigoHIDMessageForButton"
  public static let createPointerSvc   = "IndigoHIDMessageToCreatePointerService"
  public static let createMouseSvc     = "IndigoHIDMessageToCreateMouseService"
  public static let removePointerSvc   = "IndigoHIDMessageToRemovePointerService"
  public static let hidArbitrary       = "IndigoHIDMessageForHIDArbitrary"

  /// Symbols actually invoked by C3. Missing any → `dlsymFailed`.
  public static let allRequiredForC3: [String] = [
    mouseNSEvent, button, createPointerSvc, createMouseSvc,
  ]

  /// Resolved best-effort; missing is OK (C3 doesn't call them; C4+ may).
  public static let optionalForC3: [String] = [
    removePointerSvc, hidArbitrary,
  ]
}

/// Bag of resolved private-symbol function pointers. `mouseFn`,
/// `buttonFn`, `createPointerSvc`, `createMouseSvc` are required (C3 actually
/// calls all but `buttonFn`); the last two are best-effort for forward
/// compatibility.
public struct IndigoSymbols: Equatable {
  public let mouseFn: UnsafeMutableRawPointer
  public let buttonFn: UnsafeMutableRawPointer
  public let createPointerSvc: UnsafeMutableRawPointer
  public let createMouseSvc: UnsafeMutableRawPointer
  public let removePointerSvc: UnsafeMutableRawPointer?
  public let hidArbitrary: UnsafeMutableRawPointer?

  public init(
    mouseFn: UnsafeMutableRawPointer,
    buttonFn: UnsafeMutableRawPointer,
    createPointerSvc: UnsafeMutableRawPointer,
    createMouseSvc: UnsafeMutableRawPointer,
    removePointerSvc: UnsafeMutableRawPointer?,
    hidArbitrary: UnsafeMutableRawPointer?
  ) {
    self.mouseFn = mouseFn
    self.buttonFn = buttonFn
    self.createPointerSvc = createPointerSvc
    self.createMouseSvc = createMouseSvc
    self.removePointerSvc = removePointerSvc
    self.hidArbitrary = hidArbitrary
  }

  public static func resolve(
    handle: UnsafeMutableRawPointer,
    via resolver: DlsymResolver
  ) throws -> IndigoSymbols {
    func required(_ name: String) throws -> UnsafeMutableRawPointer {
      guard let p = resolver.sym(handle, name) else {
        throw HostHIDError.dlsymFailed(name: name)
      }
      return p
    }
    func optional(_ name: String) -> UnsafeMutableRawPointer? {
      return resolver.sym(handle, name)
    }

    let mouse = try required(IndigoSymbolNames.mouseNSEvent)
    let button = try required(IndigoSymbolNames.button)
    let createPointer = try required(IndigoSymbolNames.createPointerSvc)
    let createMouse = try required(IndigoSymbolNames.createMouseSvc)
    let removePointer = optional(IndigoSymbolNames.removePointerSvc)
    let hidArbitrary = optional(IndigoSymbolNames.hidArbitrary)

    return IndigoSymbols(
      mouseFn: mouse,
      buttonFn: button,
      createPointerSvc: createPointer,
      createMouseSvc: createMouse,
      removePointerSvc: removePointer,
      hidArbitrary: hidArbitrary
    )
  }
}

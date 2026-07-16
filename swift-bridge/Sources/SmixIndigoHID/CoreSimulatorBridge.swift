import Foundation
#if canImport(ObjectiveC)
import ObjectiveC.runtime
#endif

/// Bridges to CoreSimulator (SimDevice / SimDeviceSet) via dlopen + ObjC
/// runtime — the framework is loaded dynamically; we never declare a module
/// import for it (dlsym-only invariant for private frameworks).
public enum CoreSimulatorBridge {
  /// SimulatorKit lives per-Xcode under `xcode-select -p`.
  public static let simulatorKitPath: (_ developerDir: String) -> String = { dev in
    "\(dev)/Library/PrivateFrameworks/SimulatorKit.framework/SimulatorKit"
  }

  /// CoreSimulator is system-wide; path is independent of the active Xcode.
  public static let coreSimulatorPath: String =
    "/Library/Developer/PrivateFrameworks/CoreSimulator.framework/CoreSimulator"

  /// `xcode-select -p` → developer dir (e.g. `/Applications/Xcode.app/Contents/Developer`).
  public static func developerDir() throws -> String {
    let proc = Process()
    proc.executableURL = URL(fileURLWithPath: "/usr/bin/xcode-select")
    proc.arguments = ["-p"]
    let pipe = Pipe()
    proc.standardOutput = pipe
    proc.standardError = Pipe()  // swallow stderr
    do {
      try proc.run()
    } catch {
      throw HostHIDError.initFailed(detail: "spawn xcode-select: \(error)")
    }
    proc.waitUntilExit()
    if proc.terminationStatus != 0 {
      throw HostHIDError.initFailed(detail: "xcode-select -p exit \(proc.terminationStatus)")
    }
    let data = pipe.fileHandleForReading.readDataToEndOfFile()
    guard let s = String(data: data, encoding: .utf8) else {
      throw HostHIDError.initFailed(detail: "xcode-select -p output is not UTF-8")
    }
    let trimmed = s.trimmingCharacters(in: .whitespacesAndNewlines)
    if trimmed.isEmpty {
      throw HostHIDError.initFailed(detail: "xcode-select -p returned empty path")
    }
    return trimmed
  }

  /// Resolves a `SimDevice` by udid by going through CoreSimulator's
  /// `SimServiceContext` (the supported entry point on modern Xcode):
  ///
  ///   1. `+[SimServiceContext sharedServiceContextForDeveloperDir:error:]`
  ///   2. `-[SimServiceContext defaultDeviceSetWithError:]`
  ///   3. `-[SimDeviceSet devicesByUDID]` → `[NSUUID: SimDevice]`
  ///
  /// (`+[SimDeviceSet defaultSet]` doesn't exist on the modern class — only
  /// `+defaultSetPath` is present in CoreSimulator's metadata.)
  ///
  /// Throws `.deviceLookupFailed` if no SimDevice with that udid exists,
  /// `.deviceNotBooted` if it exists but `state` ≠ 3 (Booted).
  public static func resolveSimDevice(
    udid: String,
    developerDir: String,
    coreSimulatorHandle: UnsafeMutableRawPointer
  ) throws -> AnyObject {
    _ = coreSimulatorHandle  // handle is held by caller to keep image loaded

    guard let ctxClass = NSClassFromString("SimServiceContext") else {
      throw HostHIDError.classLookupFailed(name: "SimServiceContext")
    }

    // +[SimServiceContext sharedServiceContextForDeveloperDir:error:]
    let ctxSel = NSSelectorFromString("sharedServiceContextForDeveloperDir:error:")
    typealias CtxIMP = @convention(c) (
      AnyClass, Selector, NSString, AutoreleasingUnsafeMutablePointer<NSError?>?
    ) -> AnyObject?
    guard let ctxImpRaw = class_getMethodImplementation(object_getClass(ctxClass), ctxSel) else {
      throw HostHIDError.classLookupFailed(
        name: "+SimServiceContext sharedServiceContextForDeveloperDir:error:"
      )
    }
    let ctxFn = unsafeBitCast(ctxImpRaw, to: CtxIMP.self)
    var ctxErr: NSError?
    let ctx = withUnsafeMutablePointer(to: &ctxErr) { ptr -> AnyObject? in
      return ctxFn(ctxClass, ctxSel, developerDir as NSString, AutoreleasingUnsafeMutablePointer(ptr))
    }
    if let e = ctxErr {
      throw HostHIDError.initFailed(detail: "sharedServiceContextForDeveloperDir: \(e.localizedDescription)")
    }
    guard let context = ctx else {
      throw HostHIDError.initFailed(detail: "sharedServiceContextForDeveloperDir: returned nil")
    }

    // -[SimServiceContext defaultDeviceSetWithError:]
    let setSel = NSSelectorFromString("defaultDeviceSetWithError:")
    typealias SetIMP = @convention(c) (
      AnyObject, Selector, AutoreleasingUnsafeMutablePointer<NSError?>?
    ) -> AnyObject?
    guard let setImpRaw = class_getMethodImplementation(type(of: context), setSel) else {
      throw HostHIDError.classLookupFailed(name: "-SimServiceContext defaultDeviceSetWithError:")
    }
    let setFn = unsafeBitCast(setImpRaw, to: SetIMP.self)
    var setErr: NSError?
    let setAny = withUnsafeMutablePointer(to: &setErr) { ptr -> AnyObject? in
      return setFn(context, setSel, AutoreleasingUnsafeMutablePointer(ptr))
    }
    if let e = setErr {
      throw HostHIDError.initFailed(detail: "defaultDeviceSetWithError: \(e.localizedDescription)")
    }
    guard let deviceSet = setAny else {
      throw HostHIDError.initFailed(detail: "defaultDeviceSetWithError: returned nil")
    }

    // -[SimDeviceSet devicesByUDID] → dict { NSUUID : SimDevice }
    let dictSel = NSSelectorFromString("devicesByUDID")
    let dictRaw = (deviceSet as AnyObject).perform(dictSel)?.takeUnretainedValue()
    guard let dict = dictRaw as? [AnyHashable: AnyObject] else {
      throw HostHIDError.initFailed(detail: "devicesByUDID is not a dict")
    }

    let upper = udid.uppercased()
    guard let uuid = NSUUID(uuidString: upper) else {
      throw HostHIDError.invalidArgument("malformed udid: \(udid)")
    }
    guard let device = dict[uuid as AnyHashable] else {
      throw HostHIDError.deviceLookupFailed(udid: upper)
    }

    // SimDevice exposes `state` as a property (raw value 3 == Booted).
    let stateAny = (device as AnyObject).value(forKey: "state")
    guard let stateNum = stateAny as? NSNumber else {
      throw HostHIDError.initFailed(detail: "SimDevice.state is not NSNumber")
    }
    if stateNum.intValue != 3 {
      throw HostHIDError.deviceNotBooted(udid: upper)
    }

    return device
  }
}

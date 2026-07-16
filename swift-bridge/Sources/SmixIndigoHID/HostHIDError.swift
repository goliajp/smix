import Foundation

/// Typed errors for the host-side HID path. The string codes in
/// `HostHIDErrorCode` are part of the wire contract with TS-side jq parsing
/// downstream (`smix doctor`), so changing them is a breaking change.
public enum HostHIDError: Error, Equatable {
  case dlopenFailed(path: String, detail: String)
  case dlsymFailed(name: String)
  case classLookupFailed(name: String)
  case deviceLookupFailed(udid: String)
  case deviceNotBooted(udid: String)
  case initFailed(detail: String)
  case sendFailed(stage: String)
  case invalidArgument(String)
}

public enum HostHIDErrorCode {
  public static let dlopenFailed       = "dlopen_failed"
  public static let dlsymFailed        = "dlsym_failed"
  public static let classLookupFailed  = "class_lookup_failed"
  public static let deviceLookupFailed = "device_lookup_failed"
  public static let deviceNotBooted    = "udid_not_booted"
  public static let initFailed         = "init_failed"
  public static let sendFailed         = "send_failed"
  public static let invalidArgument    = "invalid_argument"
}

extension HostHIDError {
  /// Stable string code (machine-checkable by jq on the smoke-test side).
  public var code: String {
    switch self {
    case .dlopenFailed:       return HostHIDErrorCode.dlopenFailed
    case .dlsymFailed:        return HostHIDErrorCode.dlsymFailed
    case .classLookupFailed:  return HostHIDErrorCode.classLookupFailed
    case .deviceLookupFailed: return HostHIDErrorCode.deviceLookupFailed
    case .deviceNotBooted:    return HostHIDErrorCode.deviceNotBooted
    case .initFailed:         return HostHIDErrorCode.initFailed
    case .sendFailed:         return HostHIDErrorCode.sendFailed
    case .invalidArgument:    return HostHIDErrorCode.invalidArgument
    }
  }

  /// Human-readable detail for the `detail` JSON field.
  public var detail: String {
    switch self {
    case .dlopenFailed(let path, let detail): return "dlopen \(path): \(detail)"
    case .dlsymFailed(let name):              return "dlsym \(name)"
    case .classLookupFailed(let name):        return "NSClassFromString(\(name)) returned nil"
    case .deviceLookupFailed(let udid):       return "no SimDevice with udid \(udid)"
    case .deviceNotBooted(let udid):          return "SimDevice \(udid) is not booted"
    case .initFailed(let detail):             return detail
    case .sendFailed(let stage):              return "send failed at stage: \(stage)"
    case .invalidArgument(let msg):           return msg
    }
  }
}

import Foundation
#if canImport(ObjectiveC)
import ObjectiveC.runtime
#endif

/// Single-source ObjC selector caller for
/// `SimDeviceLegacyHIDClient.sendWithMessage:freeWhenDone:completionQueue:completion:`.
///
/// Extracted in C4 so both legacy paths (Indigo mouseFn 9-arg, IOHIDEvent
/// digitizer tree) hit one common Mach-port handoff site. Behaviour is
/// byte-identical to the C3 `IndigoHIDDispatcher.sendMessage` it replaced —
/// the C3 test suite still locks the call shape via end-to-end smoke.
///
/// CLAUDE.md §9.6 invariant: we never construct `mach_msg` directly; the Mach
/// port is encapsulated inside the private SimulatorKit class.
internal enum LegacyHIDClientSender {
  /// Sends an Indigo message blob through `SimDeviceLegacyHIDClient`'s
  /// `sendWithMessage:freeWhenDone:completionQueue:completion:` selector.
  ///
  /// `freeWhenDone = true` → the client owns the C-allocated blob;
  /// `completionQueue = nil` / `completion = nil` → fire-and-forget.
  internal static func send(
    message: UnsafeMutableRawPointer,
    to client: AnyObject,
    stage: String
  ) throws {
    let sel = NSSelectorFromString("sendWithMessage:freeWhenDone:completionQueue:completion:")
    guard (client as AnyObject).responds(to: sel) else {
      throw HostHIDError.sendFailed(stage: "\(stage): no -sendWithMessage:... IMP")
    }
    typealias SendIMP = @convention(c) (
      AnyObject, Selector,
      UnsafeMutableRawPointer, Bool,
      DispatchQueue?, (@convention(block) (NSError?) -> Void)?
    ) -> Void
    guard let impRaw = class_getMethodImplementation(type(of: client), sel) else {
      throw HostHIDError.sendFailed(stage: "\(stage): IMP lookup failed")
    }
    let send = unsafeBitCast(impRaw, to: SendIMP.self)
    send(client, sel, message, true, nil, nil)
  }
}

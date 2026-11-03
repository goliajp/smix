import FlyingFox
import Foundation

/// What the runner read about an app that is not in front: its state, and
/// which apps SpringBoard shows in front instead (empty when it could not
/// tell, or when the home screen is showing).
public struct AppNotInFront: Sendable, Equatable {
  public let bundleId: String
  /// `running-background` or `running-background-suspended`.
  public let state: String
  public let inFront: [String]

  public init(bundleId: String, state: String, inFront: [String]) {
    self.bundleId = bundleId
    self.state = state
    self.inFront = inFront
  }
}

/// A query that resolves elements in the app under test, asked while that
/// app is in the background, is answered at once instead of being handed
/// to XCUITest — which looks for the app's window for 30 s, and again,
/// before it gives up (measured: 31 s per attempt, three attempts, on
/// iOS 27).
///
/// Only the routes that read the app's own elements are held to this.
/// `?include=all-windows` reaches SpringBoard's windows too, and a
/// notification banner is exactly what a backgrounded app's flow taps, so
/// it is let through; so is a request that asks for the app to be
/// activated first. A coordinate gesture, typing into whatever holds
/// focus, OCR and the system-popup routes act on the screen, not on the
/// app, and are not held at all.
public enum ForegroundGate {
  public typealias Reader = @Sendable (RequestContext) async -> AppNotInFront?

  // set once at boot, before the server answers anything; a task-local
  // would not reach the handlers FlyingFox spawns per request
  private static let slot = Slot()
  private final class Slot: @unchecked Sendable {
    let lock = NSLock()
    var reader: Reader?
  }

  /// The reader the runner installed; `nil` lets every query through.
  public static var installed: Reader? {
    slot.lock.lock()
    defer { slot.lock.unlock() }
    return slot.reader
  }

  public static func install(_ reader: Reader?) {
    slot.lock.lock()
    slot.reader = reader
    slot.lock.unlock()
  }

  static let heldRoutes: Set<String> = [
    "GET /tree",
    "GET /coordinate-space",
    "POST /find",
    "POST /tap",
    "POST /tap-by-id",
    "POST /fill",
    "POST /clear",
    "POST /hittable",
  ]

  /// Whether `request` is one the gate asks about.
  public static func holds(_ request: HTTPRequest, context: RequestContext) -> Bool {
    holds(
      route: "\(request.method.rawValue) \(request.path)",
      include: request.query["include"],
      activate: context.activate)
  }

  static func holds(route: String, include: String?, activate: Bool) -> Bool {
    !activate && heldRoutes.contains(route) && include != "all-windows"
  }

  /// The refusal: the snapshot-unavailable envelope every route already
  /// answers with, under its own reason, so a client that knows the
  /// envelope reads it without learning anything new.
  public static func refusal(_ seen: AppNotInFront) -> HTTPResponse {
    TreeRoute.unavailable(reason: .inBackground, hint: hint(seen))
  }

  public static func hint(_ seen: AppNotInFront) -> String {
    let front: String
    switch seen.inFront.count {
    case 0: front = "SpringBoard shows no app in front (the home screen, or it could not tell)"
    case 1: front = "\(seen.inFront[0]) is in front"
    default: front = "\(seen.inFront.joined(separator: ", ")) are in front"
    }
    return "\(seen.bundleId) is in the background (\(seen.state)); \(front). "
      + "Bring it back with launchApp, or tap the notification that opens it — "
      + "a notification banner is found with the system windows, not the app's."
  }
}

import Foundation

// Which orientation a synthesised touch is stamped with, and where its
// point goes.
//
// `XCSynthesizedEventRecord` is built with an interface orientation,
// and that value decides which space the coordinates in its event paths
// are read against. The runner computes points against
// `XCUIApplication.frame` — the app's own space — and stamped every
// event `.portrait`, so an app laid out landscape sends touches that
// are read against a portrait screen and land nowhere near the element
// aimed at.
//
// Two repairs are possible: move the stamp to match the point, or move
// the point to match the stamp. Only one can be what the system
// honours, and reading the private API's header cannot say which —
// there is no header. Both are therefore buildable, selected at run
// time, and a device decides.
public enum EventStampStrategy: String, Sendable {
  /// What the runner did: `.portrait`, always.
  case legacyAlwaysPortrait
  /// Stamp follows the app's layout; the point stays in the app's space.
  case deriveFromAppFrame
  /// Stamp stays portrait; the point is rotated into the device's space.
  case convertPointToDeviceSpace

  /// Read once at startup.
  ///
  /// The default is what a simulator chose. Stamping the event with the
  /// app's orientation — the repair anybody would reach for first, and
  /// the one this comment used to describe as obvious — changed
  /// nothing: the counter stayed at 0 and the same handful of pixels
  /// moved as when nothing was fixed. Rotating the point into the
  /// device's space and leaving the stamp portrait is what lands. The
  /// system does not use that stamp to map coordinates.
  ///
  /// The variable stays because re-running the experiment on a new iOS
  /// is the only way to know the answer has not changed, and because
  /// setting it to `legacyAlwaysPortrait` is the one reachable way to
  /// produce the mismatch the driver's refusal exists to catch — a
  /// guard nothing can trigger is a guard nobody can trust.
  public static func fromEnvironment(
    _ env: [String: String] = ProcessInfo.processInfo.environment
  ) -> EventStampStrategy {
    // Both spellings. Xcode forwards `TEST_RUNNER_*` into the test
    // process with the prefix stripped, so the runner sees the bare
    // name — but a harness that sets only the bare name on the CLI
    // process would have it stop at xcodebuild, and the experiment
    // would silently measure the default three times.
    let raw = env["SMIX_EVENT_STAMP"] ?? env["TEST_RUNNER_SMIX_EVENT_STAMP"]
    return raw.flatMap(EventStampStrategy.init(rawValue:)) ?? .convertPointToDeviceSpace
  }
}

/// An interface orientation, named without UIKit.
///
/// This module's tests run on macOS, where `UIInterfaceOrientation`
/// does not exist. Depending on it here would have made the arithmetic
/// testable only on the platform where testing it is hardest — so the
/// core names the four cases and the UITest host, which is iOS by
/// definition, translates at the boundary.
public enum StampOrientation: String, Sendable {
  case portrait
  case portraitUpsideDown
  case landscapeLeft
  case landscapeRight
}

/// The orientation to stamp, given how the app is laid out.
///
/// `landscapeRight` rather than `landscapeLeft` for a wide frame: the
/// two differ by 180 degrees, and picking the wrong one lands every
/// touch in the opposite corner — a failure that looks like a working
/// fix on a centred target and fails on everything else.
public func eventStamp(
  forAppFrame size: CGSize, strategy: EventStampStrategy
) -> StampOrientation {
  switch strategy {
  case .legacyAlwaysPortrait, .convertPointToDeviceSpace:
    return .portrait
  case .deriveFromAppFrame:
    // Strictly wider: a square has no handedness, and a rule that
    // resolves the tie by which comparison happened to be typed is one
    // nobody can predict.
    return size.width > size.height ? .landscapeRight : .portrait
  }
}

/// A point in the app's coordinate space, expressed in the device's.
///
/// A rotation, not a swap. Swapping x and y mirrors the screen about
/// its diagonal: the centre still lands on the centre, so a smoke test
/// on one middling target passes and everything near an edge goes to
/// the wrong side.
public func pointInDeviceSpace(
  _ point: CGPoint, appFrame: CGSize, interface: StampOrientation
) -> CGPoint {
  switch interface {
  case .portrait:
    return point
  case .landscapeRight:
    // The app's +x runs down the device, its +y runs left across it.
    return CGPoint(x: appFrame.height - point.y, y: point.x)
  case .landscapeLeft:
    return CGPoint(x: point.y, y: appFrame.width - point.x)
  case .portraitUpsideDown:
    return CGPoint(x: appFrame.width - point.x, y: appFrame.height - point.y)
  }
}

import SmixRunnerCore
import XCTest

/// Turn the Digital Crown. Only an Apple Watch and an Apple Vision Pro have
/// one XCTest can turn; anything else says what it is.
func smixRotateCrown(_ req: CrownRoute.Request) -> CrownRoute.Outcome {
  #if os(watchOS) || os(visionOS)
  let turned = smixGuarded("rotate-crown") { () -> Bool in
    if let velocity = req.velocity {
      XCUIDevice.shared.rotateDigitalCrown(
        delta: CGFloat(req.rotations), velocity: XCUIGestureVelocity(rawValue: CGFloat(velocity)))
    } else {
      XCUIDevice.shared.rotateDigitalCrown(delta: CGFloat(req.rotations))
    }
    return true
  } ?? false
  return turned ? .rotated : .raised
  #elseif os(tvOS)
  return .noCrown("an Apple TV has no Digital Crown; only an Apple Watch or Apple Vision Pro has one")
  #else
  return .noCrown("an iPhone or iPad has no Digital Crown; only an Apple Watch or Apple Vision Pro has one")
  #endif
}

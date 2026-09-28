import Foundation
import Testing

@testable import SmixRunnerCore

// A wide frame is either landscape, and the two differ by 180 degrees:
// taken from the frame alone, a tap in one of them lands in the opposite
// corner of the screen.
@Suite struct LayoutOrientationTests {
  let wide = CGSize(width: 874, height: 402)
  let tall = CGSize(width: 402, height: 874)

  @Test func theAppsOwnOrientationDecides() {
    for o in [StampOrientation.landscapeLeft, .landscapeRight] {
      #expect(layoutOrientation(appFrame: wide, reported: o, device: nil) == o)
    }
    #expect(layoutOrientation(appFrame: tall, reported: .portraitUpsideDown, device: nil) == .portraitUpsideDown)
  }

  @Test func withoutItAWideFrameFollowsTheDevice() {
    #expect(layoutOrientation(appFrame: wide, reported: nil, device: .landscapeLeft) == .landscapeLeft)
    #expect(layoutOrientation(appFrame: wide, reported: nil, device: .landscapeRight) == .landscapeRight)
  }

  @Test func aDeviceThatSaysNothingAboutLandscapeKeepsTheOldReading() {
    // An app that only supports one landscape stays in it while the device
    // is held upright; that is the case the old reading was measured on.
    #expect(layoutOrientation(appFrame: wide, reported: nil, device: .portrait) == .landscapeRight)
    #expect(layoutOrientation(appFrame: wide, reported: nil, device: nil) == .landscapeRight)
    #expect(layoutOrientation(appFrame: tall, reported: nil, device: .landscapeLeft) == .portrait)
  }

  @Test func landscapeLeftIsTheLandscapeRightMirror() {
    // The consumer's case, measured on iOS 27: aimed at (214,56) in a
    // 874x402 app turned landscapeLeft, the touch arrived at (660,346).
    let p = CGPoint(x: 214, y: 56)
    let left = pointInDeviceSpace(p, appFrame: wide, interface: .landscapeLeft)
    let right = pointInDeviceSpace(p, appFrame: wide, interface: .landscapeRight)
    #expect(left != right)
    #expect(left.x + right.x == wide.height)
    #expect(left.y + right.y == wide.width)
  }
}

import SwiftUI
import UIKit

/// The screen a launch argument opens instead of the main one, each its own
/// way in, so no row is added to the main list the corpus flows scroll. The
/// first argument listed here that was passed wins; nil opens the main screen.
func screenAsked(by arguments: [String]) -> UIViewController? {
  let screens: [(String, () -> UIViewController)] = [
    ("-tall-cards", { UIHostingController(rootView: TallCardsView()) }),
    ("-uikit-rows", { UIKitRowsController() }),
    ("-uikit-signin", { UIKitSignInController(tapsAway: false) }),
    ("-uikit-signin-taps-away", { UIKitSignInController(tapsAway: true) }),
    ("-pinch", { PinchController() }),
    ("-zoom", { ZoomController() }),
    ("-blink", { BlinkController() }),
    ("-live", { LiveController() }),
    ("-playing", { PlayingController() }),
    ("-push", { PushController() }),
    ("-profile", { ProfileController() }),
    ("-arrive", { ArriveController() }),
    ("-toast", { ToastController() }),
    ("-sheet-over-form", { UIHostingController(rootView: SheetOverFormView()) }),
  ]
  return screens.first { arguments.contains($0.0) }.map { $0.1() }
}

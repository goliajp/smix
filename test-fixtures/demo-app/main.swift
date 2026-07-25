// The app smix's own end-to-end tests drive.
//
// It is a test asset, not an example: the repo deliberately does not ship
// a runnable sample app, and this one exists so the standalone loop can
// exercise `sim install` — which drives nothing if the tests only ever
// point at an app Apple already put on the device.
//
// Three identifiers, chosen to cover one of each thing smix does: a field
// to type into, a control to act on, and a label to assert against.
import SwiftUI
import UIKit

struct ContentView: View {
  @State private var typed = ""
  @State private var submitted = ""

  var body: some View {
    VStack(spacing: 24) {
      Text("smix fixture").font(.headline)

      TextField("type here", text: $typed)
        .textFieldStyle(.roundedBorder)
        .accessibilityIdentifier("fixture-input")
        .padding(.horizontal, 32)

      Button("Submit") { submitted = typed }
        .accessibilityIdentifier("fixture-submit")

      // Empty until Submit is pressed, so an assertion on it distinguishes
      // "the tap landed" from "the field merely holds text".
      Text(submitted.isEmpty ? "nothing submitted" : submitted)
        .accessibilityIdentifier("fixture-result")
    }
    .padding()
  }
}

final class AppDelegate: NSObject, UIApplicationDelegate {
  var window: UIWindow?

  func application(
    _ application: UIApplication,
    didFinishLaunchingWithOptions options: [UIApplication.LaunchOptionsKey: Any]? = nil
  ) -> Bool {
    let window = UIWindow(frame: UIScreen.main.bounds)
    window.rootViewController = UIHostingController(rootView: ContentView())
    window.makeKeyAndVisible()
    self.window = window
    return true
  }
}

// UIApplicationMain rather than SwiftUI's @main: this is compiled as a
// loose file with swiftc, not as a target with a generated entry point.
UIApplicationMain(
  CommandLine.argc,
  CommandLine.unsafeArgv,
  nil,
  NSStringFromClass(AppDelegate.self)
)

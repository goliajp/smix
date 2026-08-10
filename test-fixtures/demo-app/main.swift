// The app smix's own end-to-end tests drive.
//
// It is a test asset, not an example: the repo deliberately does not ship
// a runnable sample app, and this one exists so the standalone loop can
// exercise `sim install` — which drives nothing if the tests only ever
// point at an app Apple already put on the device.
//
// Two screens. The first has a field to type into, a control to act on
// and a label to assert against; below them a list long enough that its
// far rows are off screen at launch, a row to long-press, and a link
// into a detail screen reached through a NavigationStack.
//
// The list and the detail screen exist because twenty of the
// twenty-one corpus flows drive the system Settings app, whose row
// identifiers change with the iOS version and the device model. Those
// flows cannot run on a CI machine and mean anything. The portable
// counterparts drive this instead — same structure, a subject that
// travels.
import SwiftUI
import UIKit

// Forty rows, not four.
//
// A scroll flow whose target is already visible passes on a device that
// never scrolled — it looks like coverage and is not. Forty is well past
// a screenful on every simulator size, so `fixture-row-39` is reachable
// only by scrolling, and stays that way if the fixture is later run on a
// larger device.
let fixtureRowCount = 40

struct DetailView: View {
  var body: some View {
    VStack(spacing: 16) {
      Text("Detail").font(.headline)
      // Something on the destination to assert. Without it, arriving and
      // not arriving look identical from a flow's point of view.
      Text("you are on the detail screen")
        .accessibilityIdentifier("fixture-detail")
    }
    .navigationTitle("Detail")
  }
}

struct ContentView: View {
  @State private var typed = ""
  @State private var submitted = ""
  @State private var longPressed = false

  var body: some View {
    // NavigationStack, and the back button it provides, rather than a
    // bespoke close control: the Settings flows come back through the
    // navigation bar's button, and a counterpart that used something
    // else would exercise a different path and answer a different
    // question.
    NavigationStack {
      List {
        Section {
          Text("smix fixture").font(.headline)

          TextField("type here", text: $typed)
            .textFieldStyle(.roundedBorder)
            .accessibilityIdentifier("fixture-input")

          Button("Submit") { submitted = typed }
            .accessibilityIdentifier("fixture-submit")

          // Empty until Submit is pressed, so an assertion on it
          // distinguishes "the tap landed" from "the field merely holds
          // text".
          Text(submitted.isEmpty ? "nothing submitted" : submitted)
            .accessibilityIdentifier("fixture-result")

          NavigationLink("Open detail") { DetailView() }
            .accessibilityIdentifier("fixture-detail-link")

          // Its label changes on the gesture, so the assertion is about
          // the long press having happened rather than about the row
          // still existing.
          Text(longPressed ? "long pressed" : "hold me")
            .accessibilityIdentifier("fixture-longpress")
            .onLongPressGesture { longPressed = true }
        }

        Section {
          ForEach(0..<fixtureRowCount, id: \.self) { i in
            Text("Row \(i)")
              .accessibilityIdentifier("fixture-row-\(i)")
          }
        }
      }
      .navigationTitle("smix fixture")
    }
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

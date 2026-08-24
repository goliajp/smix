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

// A landscape-only screen, shaped like the one a consumer could not tap.
//
// Theirs is a `.fullScreen` controller with
// `supportedInterfaceOrientations = .landscapeRight`, and every touch
// into it — by identifier and by coordinate, six rotation mappings —
// left the screen byte-identical while smix reported the tap landing
// inside the button aimed at.
//
// Two targets, deliberately: `landscape-increment` is wide enough that
// missing it cannot be blamed on precision, and `landscape-exit` is
// 44×40 in the top-left corner, the size and place of the button they
// were actually after. If only the small one misses, that is a
// different fault from both missing.
final class LandscapeCounter: ObservableObject {
  @Published var value = 0
}

// One view, two orientations, so the portrait run is a control rather
// than a different experiment. Same hierarchy, same identifiers modulo
// the prefix, same presentation path — the only difference between the
// two is the orientation mask on the controller. If portrait moves
// pixels and landscape does not, orientation is the only thing it can
// be.
struct CounterView: View {
  let prefix: String
  @ObservedObject var counter: LandscapeCounter
  let onExit: () -> Void

  var body: some View {
    ZStack(alignment: .topLeading) {
      Color(white: 0.93).ignoresSafeArea()

      VStack(spacing: 24) {
        // The subject of the pixel comparison: its glyphs change on a
        // tap that arrives, and nothing else on this screen moves on
        // its own — no animation, no clock, no spinner — so a diff of
        // before and after has exactly one thing it can be reporting.
        Text("\(counter.value)")
          .font(.system(size: 96, weight: .bold, design: .monospaced))
          .accessibilityIdentifier("\(prefix)-counter")

        Button(action: { counter.value += 1 }) {
          Text("increment")
            .font(.title2)
            .frame(width: 280, height: 96)
            .background(Color.blue.opacity(0.25))
        }
        .accessibilityIdentifier("\(prefix)-increment")
      }
      .frame(maxWidth: .infinity, maxHeight: .infinity)

      Button(action: onExit) {
        Text("×")
          .font(.title3)
          .frame(width: 44, height: 40)
          .background(Color.red.opacity(0.3))
      }
      .accessibilityIdentifier("\(prefix)-exit")
      .padding(.leading, 64)
      .padding(.top, 4)
    }
  }
}

final class LandscapeHost: UIHostingController<CounterView> {
  override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .landscapeRight }
  override var shouldAutorotate: Bool { true }
}

final class PortraitHost: UIHostingController<CounterView> {
  override var supportedInterfaceOrientations: UIInterfaceOrientationMask { .portrait }
  override var shouldAutorotate: Bool { true }
}

enum LandscapeStage {
  static let counter = LandscapeCounter()
  static weak var presenter: UIViewController?

  static func present(landscape: Bool) {
    guard let root = presenter else { return }
    counter.value = 0
    let prefix = landscape ? "landscape" : "portrait"
    let view = CounterView(
      prefix: prefix, counter: counter, onExit: { root.dismiss(animated: false) })
    let host: UIViewController =
      landscape ? LandscapeHost(rootView: view) : PortraitHost(rootView: view)
    host.modalPresentationStyle = .fullScreen
    // Unanimated on purpose: a screenshot taken during a presentation
    // transition differs from the one before it for reasons that have
    // nothing to do with whether a touch arrived.
    root.present(host, animated: false)
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
      // Named so a swipe can be aimed inside it. `swipe: { over: ... }`
      // takes shares of an element's box, and a box needs an element
      // that can be addressed — which is the whole difference between
      // that form and measuring the screen.
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

          Button("Open landscape") { LandscapeStage.present(landscape: true) }
            .accessibilityIdentifier("landscape-enter")

          Button("Open portrait counter") { LandscapeStage.present(landscape: false) }
            .accessibilityIdentifier("portrait-enter")

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
      .accessibilityIdentifier("fixture-list")
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
    let root = UIHostingController(rootView: ContentView())
    window.rootViewController = root
    window.makeKeyAndVisible()
    self.window = window
    LandscapeStage.presenter = root
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

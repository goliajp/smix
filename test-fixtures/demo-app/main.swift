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
  // Whether the anchor below has been pushed down. `rememberBounds` /
  // `assertBoundsUnchanged` need a control that can be made to move by a
  // known amount and one press that moves nothing, so "moved" and "did
  // not move" are both there to be judged. Here rather than on the main
  // list, whose rows other checks measure.
  @State private var anchorShifted = false

  var body: some View {
    VStack(spacing: 16) {
      Text("Detail").font(.headline)
      // Something on the destination to assert. Without it, arriving and
      // not arriving look identical from a flow's point of view.
      Text("you are on the detail screen")
        .accessibilityIdentifier("fixture-detail")
      // A spacer above it rather than padding on it: padding belongs to
      // the element's own frame, so the frame would grow while its corner
      // stayed put. Both sit in a fixed-height, top-aligned box: the outer
      // stack is centred on the screen, and a spacer growing inside it
      // would push everything above up by half and the anchor down by the
      // other half — measured, 4 points instead of 8. In the box the
      // anchor, and only the anchor, moves by exactly the 8 points a
      // consumer's tour buttons were built not to cause.
      VStack(spacing: 0) {
        Color.clear.frame(height: anchorShifted ? 8 : 0)
        Text("anchor")
          .accessibilityIdentifier("fixture-bounds-target")
      }
      .frame(height: 44, alignment: .top)
      HStack {
        Button("Move") { anchorShifted = true }
          .accessibilityIdentifier("fixture-bounds-move")
        Button("Stay") {}
          .accessibilityIdentifier("fixture-bounds-stay")
      }
      // A screen of its own rather than more rows here or on the main
      // list, whose positions other checks measure.
      NavigationLink("Open watch") { WatchView() }
        .accessibilityIdentifier("fixture-watch-link")
    }
    .navigationTitle("Detail")
  }
}

// The subjects `neverVisible` and a disappearing control are judged on.
//
// `Flash` does what a consumer's alert row does when it opens a
// recording: the press returns, and a moment later a loading overlay
// stands over the screen for 400 ms and goes. `Quiet` takes the same time
// and shows nothing — the control that says a watch which never saw
// anything was not simply blind. Both end by showing `done`, which is
// what a flow waits for.
//
// `Reveal` shows a button that takes itself away after three seconds, as
// a player's controls do after the picture is touched. It counts the
// presses it received and how long after it appeared the press came, on
// the app's own clock — the reading a check trusts.
struct WatchView: View {
  @State private var overlay = false
  @State private var done = false
  @State private var vanishing = false
  @State private var shownAt = Date()
  @State private var presses = 0
  @State private var latency = -1

  var body: some View {
    ZStack {
      VStack(spacing: 16) {
        Text("watch")
        Button("Flash") {
          DispatchQueue.main.asyncAfter(deadline: .now() + 0.3) {
            overlay = true
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.4) {
              overlay = false
              done = true
            }
          }
        }
        .accessibilityIdentifier("watch-flash")
        Button("Quiet") {
          DispatchQueue.main.asyncAfter(deadline: .now() + 0.7) { done = true }
        }
        .accessibilityIdentifier("watch-quiet")
        if done {
          Text("done").accessibilityIdentifier("watch-done")
        }
        Button("Reveal") {
          vanishing = true
          shownAt = Date()
          DispatchQueue.main.asyncAfter(deadline: .now() + 3) { vanishing = false }
        }
        .accessibilityIdentifier("watch-reveal")
        if vanishing {
          Button("Tap me") {
            presses += 1
            latency = Int(Date().timeIntervalSince(shownAt) * 1000)
          }
          .accessibilityIdentifier("watch-vanishing")
        }
        Text("presses \(presses)").accessibilityIdentifier("watch-presses")
        Text("latency \(latency)").accessibilityIdentifier("watch-latency")
      }
      if overlay {
        Color.black.opacity(0.4)
          .ignoresSafeArea()
          .overlay(Text("loading").foregroundColor(.white).accessibilityIdentifier("watch-overlay"))
      }
    }
    .navigationTitle("Watch")
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

// A UIKit alert, the kind React Native's `Alert.alert` raises on iOS —
// not SwiftUI's `.alert`, which the alert above is. A consumer's confirm
// of exactly this kind was reported as tapped and not pressed; the
// wording, the button styles and their order are theirs, so a run here
// asks the same question their flow does.
final class UIKitAlertStage: ObservableObject {
  static let shared = UIKitAlertStage()
  /// Presses `Delete` received — the app's own reading.
  @Published var deleted = 0

  static let message =
    "You are about to remove all credentials from this device. This action cannot be undone."

  func present() {
    guard let root = LandscapeStage.presenter else { return }
    let alert = UIAlertController(
      title: "Clear cache", message: Self.message, preferredStyle: .alert)
    alert.addAction(UIAlertAction(title: "Cancel", style: .cancel))
    alert.addAction(
      UIAlertAction(title: "Delete", style: .destructive) { _ in self.deleted += 1 })
    root.present(alert, animated: true)
  }
}

struct ContentView: View {
  @State private var typed = ""
  @State private var submitted = ""
  @State private var longPressed = false
  // Counts, not flags: a gesture delivered twice and one delivered once
  // read the same as a flag, and a double tap that arrived as two single
  // taps must not count at all.
  @State private var longPresses = 0
  @State private var doubleTaps = 0
  @ObservedObject private var uikitAlert = UIKitAlertStage.shared
  // A modal, to answer the question Compose answered badly: does SwiftUI
  // keep an identifier on a control inside an alert, or does the modal
  // cost it the way `testTagsAsResourceId` does on Android?
  @State private var alertShown = false
  // What the alert's confirm has actually received. A confirmed alert and
  // a dismissed one are both gone, so "the alert went away" says nothing;
  // this count is the app's own reading of whether the button was pressed.
  @State private var alertConfirmed = 0
  // Presses the icon-only button received.
  @State private var iconPauses = 0

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

          // One row with the alert above, so no row below moves.
          HStack {
            Button("Open alert") { alertShown = true }
              .buttonStyle(.borderless)
              .accessibilityIdentifier("fixture-open-alert")
            Spacer()
            Button("UIKit alert") { UIKitAlertStage.shared.present() }
              .buttonStyle(.borderless)
              .accessibilityIdentifier("fixture-open-uikit-alert")
            Text("deleted \(uikitAlert.deleted)")
              .accessibilityIdentifier("fixture-uikit-alert-count")
          }

          // Empty until Submit is pressed, so an assertion on it
          // distinguishes "the tap landed" from "the field merely holds
          // text".
          Text(submitted.isEmpty ? "nothing submitted" : submitted)
            .accessibilityIdentifier("fixture-result")
            .alert("An alert", isPresented: $alertShown) {
              Button("Delete", role: .destructive) {
                alertConfirmed += 1
                alertShown = false
              }
                .accessibilityIdentifier("fixture-alert-confirm")
              Button("Cancel", role: .cancel) { alertShown = false }
                .accessibilityIdentifier("fixture-alert-cancel")
            } message: {
              Text("hosted by the system")
            }

          // Same row as the count above rather than a row of its own: the
          // list's row positions are the subject of a scroll check, and one
          // more row moves where the screen's bottom edge falls.
          HStack {
            Text("confirmed \(alertConfirmed)")
              .accessibilityIdentifier("fixture-alert-count")
            Spacer()
            // A control with no words and no identifier: its only name is
            // the accessibility label, as an icon-only button's is. Counts
            // the presses it received, which is what a check reads.
            Text("pauses \(iconPauses)")
              .accessibilityIdentifier("fixture-icon-count")
            Button { iconPauses += 1 } label: { Image(systemName: "pause.fill") }
              .buttonStyle(.borderless)
              .accessibilityLabel("Pause")
          }

          NavigationLink("Open detail") { DetailView() }
            .accessibilityIdentifier("fixture-detail-link")

          Button("Open landscape") { LandscapeStage.present(landscape: true) }
            .accessibilityIdentifier("landscape-enter")

          Button("Open portrait counter") { LandscapeStage.present(landscape: false) }
            .accessibilityIdentifier("portrait-enter")

          // Its label changes on the gesture, so the assertion is about
          // the long press having happened rather than about the row
          // still existing.
          HStack {
            Text(longPressed ? "long pressed" : "hold me")
              .accessibilityIdentifier("fixture-longpress")
              .onLongPressGesture {
                longPressed = true
                longPresses += 1
              }
            Text("held \(longPresses)")
              .accessibilityIdentifier("fixture-longpress-count")
            Spacer()
            // Its own count is its label, so the check reads the target.
            Text("double taps \(doubleTaps)")
              .padding(8)
              .background(Color.green.opacity(0.2))
              .accessibilityIdentifier("fixture-doubletap")
              .onTapGesture(count: 2) { doubleTaps += 1 }
          }
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

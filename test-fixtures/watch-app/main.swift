import SwiftUI

/// An Apple Watch screen to drive: a button that counts its presses
/// (`watch-count`: "pressed 2") above a list of thirty rows, long enough
/// that the last of them is only reached by scrolling or the crown.
struct FixtureView: View {
  @State private var presses = 0

  var body: some View {
    List {
      Button("Press") { presses += 1 }
        .accessibilityIdentifier("watch-press")
      Text("pressed \(presses)")
        .accessibilityIdentifier("watch-count")
      ForEach(1...30, id: \.self) { n in
        Text("Row \(n)").accessibilityIdentifier("watch-row-\(n)")
      }
    }
  }
}

@main
struct FixtureApp: App {
  var body: some Scene {
    WindowGroup { FixtureView() }
  }
}

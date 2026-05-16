import SwiftUI

// Host app target for the UITest bundle; intentionally minimal.
// The actual HTTP server runs inside the XCUITest runner process, not this app.
@main
struct SimxRunnerApp: App {
  var body: some Scene {
    WindowGroup {
      Text("simx runner host")
    }
  }
}

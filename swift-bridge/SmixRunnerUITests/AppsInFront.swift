import SmixRunnerCore
import XCTest

/// `target` as the foreground gate sees it: `nil` unless it is running in
/// the background. Reading `state` asks for the process, not for its
/// window, so it answers at once where an element query would wait 30 s.
@MainActor
func appNotInFront(_ target: XCUIApplication, bundleId: String) -> AppNotInFront? {
  let state: String
  switch target.state {
  case .runningBackground: state = "running-background"
  case .runningBackgroundSuspended: state = "running-background-suspended"
  case .runningForeground, .notRunning, .unknown: return nil
  @unknown default: return nil
  }
  return AppNotInFront(bundleId: bundleId, state: state, inFront: appsInFront())
}

/// The apps SpringBoard shows in front. Its switcher window holds a card
/// per scene on screen, named `card:<bundle id>:sceneID:<scene>` — the
/// same on iOS 26.5 and 27. Empty on the home screen.
@MainActor
private func appsInFront() -> [String] {
  let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
  let cards = springboard.descendants(matching: .any)
    .matching(NSPredicate(format: "identifier BEGINSWITH 'card:'"))
    .allElementsBoundByIndex
  var seen: [String] = []
  for card in cards {
    let parts = card.identifier.split(separator: ":")
    if parts.count > 1, !seen.contains(String(parts[1])) { seen.append(String(parts[1])) }
  }
  return seen
}

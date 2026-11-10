import SmixRunnerCore
import XCTest

// What differs where the runner drives a headset (visionOS) rather than a
// phone or tablet: no device orientation, a window that is the shape it
// is, taps that event synthesis cannot place, and no SpringBoard.

/// The device's orientation as a touch stamp reads it; nil on a headset.
func smixDeviceStamp() -> StampOrientation? {
  #if os(iOS)
    switch XCUIDevice.shared.orientation {
    case .portrait: return .portrait
    case .portraitUpsideDown: return .portraitUpsideDown
    // The device turned left shows the interface turned right.
    case .landscapeLeft: return .landscapeRight
    case .landscapeRight: return .landscapeLeft
    default: return nil
    }
  #else
    return nil
  #endif
}

/// The device's orientation now; `.unknown` on a headset.
func smixDeviceOrientationNow() -> UIDeviceOrientation {
  #if os(iOS)
    return XCUIDevice.shared.orientation
  #else
    return .unknown
  #endif
}

/// Turn the device; false, said on stderr, where there is no orientation.
func smixSetDeviceOrientation(_ orientation: UIDeviceOrientation) -> Bool {
  #if os(iOS)
    XCUIDevice.shared.orientation = orientation
    // settle: orientation change is asynchronous in sim;
    // 200ms aligns with iOS rotation animation budget.
    Thread.sleep(forTimeInterval: 0.2)
    return true
  #else
    FileHandle.standardError.write(
      Data("smix-runner: set-orientation: this platform has no device orientation\n".utf8))
    return false
  #endif
}

/// Whether the app says it is laid out landscape. Only the app's own
/// interface orientation is read: unlike `XCUIDevice.orientation`, it may
/// be asked off the main thread, where the tree is built.
///
/// Nil where the platform has no interface orientation (visionOS): a
/// window there is the shape it is, and turning a 1280×800 one into
/// 800×1280 put every element right of x=800 outside the app.
func smixInterfaceIsLandscape(_ app: XCUIApplication) -> Bool? {
  #if os(iOS)
    guard app.responds(to: NSSelectorFromString("interfaceOrientation")),
      let raw = app.value(forKey: "interfaceOrientation") as? Int,
      let o = UIInterfaceOrientation(rawValue: raw)
    else { return false }
    return o.isLandscape
  #else
    return nil
  #endif
}

/// `smixShaped` that keeps the frame as it is when no orientation is known.
func smixShaped(_ frame: CGRect, landscape: Bool?) -> CGRect {
  guard let landscape else { return frame }
  return smixShaped(frame, landscape: landscape)
}

/// A tap on a headset. Event synthesis asks Accessibility for the scene
/// the point is in and a visionOS window gives none (XCTDaemon error 74,
/// "invalid scene ID (nil)"); XCUI's coordinate tap is accepted there and
/// lands on nothing (visionOS 27.2: no control under the point reacted).
/// So the element under the point is tapped by its identifier, the host's
/// aim first, and only a point with no identified element is tapped as one.
func smixHeadsetTap(
  _ app: XCUIApplication, nx: Double, ny: Double, times: Int, holdMs: Int,
  aimedAt: TapAtCoordRoute.AimTarget?, chain: [HitChainEntry]
) -> Bool {
  let id = [aimedAt?.identifier].compactMap { $0 }.first(where: { !$0.isEmpty })
    ?? chain.first(where: { !$0.identifier.isEmpty })?.identifier
  return smixGuarded("tap-at-norm-coord-xcui") { () -> Bool in
    let hold = Double(holdMs) / 1000
    if let id {
      let element = app.descendants(matching: .any).matching(identifier: id).firstMatch
      guard element.exists else { return false }
      #if os(tvOS)
      // no press on a television: each is a focus-then-select
      for _ in 0..<max(1, times) { guard smixActivate(element) else { return false } }
      #else
      for _ in 0..<max(1, times) { if hold > 0 { element.press(forDuration: hold) } else { element.tap() } }
      #endif
    } else {
      #if os(tvOS)
      // a television takes no touch at a point; only the focused element acts
      return false
      #else
      let point = app.coordinate(withNormalizedOffset: CGVector(dx: nx, dy: ny))
      for _ in 0..<max(1, times) { if hold > 0 { point.press(forDuration: hold) } else { point.tap() } }
      #endif
    }
    return true
  } == true
}

/// The process that shows system alerts over the app. SpringBoard on a
/// phone or tablet; a headset has none, and its system alerts are hosted
/// by one of these, the first that is running with an alert up.
private let headsetSystemUI = [
  "com.apple.RealityChrome", "com.apple.RealityHUD", "com.apple.RealityNotifications",
  "com.apple.RealityLauncher",
]

func smixSystemUIBundle() -> String {
  #if os(iOS)
    return "com.apple.springboard"
  #else
    let running = headsetSystemUI.filter {
      XCUIApplication(bundleIdentifier: $0).state != .notRunning
    }
    let withAlert = running.first {
      XCUIApplication(bundleIdentifier: $0).alerts.count > 0
    }
    return withAlert ?? running.first ?? headsetSystemUI[0]
  #endif
}

func smixSystemUI() -> XCUIApplication {
  XCUIApplication(bundleIdentifier: smixSystemUIBundle())
}

/// A headset's system alerts — a permission prompt among them — are not
/// alert elements: RealityNotifications hosts each under an element named
/// `….SystemAlert` (or `CriticalSystemAlert`; an "other", not a window),
/// its title and message as texts identified `title` / `message`, its
/// buttons as buttons. One with no buttons is nothing up.
private func headsetAlertWindows() -> [XCUIElement] {
  let host = XCUIApplication(bundleIdentifier: "com.apple.RealityNotifications")
  guard host.state != .notRunning else { return [] }
  return host.descendants(matching: .any)
    .matching(NSPredicate(format: "identifier ENDSWITH %@", "SystemAlert"))
    .allElementsBoundByAccessibilityElement
    .filter { $0.buttons.count > 0 }
}

/// The headset's system alerts, in the shape the phone's are reported.
/// Button ids are `b-<index>`, as on the phone when a button has none of
/// its own (here every one is identified `alert`).
func smixHeadsetSystemAlerts() -> [SystemPopupsRoute.Popup] {
  headsetAlertWindows().map { w in
    let attr = { (word: String) in
      Set(w.buttons.matching(NSPredicate(format: "userTestingAttributes CONTAINS %@", word))
        .allElementsBoundByAccessibilityElement.map(\.label))
    }
    let cancel = attr("cancel-button")
    let destructive = attr("destructive")
    let buttons = w.buttons.allElementsBoundByAccessibilityElement.enumerated().map { i, b in
      SystemPopupsRoute.PopupButton(
        id: "b-\(i)", label: b.label,
        role: cancel.contains(b.label) ? "cancel" : destructive.contains(b.label) ? "destructive" : "default",
        dangerous: destructive.contains(b.label), outcomeHint: nil)
    }
    return SystemPopupsRoute.Popup(
      id: w.identifier, type: "alert", source: "com.apple.RealityNotifications",
      title: w.staticTexts["title"].firstMatch.label, body: w.staticTexts["message"].firstMatch.label,
      buttons: buttons)
  }
}

/// Press a headset system alert's button by element: synthesis cannot
/// place a touch on visionOS. Nil when no headset alert has that id.
func smixHeadsetSystemAlertAction(
  popupId: String, buttonId: String
) -> SmixRunnerServer.SystemPopupActionOutcome? {
  guard let w = headsetAlertWindows().first(where: { $0.identifier == popupId }) else { return nil }
  let buttons = w.buttons.allElementsBoundByAccessibilityElement
  guard buttonId.hasPrefix("b-"), let i = Int(buttonId.dropFirst(2)), buttons.indices.contains(i)
  else { return .notFound }
  smixActivate(buttons[i])
  return .found
}

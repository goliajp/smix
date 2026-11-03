import SmixRunnerCore
import XCTest

// SpringBoard's notification banners, reported as system popups so a flow
// finds and presses one the way it presses a permission prompt: by the
// words on it. A banner is not an alert or a sheet, so the popup walk did
// not see it; it is `NotificationShortLookView` in SpringBoard's tree, the
// same on iOS 26.5 and 27, with its title under `NotificationTitle` and its
// body under `TextContent.Primary`. It shows about seven seconds.
//
// Its one button is named by the notification's title — what a flow writes
// in `tapOn` — and pressing it opens the notification, as a finger does.

private let bannerId = "NotificationShortLookView"
private let bannerPrefix = "notification-"

private func springboardBanners() -> [XCUIElement] {
  XCUIApplication(bundleIdentifier: "com.apple.springboard")
    .descendants(matching: .any).matching(identifier: bannerId)
    .allElementsBoundByAccessibilityElement
}

/// Every banner on screen as a popup of type `notification`.
func collectBanners() -> [SystemPopupsRoute.Popup] {
  var out: [SystemPopupsRoute.Popup] = []
  for (i, el) in springboardBanners().enumerated() {
    guard let snap = try? el.snapshot() else { continue }
    let title = labelOf(snap, identifier: "NotificationTitle") ?? ""
    let body = labelOf(snap, identifier: "TextContent.Primary") ?? ""
    out.append(
      SystemPopupsRoute.Popup(
        id: "\(bannerPrefix)\(i)", type: "notification", source: "com.apple.springboard",
        title: title, body: body,
        buttons: [
          SystemPopupsRoute.PopupButton(
            id: "open", label: title, role: "default", dangerous: false, outcomeHint: nil)
        ]))
  }
  return out
}

/// Press banner `popupId` (`notification-N`), or `nil` when the id is not
/// a banner's and the ordinary popup walk should answer.
func tapBanner(popupId: String) -> SmixRunnerServer.SystemPopupActionOutcome? {
  guard popupId.hasPrefix(bannerPrefix), let i = Int(popupId.dropFirst(bannerPrefix.count)) else {
    return nil
  }
  let banners = springboardBanners()
  guard i < banners.count, let snap = try? banners[i].snapshot() else { return .notFound }
  // laid out by SpringBoard, whose frame is the screen: the app under test
  // may be in the background, and reading its frame then waits 30 s
  let springboard = XCUIApplication(bundleIdentifier: "com.apple.springboard")
  return tapThroughDaemon(at: CGPoint(x: snap.frame.midX, y: snap.frame.midY), layoutOf: springboard)
}

/// A touch at `center`, in the points of `layoutOf`'s frame, synthesized
/// the way a finger's is, so SpringBoard's handlers and an app's both fire.
func tapThroughDaemon(
  at center: CGPoint, layoutOf: XCUIApplication
) -> SmixRunnerServer.SystemPopupActionOutcome {
  let delivery = smixDelivery(of: center, layout: smixLayout(of: layoutOf))
  guard let record = SmixEventRecord(orientation: delivery.orientation, displayID: delivery.displayID),
        record.addPointerTouchEvent(at: delivery.point) else {
    return .notFound
  }
  let sema = DispatchSemaphore(value: 0)
  nonisolated(unsafe) var ok = false
  Task {
    do {
      try await SmixRunnerDaemonProxy.shared.synthesize(record: record)
      ok = true
    } catch {
      FileHandle.standardError.write(Data("smix-runner: system-popup-action daemonProxy error: \(error)\n".utf8))
    }
    sema.signal()
  }
  sema.wait()
  return ok ? .found : .notFound
}

private func labelOf(_ snap: XCUIElementSnapshot, identifier: String) -> String? {
  if snap.identifier == identifier { return snap.label }
  for c in snap.children {
    if let l = labelOf(c, identifier: identifier) { return l }
  }
  return nil
}

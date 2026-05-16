import XCTest
import SimxRunnerCore

// Long-running XCTestCase that hosts the in-simulator HTTP server.
// Pattern: Maestro re-building-the-ios-driver.
// The test method `test_runForever` intentionally never returns — `runForever()`
// blocks on FlyingFox `server.run()` until xcodebuild cancels the runner.
final class SimxRunnerUITests: XCTestCase {
  func test_runForever() async throws {
    // C2 target app: Settings (Calculator not preinstalled on iOS 26 sim runtime).
    // Force English locale so label-based selectors are portable across host locales.
    let bundleId = "com.apple.Preferences"
    let app = XCUIApplication(bundleIdentifier: bundleId)
    app.launchArguments = ["-AppleLanguages", "(en)", "-AppleLocale", "en_US"]
    app.launch()

    let server = SimxRunnerServer()
    try await server.runForever(
      port: 22087,
      tapHandler: { req in
        // C2 selector subset: text → match by label OR identifier.
        // Settings rows are XCUIElementTypeCell, not button; using `.any` covers both.
        let predicate = NSPredicate(
          format: "label == %@ OR identifier == %@",
          req.selector.text, req.selector.text
        )
        let query = app.descendants(matching: .any).matching(predicate)
        let element = query.firstMatch
        guard element.waitForExistence(timeout: 3) else { return .notFound }
        guard element.isHittable else { return .notFound }
        element.tap()
        let label = element.label
        return .matched(label: label.isEmpty ? req.selector.text : label)
      },
      snapshotHandler: {
        // v0.3 C1 — XCUIElement.snapshot() is a throwing, blocking call
        // (~50-100 ms on Settings). Returning nil here causes the server to
        // respond `500 {"ok":false,"error":"snapshot_unavailable"}` — used
        // when the target app has terminated mid-test.
        guard let snap = try? app.snapshot() else { return nil }
        // `XCUIElementSnapshot.identifier` on the application root is
        // typically empty (XCUITest only auto-populates identifier for
        // accessibility-identified subviews). C1 surfaces the bundle id at
        // the root by overriding the converted POCO's identifier when the
        // snapshot doesn't carry one — keeps `TreeRoute.serialize` purely
        // mechanical and host-side smoke gates can assert on
        // `.identifier == "com.apple.Preferences"`.
        let root = convertSnapshot(snap, rootIdentifierOverride: bundleId)
        return (root: root, appFrame: app.frame)
      }
    )
  }
}

/// Bridge XCUIElementSnapshot (XCUI / XCTest type) → A11ySnapshotData POCO
/// (SimxRunnerCore type, no XCUI dependency). Maintains the invariant that
/// SimxRunnerCore never imports XCTest/XCUI.
///
/// `rootIdentifierOverride` is applied only at the top level call (children
/// recurse with nil) and only when the snapshot's own identifier is empty.
/// This compensates for `XCUIApplication.snapshot()` returning a root with
/// an empty identifier even though the caller knows the bundle id.
private func convertSnapshot(
  _ s: XCUIElementSnapshot,
  rootIdentifierOverride: String? = nil
) -> TreeRoute.A11ySnapshotData {
  // s.value is Any?; C1 captures only the String? subset. Future
  // checkpoints may extend to Bool / Int / Float when AX paths expose them.
  let valueStr: String? = (s.value as? String).flatMap { $0.isEmpty ? nil : $0 }
  let kids = s.children.map { convertSnapshot($0) }
  let identifier: String = {
    if !s.identifier.isEmpty { return s.identifier }
    return rootIdentifierOverride ?? ""
  }()
  return TreeRoute.A11ySnapshotData(
    elementTypeRawValue: UInt(s.elementType.rawValue),
    identifier: identifier,
    label: s.label,
    value: valueStr,
    frame: s.frame,
    isEnabled: s.isEnabled,
    isSelected: s.isSelected,
    children: kids
  )
}

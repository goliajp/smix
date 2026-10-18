import Foundation
import SmixRunnerCore
import XCTest

// POST /hide-keyboard. Idempotent — keyboard already absent is a no-op
// success. Typical use: an explicit `hideKeyboard` step between a
// text-input fill and a subsequent tap, when the on-screen keyboard would
// otherwise mask the next target.
func smixHideKeyboard(app: XCUIApplication, deadline: Date?) -> HideKeyboardRoute.Outcome {
  // No one gesture dismisses every keyboard, so a chain of strategies
  // that checks after each whether the keyboard is actually gone.
  // Every strategy records itself, so a failure can say what was
  // attempted rather than only that something was.
  var tried: [String] = []
  let outcome: HideKeyboardRoute.Outcome? = smixGuarded("hide-keyboard") {
    guard app.keyboards.firstMatch.exists else { return .alreadyGone }
    func onScreen() -> Bool {
      let kb = app.keyboards.firstMatch
      return kb.exists
        && HideKeyboardRoute.keyboardOnScreen(keyboard: kb.frame, app: app.frame)
    }
    func focusedElement() -> XCUIElement {
      app.descendants(matching: .any)
        .matching(NSPredicate(format: "hasKeyboardFocus == true"))
        .firstMatch
    }
    func focusHeld() -> Bool { focusedElement().exists }
    func focusName() -> String {
      let focused = focusedElement()
      return focused.exists
        ? (focused.identifier.isEmpty ? focused.label : focused.identifier)
        : "nothing reports keyboard focus"
    }
    // Below the screen is either minimized or still on its way in, and
    // one look cannot tell which: watch it until it settles somewhere.
    // On its way in it does not move first — measured on a simulator
    // that minimizes its keyboard, right after typing it sat still at
    // y=918 for 0.9-1.0 s and then appeared on the screen in one step.
    // So "unmoved" has to outlast that by a margin before it means
    // minimized.
    if !onScreen() {
      let hold: TimeInterval = 2.5
      let began = Date()
      var looks: [(at: TimeInterval, sighting: HideKeyboardRoute.KeyboardSighting)] = []
      var verdict: HideKeyboardRoute.OffScreenVerdict?
      while verdict == nil {
        let kb = app.keyboards.firstMatch
        let sighting: HideKeyboardRoute.KeyboardSighting = !kb.exists
          ? .gone
          : HideKeyboardRoute.keyboardOnScreen(keyboard: kb.frame, app: app.frame)
            ? .onScreen : .offScreen(kb.frame)
        looks.append((Date().timeIntervalSince(began), sighting))
        verdict = HideKeyboardRoute.offScreenVerdict(looks, hold: hold)
        if verdict != nil { break }
        // Still moving after this long, or out of the host's time: not
        // settled, so it is treated as a keyboard to dismiss.
        if Date().timeIntervalSince(began) >= 4.0 { break }
        if let deadline, Date() >= deadline { break }
        Thread.sleep(forTimeInterval: 0.1)
      }
      switch verdict {
      case .gone?:
        return .alreadyGone
      case .minimized(let f)?:
        return .offScreen(
          focus: focusName(),
          seen: "at y=\(Int(f.minY)), \(Int(f.height)) tall, below an app "
            + "\(Int(app.frame.height)) tall, unmoved for \(hold) s")
      case .onScreen?, nil:
        break
      }
    }
    // The host stops listening at `deadline`. A strategy started after
    // it answers nobody: on a slow simulator the whole chain passed
    // 15 s and the host reported a step that "may have acted".
    func spent() -> Bool { deadline.map { Date() >= $0 } ?? false }
    func stopped() -> HideKeyboardRoute.Outcome {
      onScreen() || focusHeld()
        ? .stillPresent(tried: (tried.isEmpty
            ? "stopped before trying anything"
            : "tried \(tried.joined(separator: ", ")); stopped there")
            + ", because the host's time for this step ran out")
        : .dismissed(by: tried.last ?? "nothing: it went on its own")
    }
    // Each strategy already knew how to check its own work — it
    // just slept a flat 0.5s first and then looked exactly once.
    // Polling the same check returns as soon as the keyboard is
    // actually gone (and gives a slow dismissal more than 0.5s,
    // which the old shape would have called a failure).
    // Gone, or minimized below the display: either way nothing on
    // screen is covered any more. And the field let go of focus: a
    // keyboard that only left the tree for a moment does not count.
    // Measured on iOS 27, a touch on the Passwords bar took the
    // keyboard out of the tree for 0.96-1.0 s while the field kept
    // focus throughout, and then it came back.
    func keyboardGone() -> Bool {
      var until = Date().addingTimeInterval(1.0)
      if let deadline, deadline < until { until = deadline }
      let budget = PollBudget(limit: until.timeIntervalSinceNow)
      while true {
        Thread.sleep(forTimeInterval: 0.05)
        let began = budget.look()
        if !onScreen() && !focusHeld() { return true }
        if budget.spent(by: began) { return false }
      }
    }
    // First: the keyboard's own Return or Done key, the two that mean
    // "finished typing". A return key named for an action — Go, Search,
    // Continue, Next — is the field's submit: on a sign-in form Go signs in
    // and Continue sends the address, so putting the keyboard away must not
    // press it. Looked up by identifier, measured on iOS 27: a plain return
    // key is "Return", Go is "Go", Next is "Next:".
    for keyName in ["Return", "Done"] {
      if spent() { return stopped() }
      let key = app.keyboards.buttons[keyName]
      if key.exists {
        tried.append("key:\(keyName)")
        key.tap()
        if keyboardGone() { return .dismissed(by: "key:\(keyName)") }
      }
    }
    // Then: drag the focused field's scroll view down into the
    // keyboard. A form whose scroll view dismisses the keyboard by drag
    // (SwiftUI `.scrollDismissesKeyboard(.interactively)`) ignores a
    // touch outside the keyboard, and on a form a touch can land on the
    // other field; a drag only scrolls the content, so it comes before
    // the touches. Only where a scroll view holds the field — anywhere
    // else a drag has nothing to dismiss and could move something.
    if spent() { return stopped() }
    let focusedField = focusedElement()
    if focusedField.exists {
      let scrollers = (app.scrollViews.allElementsBoundByIndex
        + app.tables.allElementsBoundByIndex
        + app.collectionViews.allElementsBoundByIndex).map(\.frame)
      if let plan = KeyboardDismissDrag.plan(
        focused: focusedField.frame, scrollers: scrollers,
        keyboard: app.keyboards.firstMatch.frame, app: app.frame)
      {
        tried.append("drag-into-keyboard")
        let from = app.coordinate(withNormalizedOffset:
          CGVector(dx: plan.start.x, dy: plan.start.y))
        let to = app.coordinate(withNormalizedOffset:
          CGVector(dx: plan.end.x, dy: plan.end.y))
        // Fast: a slow drag lets the keyboard spring back.
        from.press(forDuration: 0.05, thenDragTo: to,
                   withVelocity: .fast, thenHoldForDuration: 0)
        if keyboardGone() { return .dismissed(by: "drag-into-keyboard") }
      }
    }
    // Then: touch outside the keyboard, just above its own top
    // edge. RN's Keyboard.dismiss responds to an outside touch, and
    // this used to aim at a flat 15% of the app frame — which on a
    // login form is where the OTHER field is, so the touch moved
    // focus instead of releasing it and the keyboard stayed up. The
    // keyboard knows where it is; ask it, bar above the keys included,
    // and touch the nearest spot above it that nothing acts on
    // (KeyboardOutsideTouch).
    if spent() { return stopped() }
    let kbFrame = app.keyboards.firstMatch.frame
    let appFrame = app.frame
    do {
      let snap = try app.snapshot()
      func all(_ s: XCUIElementSnapshot) -> [XCUIElementSnapshot] {
        [s] + s.children.flatMap(all)
      }
      let nodes = all(snap)
      let kbWindow = snap.children.first { w in
        w.elementType == .window && all(w).contains { $0.elementType == .keyboard }
      }
      // a control under the touch acts on it; text and plain views do not
      let acts: Set<XCUIElement.ElementType> = [
        .button, .link, .cell, .textField, .secureTextField, .textView, .searchField,
        .switch, .toggle, .slider, .stepper, .segmentedControl, .picker, .pickerWheel,
        .datePicker, .tab, .menuItem, .menuButton, .popUpButton, .checkBox,
        .radioButton, .comboBox, .incrementArrow, .decrementArrow, .key,
      ]
      if let kbWindow, kbFrame.height > 0 {
        let kbTop = KeyboardExtent.top(
          keyboard: kbFrame, window: kbWindow.frame, frames: all(kbWindow).map(\.frame))
        let actionable = nodes.filter { acts.contains($0.elementType) }.map(\.frame)
        if kbTop > appFrame.minY,
           let at = KeyboardOutsideTouch.point(
             keyboardTop: kbTop, app: appFrame, actionable: actionable) {
          tried.append("tap-above-keyboard")
          app.coordinate(withNormalizedOffset: CGVector(dx: at.x, dy: at.y)).tap()
          if keyboardGone() { return .dismissed(by: "tap-above-keyboard") }
        } else {
          tried.append("tap-above-keyboard skipped: no free spot above the keyboard")
        }
      } else {
        tried.append("tap-above-keyboard skipped: no window in the tree holds the keyboard")
      }
    } catch {
      tried.append("tap-above-keyboard skipped: the screen could not be read (\(error))")
    }
    // No swipe on the keyboard itself: a drag that starts on the keys
    // is slide typing. Measured 2026-09-28, one put "gu" into an empty
    // field and left the keyboard up.
    // Name what still holds the keyboard up, because "it did not
    // close" and "this field is still first responder" send the
    // caller to different places; and the return key when it is one
    // not pressed here, which may be the only way this field lets go.
    let keys = app.keyboards.buttons.allElementsBoundByIndex.map {
      (identifier: $0.identifier, frame: $0.frame)
    }
    let returnKey = KeyboardReturnKey.untried(buttons: keys, tried: ["Return", "Done"])
      .map { "; return key: \($0), the field's own submit, not pressed" } ?? ""
    return .stillPresent(
      tried: "tried \(tried.joined(separator: ", ")); focus: \(focusName())\(returnKey)")
  }
  // A caught exception is not evidence the keyboard is up.
  return outcome ?? .couldNotTell(
    why: "XCUITest raised while dismissing; the keyboard's state was never established")
}

import XCTest

/// The element under `identifier` a touch reaches, buttons first.
///
/// Several elements can carry one identifier: an iPad card's back button
/// and the list's own behind it. Asked for by subscript, XCUITest refuses
/// the pair as "multiple matching elements" — read as not found — and the
/// first match is the one the card covers. So every match is looked at, and
/// the first a touch reaches is taken; failing that, the first.
func smixReachableById(_ app: XCUIApplication, _ identifier: String) -> XCUIElement? {
  let byId = NSPredicate(format: "identifier == %@", identifier)
  let buttons = app.buttons.matching(byId)
  let query: XCUIElementQuery
  if buttons.firstMatch.exists || buttons.firstMatch.waitForExistence(timeout: 2) {
    query = buttons
  } else {
    let any = app.descendants(matching: .any).matching(byId)
    guard any.firstMatch.exists || any.firstMatch.waitForExistence(timeout: 3) else { return nil }
    query = any
  }
  let all = query.allElementsBoundByIndex
  if all.count <= 1 { return query.firstMatch }
  return all.first(where: { $0.isHittable }) ?? all.first
}

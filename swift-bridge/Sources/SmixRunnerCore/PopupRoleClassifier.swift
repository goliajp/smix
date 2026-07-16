// Pure three-way role classification, done in memory.
//
// Classifying per button with live predicate queries (two per button:
// `label == %@ AND userTestingAttributes CONTAINS ...`) does not survive a
// modal. While an in-app UIAlertController is up, a single XCUIElementQuery
// costs ~1.2 s, so 2N queries across N buttons accumulate past 15 s and hit
// the FlyingFox socket timeout, hanging the runner's main thread.
//
// Instead, `consume()` runs exactly two attribute-only queries up front
// (no label constraint) to collect the cancel / destructive label sets, and
// this function compares against them in memory. Query count is a fixed 2,
// independent of the button count N.
//
// Semantics match the per-button double-predicate approach: a cancel hit
// wins over destructive, and no hit means default. Locale-invariant — it
// consumes only label sets derived from Apple-internal
// userTestingAttributes, never literal English labels.
public func classifyPopupButtonRole(
  label: String, cancelLabels: Set<String>, destructiveLabels: Set<String>
) -> String {
  if cancelLabels.contains(label) { return "cancel" }
  if destructiveLabels.contains(label) { return "destructive" }
  return "default"
}

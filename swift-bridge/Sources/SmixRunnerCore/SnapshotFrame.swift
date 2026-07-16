import CoreGraphics

// Converts the `frame` value of `XCUIElementSnapshot.dictionaryRepresentation`
// into a CGRect. The value arrives either as `[String: Double]` (X/Y/Width/
// Height — the Apple a11y server's raw dict) or as a CGRect; anything else
// yields `.zero`.
//
// Kept as a pure Core function taking `Any?` so it has no XCUI dependency:
// the act side (findAndTapSystemPopupButton, which gets button frames via
// collectPopupNodes to compute a tap center), the sense side
// (convertSnapshotDict), and SPM unit tests all share one implementation.
public func frameFromDictValue(_ value: Any?) -> CGRect {
  if let d = value as? [String: Double] {
    return CGRect(
      x: d["X"] ?? 0, y: d["Y"] ?? 0,
      width: d["Width"] ?? 0, height: d["Height"] ?? 0)
  }
  if let r = value as? CGRect {
    return r
  }
  return .zero
}

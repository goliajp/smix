import CoreGraphics

// v4.3 c1 — XCUIElementSnapshot.dictionaryRepresentation 的 frame value → CGRect。
// frame value 形态 = [String: Double] (X/Y/Width/Height, Apple a11y server raw
// dict) 或 CGRect; 不可解析 → .zero。与 UITests 侧 convertSnapshotDict 既有
// frame 解析逻辑等价, 抽 Core 纯函数 (无 XCUI 依赖, 入参 Any?) 供 act side
// (findAndTapSystemPopupButton 经 collectPopupNodes 拿 button frame 算 tap
// center) + sense side convertSnapshotDict 复用 + SPM 单测可直验。
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

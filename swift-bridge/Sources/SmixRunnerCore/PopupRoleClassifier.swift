// C-fix — role 三态判定纯函数。consume() 在 in-app UIAlertController 真触
// 状态下扫每 button 时, 旧 classifyButtonRoleByLabel 对每 button 跑 2 次
// live predicate query (label==%@ AND userTestingAttributes CONTAINS ...);
// 在 modal alert 弹出状态下单次 XCUIElementQuery ~1.2s, 2N 次累加 >15s
// 撞 FlyingFox socket timeout 致 runner 主线程 hang。改为 consume 开头一次
// 性两个去 label 约束的 attribute-only query 收 label Set, 本函数做内存比对,
// query 数从 2N 降到固定 2 (与 button 数 N 无关)。
//
// 语义与旧 per-button 双 predicate 等价: cancel 命中优先于 destructive
// (旧 cancelPred 先查先返回), 都不命中为 default。locale-invariant —
// 仅消费 Apple-internal userTestingAttributes 派生的 label 集合, 无英文
// label 字面 (CLAUDE.md §12.1)。
public func classifyPopupButtonRole(
  label: String, cancelLabels: Set<String>, destructiveLabels: Set<String>
) -> String {
  if cancelLabels.contains(label) { return "cancel" }
  if destructiveLabels.contains(label) { return "destructive" }
  return "default"
}

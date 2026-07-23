# plan-hot — v2.8 到 C7：遮挡感知命中判定 —— 先调研 z 序可得性

## 目标 checkpoint

C7：**「从 XCUITest snapshot / 私有 API 能不能可靠拿到 z 序(或等价的遮挡判定信息)?」这个 EXT1 #4 defer 过的问题，被一份带证据的调研文档有据地回答**，落成三选一 verdict（`OBTAINABLE` / `NOT-OBTAINABLE` / `PARTIAL`），并把由此得出的 tier 决策写进 `docs/v2.md` 决策日志。

C7 是**研究先行 checkpoint**，只回答可得性、不做实现。verdict 出来后走实现还是 re-tier（把遮挡判定标 structurally-blocked），由**下一段热计划**按结论热化 —— 本段**不写任何实现步骤**（§2 无分叉铁律：`OBTAINABLE→实现` vs `NOT-OBTAINABLE→re-tier` 是一个分叉，必须留到分叉点已被 verdict 决定之后再热化）。

## 前置条件

```bash
git status --short | grep -q 'plan-history/v2.8-c6-hot.md'      # C6 热计划已归档
grep '^version' Cargo.toml | head -1                            # 期望：version = "2.0.0"
grep -q 'pub fn tap_landed_within' crates/smix-driver/src/lib.rs # C7 要扩的 v2.7-C1 命中判定地基在
python3 scripts/dev/route-conformance.py                        # pre-fold v2.0 additive 基线未破
python3 scripts/release/stress-select.py --test                 # C5 压测台 gate 仍绿
```

## 已经查清、不必重查的事实

- **本机环境**：Xcode 26.6 / Build 17F113；唯一可用 runtime = iOS 26.5（与 EXT1 反馈同系统同机型 iPhone 17 Pro）。
- **C7 扩的地基（v2.7-C1 已 land）**：`crates/smix-driver/src/lib.rs` 已有 `ActOutcome` / `HitElement` / `ActVerdict` / `tap_landed_within()`。命中链 `observed` 恒随 outcome 出栈，即便 `Confirmed`。**遮挡判定要加在这份地基上**，不是从零。
- **defer 前提的原始出处（读过，都指向「snapshot 无 z 序」）**：
  - `crates/smix-driver/src/lib.rs:1419-1433`「WHAT THIS CANNOT SEE: Occlusion」docstring —— 明写 scrim 也包含那个点故 `tap_landed_within` 恒 pass；snapshot 无 z 序；`isHittable` 被**刻意拒过两次**（① AX 可达但视觉被盖时恒 false = see-through tap 的语义就是穿过去；② v1.0.27 破了 QA-overlay 断言）。
  - `swift-bridge/Sources/SmixRunnerCore/TreeRoute.swift:273`「Snapshots are dead frames so no live `isHittable`」。
  - `swift-bridge/SmixRunnerUITests/SmixRunnerUITests.swift:1403-1406`「iOS hit-testing still routes the actual touch by z-order … Reading through is a sense capability; acting is still bound by iOS hit-testing」—— **iOS 自身 act 期按 z 序路由，但 runner 走的 snapshot 不携带它**。这是「静态 snapshot 轴」与「act 期 hit-test 轴」两条调研路的分界。
  - 同一「看不见遮挡」已写在三处散文：`docs/ai-guide/04-actions.md:50`、`docs/ai-guide/07-errors.md:132,171`、`docs/ai-guide/wire-format.md:81`（后者还记了 isHittable 拒因：floating overlay 下 hittable=false 但元素确实可见可断言）；+ v2.md 决策日志 2026-07-20（约 line 785）。
- **私有符号到达的现成范式（决定「可得」是否落地的关键）**：项目**已经** KVC 读私有 ivar —— `TreeRoute.swift:37-40` 用 `value(forKey:"hasKeyboardFocus")` 直读 `XCElementSnapshot._hasKeyboardFocus`。故「XCElementSnapshot 上有没有携带遍历序 / z-index / frontmost / occluded 的 ivar 或 selector」是一个**用符号枚举就能实证的开放问题** —— defer 当初**没有枚举过 snapshot 的私有 ivar 表**，这正是本调研要补的空白。
- **可 read-only 枚举私有符号的框架二进制（已核实实际位置）**：`XCElementSnapshot` **不在** XCTestCore（`nm` 0 命中），而在 `/Applications/Xcode.app/Contents/Developer/Platforms/iPhoneSimulator.platform/Developer/Library/PrivateFrameworks/XCTAutomationSupport.framework/XCTAutomationSupport`（`nm -a | grep -ci XCElementSnapshot` = **263**）。`nm -a` / `strings -a` / Obj-C `__objc` 段 ivar 枚举皆 read-only。**注意**：静态二进制可能 strip 掉部分符号，权威的 ivar 表以运行期 `class_copyIvarList(NSClassFromString("XCElementSnapshot"))` 为准 —— 但那要在 test-host 进程内，超出本段 read-only 边界；本段以 `nm`/`strings`/`__objc` 段静态枚举为主，运行期核实留给实现段（若 verdict 需要）。
- **已初探到的两个具体候选（轴 B，研究须逐一评估、勿当结论）**：`strings -a` XCTestCore 命中 `_XCT_requestElementAtPoint:reply:` —— 一个「点 → 元素」的私有 selector，是轴 B（act 期 hit-test 是否尊重 z 序）的头号候选；须判定它回的是否 z-order-aware 的最前响应者、还是复用被拒的 `isHittable` 语义。XCTestCore 的快速 `strings` grep **未**浮出 traversal / zIndex / zPosition / frontmost / occluded selector（轴 A 的候选需去 XCTAutomationSupport 的 `XCElementSnapshot` ivar 表里找，不是 XCTestCore）。
- **§9#6 不变量**：私有符号必须 dlsym / KVC 动态加载，不硬链接。故「拿得到」= 「在私有符号不变量内拿得到」。

## 本段预先定死的口径（防 scope 漂移与自欺）

- **全程 read-only**：读源码 / 读 Apple XCUITest 文档 / `nm` 枚举 `XCTestCore` 的 `XCElementSnapshot` ivar+selector。**不 edit 代码、不跑设备、不起 runner**。唯一的写是：调研文档 + 一条 §10 决策日志 + 把上述三处「看不见遮挡」散文注回指本调研文档。
- **isHittable 默认不是答案**：它被拒过两次（见「已查清」）。调研若把它当候选浮出，必须在 rubric 里**逐一对照那两条已记拒因**评估，不许把 `isHittable` 悄悄当成「z 序」塞回来。
- **no-ceiling-words（`.claude/rule/decomposition-discipline.md` debug/no-ceiling-words）**：若枚举一无所获，verdict = `NOT-OBTAINABLE` **且必须附上完整枚举出的 ivar/selector 表**作为「搜索已穷尽」的证据 —— 不许用「结构性不可能 / 平台天花板」hand-wave 收尾。负向结论的举证责任 = 把找过的地方全列出来。
- **verdict 恰为三选一 token**：`OBTAINABLE`（静态 snapshot 私有面即可可靠判遮挡）/ `NOT-OBTAINABLE`（两条轴都拿不到）/ `PARTIAL`（一条轴可得、另一条不可得，如 act 期 hit-test 可判但静态 snapshot chain 不可判）。verdict 是**产物**，本热计划不据它分叉。
- **tier 决策无条件落一条**：无论 verdict 是哪个，都往 `docs/v2.md` 决策日志写**一条**记「verdict + 由此的 tier」的行（内容随 verdict 变、存在性不变 = 不构成分叉）。

## 步骤（线性，1 个）

### S1. 调研 z 序可得性，落成带证据的 verdict + tier 决策

**红（先写「什么证据能证伪『拿得到 z 序』」）**
- 文件：`docs/research/c7-zorder-obtainability.md`
- 先写 `## Falsification rubric` 段，**在收集证据之前**把判据钉死，每条判据的 `Evidence:` 槽先留空（证明结论非事后合理化）。至少覆盖两条调研轴各自的证伪/证实条件：
  - **轴 A（静态 snapshot）**：判 `OBTAINABLE-A` 的充分证据 = `XCElementSnapshot`（或其 KVC 可达属性）上存在一个能确定「某 AX 可达元素在某点是否被前层覆盖 / 是否为该点最前元素」的 ivar 或 selector，且可用现成 `value(forKey:)` / dlsym 范式读出；判 `NOT-OBTAINABLE-A` 的充分证据 = 枚举出的 `XCElementSnapshot` ivar+selector 全表里**无**任何遍历序 / z-index / frontmost / hit-order / occluded 语义项。
  - **轴 B（act 期 hit-test）**：判 `OBTAINABLE-B` 的充分证据 = 存在一个 act 期私有 API（如 XCUICoordinate/私有 elementAtPoint/hit-test 入口）在合成触摸前后能回答「该点最前响应者是不是所 aim 元素」，且不复用被拒的 `isHittable` 语义；判 `NOT-OBTAINABLE-B` 的充分证据 = 唯一候选归结为 `isHittable`，撞上两条已记拒因。
- 跑一次断言 rubric 段存在且判据非空（红态 = 证据槽全空）：
  ```bash
  grep -q '^## Falsification rubric' docs/research/c7-zorder-obtainability.md && ! grep -q '^VERDICT:' docs/research/c7-zorder-obtainability.md
  ```
  期望：exit 0（rubric 已立、verdict 尚未下 = 证据未填，先失败一次证明不是倒着写）。

**绿（read-only 收证据 → 下 verdict → 落 tier 决策）**
- 文件：`docs/research/c7-zorder-obtainability.md` 补 `## Evidence` 段，逐条填 rubric 的 `Evidence:` 槽，每条带**可复核出处**（符号名 / `file:line` / Apple 文档 URL）。至少产出：
  1. **私有 ivar/selector 全表**：`nm -a`/`strings -a`/`__objc` 段枚举 **XCTAutomationSupport** 的 `XCElementSnapshot`（read-only，已核实类在此而非 XCTestCore），把命中的遍历序/z/frontmost/hit-order/occluded 语义项列出；无命中则列全表证明穷尽（no-ceiling-words）。并评估已初探到的 `_XCT_requestElementAtPoint:reply:`（轴 B 候选）是否 z-order-aware。
  2. **现成范式核对**：以 `TreeRoute.swift:37-40` 的 `_hasKeyboardFocus` KVC 读法为模板，判定候选 ivar 是否同法可读（§9#6 内）。
  3. **act 期轴核对**：读 Apple XCUITest hit-test / hittable / elementAtPoint 文档 + `SmixRunnerUITests.swift:1403-1406` 的 act 期 z 序注记，判轴 B 是否有非 isHittable 的可得路径。
  4. **isHittable 对照**：若浮出，逐条对照 lib.rs:1424-1428 + wire-format.md:81 的两条拒因。
- 文档末尾下一行 `VERDICT: <OBTAINABLE|NOT-OBTAINABLE|PARTIAL> — <一句话依据>`。
- 文件：`docs/v2.md` 决策日志末尾加**一条**（§10 格式）：`- {date} C7 遮挡 z 序可得性调研结论：{verdict}，tier→{实现/structurally-blocked re-tier} 理由：见 docs/research/c7-zorder-obtainability.md`。
- 文件：把三处「看不见遮挡」散文注（`04-actions.md:50` / `07-errors.md:132` / `wire-format.md:81`）各加一句指回 `docs/research/c7-zorder-obtainability.md`（让「为什么看不见」有据可查，不复述结论）。
- 跑一次断言 verdict 已定且证据在：
  ```bash
  grep -Eq '^VERDICT: (OBTAINABLE|NOT-OBTAINABLE|PARTIAL)\b' docs/research/c7-zorder-obtainability.md && grep -q '^## Evidence' docs/research/c7-zorder-obtainability.md
  ```
  期望：exit 0。

**重构（可选）**
- 仅整理文档标题层级 / 确认三处散文的回指链接可解析；不改任何 verdict 内容。

## Checkpoint C7 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
test -f docs/research/c7-zorder-obtainability.md \
  && grep -q '^## Falsification rubric' docs/research/c7-zorder-obtainability.md \
  && grep -q '^## Evidence' docs/research/c7-zorder-obtainability.md \
  && grep -Eq '^VERDICT: (OBTAINABLE|NOT-OBTAINABLE|PARTIAL)\b' docs/research/c7-zorder-obtainability.md \
  && grep -q 'c7-zorder-obtainability' docs/v2.md \
  && echo C7-PASS
```

期望：stdout 打印 `C7-PASS`，exit 0。含义 = 调研文档存在 + 有先立的证伪 rubric + 有证据段 + 有三选一 verdict + `docs/v2.md` 决策日志有一条引用该调研文档的 tier 决策行。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.8-c7-hot.md`。
2. 决策日志已在 S1 绿态写入（verdict + tier）；无需重复。
3. 调 sub-agent 热化 C8（面向 AI 的文档：`docs/ai-guide/authoring.md` + SessionState → 消费者响应 playbook），见 CLAUDE.md §6。**C7 的 verdict 若为 `OBTAINABLE` / `PARTIAL`，其实现工作是一段独立热计划，按 verdict 结论单独热化后插在 C8 前 —— 由用户 / 上层 agent 拍板插入时机，不在本段自作主张展开。**

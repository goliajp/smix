# plan-hot — v2 到 C3：真 animation-idle + OCR 键盘字符 fallback

## 目标 checkpoint

C3：`waitForAnimationToEnd` 从**固定 sleep** 变成**真的等到静止**（frame-diff 静默检测），`fill` 的字符落地由 OCR 兜底核验。通过后世界：§9#4「不提供裸 sleep」在字面上成立 —— 现存最后一处「等待与可观察条件无关」的实现被消掉。

## 前置条件

```bash
git log --oneline -1                                    # 期望 C2 已提交（6788b1ae1 或其后）
python3 scripts/dev/hygiene-scan.py --noise-only        # 期望 clean
bash scripts/dev/fence-check.sh                         # 期望 clean（C2 围栏不回归）
cargo test -p smix-adapter-maestro --test verb_table_gate   # 期望 2 passed
pgrep -fl "runner.ts|smix run|supervise"                # 期望空
pgrep -fl "gradle|mobilegate|emulator"                  # C3 不碰 Android，但 Swift 侧编译要 CPU
```

## 已确证的起点（C2 收尾时查实，不必重查）

- `parser.rs:1279-1283` 自陈：**「this verb is a FIXED sleep in smix, not an XCTest quiescence wait」**。`SmixQuiescenceSwizzle.m` 为性能把 XCTest 的 idle-wait no-op 掉了，而该 verb 本来也没走那条路。400ms 默认值是为 maestro 兼容留的。
- 该 verb 实际接受**三种形式**：bare（400ms 默认）/ 数字（ms）/ `{timeout: N}` 映射。
- 但 `VERB_TABLE` 记的 `arg_shape` 是 **`None`** —— `ArgShape` 是单值枚举，**表达不了联合**。C1 的 gate 只验「verb 在不在表里」，不验 arg_shape 准不准，所以抓不到这类失真。

## 步骤（线性）

### S1. frame-diff 静默检测（sense 层）

**红（写测试）**
- 文件：`crates/smix-screen/tests/quiescence.rs`
- 断言：(a) 两帧像素差 < ε 且连续 N 次 → `idle`；(b) 持续变化的帧序列在 ceiling 之前**不**报 idle；(c) 静止帧在一个 cadence 窗口内报 idle；(d) ε / N / cadence 是显式参数而非魔数。
- 纯函数先行：判定逻辑吃「帧序列」而非设备，可无设备单测。

**绿（实现）**
- 文件：`crates/smix-screen/src/`（新增 quiescence 模块）
- API：`pub fn is_quiescent(prev: &[u8], next: &[u8], epsilon: f32) -> bool` + 一个吃采样迭代器的判定器
- 关键点：判定是**纯逻辑**（stone 层，可单测）；**采样**是 I/O，留给调用方注入。这样 sense 判定不依赖 runner。

**重构**
- 无。

### S2. 接线 + arg_shape 失真收敛

**红**
- 文件：`crates/smix-adapter-maestro/tests/runtime_mock.rs`
- 断言：(a) mock 的帧序列「先动后静」→ bare `waitForAnimationToEnd` 等到静止才返回，且**早于** ceiling；(b) 一直动 → 到 ceiling 返回并 warn（不静默假装 idle）；(c) 数字形式 `waitForAnimationToEnd: 500` 保持 maestro 兼容语义（**显式 sleep，不做 idle 检测** —— 用户点名要 500ms 就给 500ms）。

**绿**
- runtime：bare/`{timeout}` 走 idle 检测（timeout 作 ceiling）；数字形式保持 sleep。
- `VERB_TABLE`：`waitForAnimationToEnd` 的 arg_shape 现状 `None` 与实际三形式不符。**二选一并记决策**：(i) 给 `ArgShape` 加联合表达（改动波及整表 + codemod），或 (ii) 承认 arg_shape 是「主形式」的近似、在 `smix-verbs` 头部注释里写清它不是完备契约。倾向 (ii) —— (i) 的收益不抵其 blast radius，且 arg_shape 自陈就是 "informational"。
- **顺带**：C1 的 gate 只验成员关系。若选 (ii)，把「arg_shape 是近似」这一事实写进 gate 测试的注释，免得后人再当契约读。

**重构**
- 无。

### S3. OCR 键盘字符 fallback（spec F）

**红**
- 文件：`crates/smix-adapter-maestro/tests/runtime_mock.rs`
- 断言：(a) mock 的 `fill` 后 OCR 读回缺字 → 触发重试；(b) 干净输入 → **不**产生 OCR 往返（快路径不被拖慢）；(c) 重试仍缺 → 明确失败，不静默通过。

**绿**
- `smix-input` / runtime：`fill` 后按需 OCR 核验；仅在 OCR 兜底已启用时生效。
- 关键点：这是 sense(verify) + act(retry) 的组合，两者都在 core。

**重构**
- 无。

## Checkpoint C3 验收

```bash
cargo test -p smix-screen 2>&1 | tail -3                          # 期望 all pass（含 quiescence）
cargo test -p smix-adapter-maestro 2>&1 | grep -c FAILED           # 期望 0
cargo test --workspace 2>&1 | grep -c "^test result: ok"           # 期望 ≥124（不回退）
cargo build --workspace 2>&1 | grep -c warning                     # 期望 0
python3 scripts/dev/hygiene-scan.py --noise-only                    # 期望 clean
bash scripts/dev/fence-check.sh                                     # 期望 clean
cd swift-bridge && swift test 2>&1 | grep "Executed"                # 期望 0 failures
xcodebuild build-for-testing -project swift-bridge/SmixRunner.xcodeproj \
  -scheme SmixRunner -destination 'generic/platform=iOS Simulator'  # 期望 exit 0（C1 发现：此前无 gate 编译它）
```
期望：全部通过。**真机验证留 real-sim smoke**：frame-diff 的 ε / cadence 只有在真模拟器上才有意义 —— 纯逻辑单测证明不了「400ms 固定 sleep 换成 idle 检测后，真实动画不早退」。这条要么本 checkpoint 做一次 real-sim smoke，要么明确记为「做了+未验证」。

## 完成后动作

1. 归档本文件到 `docs/plan-history/v2-c3-hot.md`
2. 生成新 `plan-hot.md`（到 C4：MCP 全驱动面），见 CLAUDE.md §6

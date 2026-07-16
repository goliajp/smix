# plan-hot — v2 到 C1：VERB_TABLE 单一真源 + 五矛盾收敛 + hygiene sweep

## 目标 checkpoint

C1：VERB_TABLE 成为被测试强制的单一真源（parser 接受的 verb ⊆ VERB_TABLE）；五矛盾各有落地答案；crate/Swift/Kotlin 源码开发噪声清零；`examples/hello.yaml` 存在且 parse OK。通过后世界：外部读者读到的是干净、自洽、单一真源的代码。

## 前置条件

```bash
pgrep -fl "runner.ts|smix run|supervise|bun test:e2e"   # 期望空（in-house batch 不活动）
git branch --show-current                                # 期望 develop（或从 develop 切 feature 分支）
cargo build --workspace 2>&1 | grep -c warning           # 记录基线 warning 数
```

## 步骤（线性）

### S1. VERB_TABLE 单一真源（矛盾①）

**红（写测试）**
- 文件：`crates/smix-adapter-maestro/tests/parser.rs`
- 断言：枚举 parser dispatch 接受的每个 top-level verb 字符串，断言每个都在 `smix_verbs::VERB_TABLE`（`is_known_verb`）。当前应**失败**，暴露 `clearUserDefaults` / `resetAppData` / `clearAppData` 等缺失项（精确全集由测试打印，不用 grep）。

**绿（实现）**
- 文件：`crates/smix-verbs/src/lib.rs`
- 动作：为测试暴露的每个缺失 verb 加 `VerbEntry`（正确 category/arg_shape；`clearUserDefaults`→Lifecycle/Mapping、`resetAppData`→Lifecycle、`clearAppData`→Lifecycle）。`runScript`/`evalScript`/`assertWithAI` 保持排除（parser 若接受则测试需 allowlist 它们为「有意排除」并让 codemod warn）。
- 关键点：测试变绿 = parser ⊆ table 成立；这条测试进 ship gate。

**重构**
- 若发现 smix_name 重载（expect×3 等）无 helper，酌情补 `find_by_smix` 反查。

### S2. 其余四矛盾 + examples/hello.yaml

**红**
- 文件：`crates/smix-selector/tests/` + 新 `examples/hello.yaml`
- 断言：(a) selector 变体数测试 == 11；(b) `smix run --check examples/hello.yaml` parse OK（先失败：文件不存在）。

**绿**
- 矛盾③ swipeAtCoord：在 `docs/v2.md` §10 决策日志加一行——授权 `swipeAtCoord` 为第二 native escape hatch（理由：与 tapAtCoord 同源 Apple event chain），**或**从 VERB_TABLE 删除。二选一，记录。
- 矛盾② assertScreenshot：`docs/v2.md` 决策——v2 实现 snapshot-compare 或移除 row；本 step 先定状态。
- 矛盾④ selector count：改 `crates/smix-selector/src/lib.rs:241` 注释 "6 base forms" → 准确描述 11 变体（6 base + 5 L4-L7 层）。
- 矛盾⑤ 4 层文档：本 cycle 已建 `docs/v2.md` + `plan-cold/v2.md` + `plan-hot.md`（本文件）；C7 修 `docs/v3.md` 指针。
- 新建 `examples/hello.yaml`（黄金路径：launchApp → tapOn id → assertVisible）+ `examples/README.md`。

**重构**
- 无。

### S3. hygiene sweep（26 crate + Swift + Kotlin + MCP schema）

**红（不变式基线）**
- 命令：`python3 scripts/dev/hygiene-scan.py --noise-only` → **实测基线 1642 处**（初测 1229 漏了 `swift-bridge/SmixRunnerUITests/` 的 413 处——最初 scope 误写成 `swift-bridge/Sources`；该目录经 `smix-runner-sources` 分发到用户机器，必须扫）。原 1229 分布 / 28 区域（version-cluster 726 · cjk-comment 292 · insight 72 · cluster-tag 51 · phase-tag 28 · round-n 25 · ask-n 15 · c5i 15 · plan-refs 5）。最重：adapter-maestro 481 · swift-bridge 244 · sdk 80 · android-runner 65 · driver 53。仅 `smix-fixture` / `smix-runner-sources` 干净（26/28 crate 受影响）。
- 该脚本有噪声即 exit 1，是 C1 的机器可判 gate，并防未来回归。CJK 只扫注释行——`localizedText` 的日文/中文测试数据是合法输入，绝不能算噪声。

**绿（实现，派 worktree-isolated agent 批量）**
- 规则：删 vX.Y cN / Phase X / insight / round-N / Ask N / plan-cold / plan-hot / CJK 注释片段（含 MCP `main.rs:58` 的 `跟` 与 `smix-selector/src/lib.rs:431`）。保留 OS-bug workaround / invariant / ABI 契约注释（带 WHY）。修 shipped example 里的 `insight://` 泄漏。
- 每 crate 独立 diff review；`smix-fixture` / `smix-runner-sources` 已干净跳过。
- 不变式：sweep 后 `cargo build --workspace` warning 数 ≤ 基线且 `cargo test --workspace` 全绿、`swift test` 全绿。

**重构**
- 无（纯注释/字符串清理，不改行为）。

## Checkpoint C1 验收

```bash
cargo test -p smix-adapter-maestro 2>&1 | grep -E "parser.*table|verbs_subset"   # 期望 pass
cargo build --workspace 2>&1 | grep -c warning                                    # 期望 0
cargo test --workspace 2>&1 | tail -3                                             # 期望 all pass
python3 scripts/dev/hygiene-scan.py --noise-only                                   # 期望 exit 0 "clean"
test -f examples/hello.yaml && echo OK                                            # 期望 OK
```
期望：全部通过；VERB_TABLE 与 parser 对齐；噪声清零；hello.yaml 存在。

## 完成后动作

1. 归档本文件到 `docs/plan-history/v2-c1-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 C2：围栏式 AI 断言层），见 CLAUDE.md §6

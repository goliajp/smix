# plan-hot — v2 到 C2：围栏式 AI 断言层

## 目标 checkpoint

C2：`assertCondition` / `extractWithAI` 可用 —— 截图 → 本地 `claude` CLI → 结构化 verdict，opt-in、输出标注非确定性。通过后世界：smix 补上 maestro `assertWithAI` / `extractTextWithAI` 的对位能力，且**删掉 `smix-ai-tier` crate 不会动到任何 sensing 代码**（这条可执行的删除性即 §9#2 围栏的证明，也是 C2 的核心验收）。

## 前置条件

```bash
git log --oneline -1                                    # 期望 C1 已提交（ed2285cff 或其后）
python3 scripts/dev/hygiene-scan.py --noise-only        # 期望 clean（C1 不回归）
cargo test -p smix-adapter-maestro --test verb_table_gate  # 期望 2 passed
which claude                                            # 期望有；无则仅 mock 路径可测
pgrep -fl "runner.ts|smix run|supervise"                # 期望空
```

## 步骤（线性）

### S1. `smix-ai-tier` crate + verdict 契约

**红（写测试）**
- 文件：`crates/smix-ai-tier/tests/verdict.rs`
- 断言：(a) `StructuredVerdict` 从 CLI 的 JSON 输出反序列化，`{pass, reason}` 齐全；(b) CLI 不存在时返回 `DriverError` 且 hint 含安装指引，**不 panic**；(c) CLI 返回非 JSON 时是明确错误而非静默 false。

**绿（实现）**
- 文件：`crates/smix-ai-tier/src/lib.rs`（新 crate，加进 workspace members）
- API：`pub async fn judge(screenshot_png: &[u8], condition: &str) -> Result<StructuredVerdict, ExpectationFailure>`
- 关键点：截图**带外**取（simctl / `smix-screen`，**不是** runner HTTP route —— 无 `/screenshot` route）；单 provider 走本地 `claude` CLI（§9#2）；crate 只依赖 `smix-error` + `smix-screen`，**不依赖** resolver / driver / selector（依赖方向即围栏）。

**重构**
- 无。

### S2. verb 接线（VERB_TABLE → parser → runtime → SDK）

**红**
- 文件：`crates/smix-adapter-maestro/tests/parser.rs` + `tests/runtime_mock.rs`
- 断言：(a) `assertCondition: "a red toast is visible"` 解析成 `Step::AssertCondition`；(b) `extractWithAI: {into, fields}` 解析成 `Step::ExtractWithAI`；(c) 未开 opt-in 时这两个 verb **明确报错**（不静默跳过）；(d) mock verdict `pass=false` → `AssertionFailed` 且 message 含 `[AI · non-deterministic]`；(e) `verb_table_gate` 仍绿（新 verb 必须同时进 VERB_TABLE）。

**绿**
- `crates/smix-verbs`：加 `v("assertWithAI", "assertCondition", Assert, BareString)` + `v("extractTextWithAI", "extractWithAI", Assert, Mapping)`；**从末尾「有意排除」注释里移除 assertWithAI**（它不再是排除项）。
- `smix-adapter-maestro`：parser dispatch + `Step::AssertCondition` / `Step::ExtractWithAI`；runtime 调 `smix-ai-tier::judge`；`ACCEPTED` 列表同步（gate 会强制）。
- `smix-sdk`：`App::assert_condition(&self, condition: &str)`。
- opt-in：`config: { aiAssertions: on }` frontmatter 或 `--enable-ai-assertions`（对齐既有 `--enable-ocr-fallback` 先例）。

**重构**
- `extractWithAI` 的 `output.*` 写入复用既有 output store，不新建。

### S3. 围栏证明 + 文档

**红（可执行的删除性）**
- 命令：临时从 workspace 移除 `smix-ai-tier` 并 stub 掉两个 verb → `cargo test -p smix-selector -p smix-selector-resolver -p smix-screen` **必须全绿**（sensing 零改动）。这条是 §9#2 的机器可判证明，不是口头声明。

**绿**
- 恢复；把该删除性测试固化成 CI 可跑的形式（feature flag 或文档化命令）。
- `docs/ai-guide/` 补 AI 断言层用法 + 明说非确定性；`docs/v2.md` 记决策。

**重构**
- 无。

## Checkpoint C2 验收

```bash
cargo test -p smix-ai-tier 2>&1 | tail -3                          # 期望 all pass
cargo test -p smix-adapter-maestro 2>&1 | grep -c "FAILED"         # 期望 0
cargo test -p smix-adapter-maestro --test verb_table_gate          # 期望 pass（新 verb 已进表）
cargo build --workspace 2>&1 | grep -c warning                     # 期望 0
python3 scripts/dev/hygiene-scan.py --noise-only                   # 期望 clean
grep -rn "smix_ai_tier\|smix-ai-tier" crates/smix-selector crates/smix-selector-resolver crates/smix-screen crates/smix-driver | wc -l   # 期望 0（围栏：sensing 不得引用 AI 层）
```
期望：全部通过。最后一条是围栏的静态证明 —— sensing / driver 侧对 AI 层零引用。

## 完成后动作

1. 归档本文件到 `docs/plan-history/v2-c2-hot.md`
2. 生成新 `plan-hot.md`（到 C3：真 animation-idle + OCR 键盘 fallback），见 CLAUDE.md §6

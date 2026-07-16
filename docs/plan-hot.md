# plan-hot — v2 到 C4：MCP 全驱动面

## 目标 checkpoint

C4：外部 agent 能只经 MCP 跑完一次真实会话 —— 启动 app → 找 → 点 → 输入 → 滚动 → 断言 → 出错时拿到可读诊断。通过后世界：「smix 是 AI dev/debug 闭环里的执行底座」这句话有可执行的证据，而不是 dossier 里的一句话。

## 前置条件

```bash
git log --oneline -1                                  # 期望 C3 已提交（57774207c 或其后）
python3 scripts/dev/hygiene-scan.py --noise-only      # 期望 clean
bash scripts/dev/fence-check.sh                       # 期望 clean
cargo test -p smix-adapter-maestro --test verb_table_gate  # 期望 2 passed
pgrep -fl "runner.ts|smix run|supervise"              # 期望空
```

## 已确证的起点（C3 收尾时查实）

- MCP 现有 **6** 个 tool：`smix_describe` / `smix_tree` / `smix_find_text` / `smix_tap_text` / `smix_press_key` / `smix_screenshot`。`App::` 有 **68** 个公开 async 方法。
- **`smix_tap_by_id` 不存在** —— MCP 只能按文本点。而 `docs/ai-guide/03-selectors.md` 把 id 列为最稳的选择器，`examples/hello.yaml` 也全用 id。**MCP 拿不到自家推荐的那条路**。
- `App::tap(&Selector)` 吃完整 `Selector`；`Selector` 是 serde untagged + camelCase wire。即 MCP 可以直接收选择器对象，不必每种选择器开一个 tool。

## 步骤（线性）

### S1. 选择器入参形状（先定，否则每个 tool 都要返工）

**决策点**：MCP 的点/找/断言类 tool 怎么收选择器？
- (a) 每种选择器一个 tool（`tap_text` / `tap_id` / `tap_role` …）→ 组合爆炸，且新增选择器要动 MCP。
- (b) 一个 tool 收 `Selector` JSON blob → 最强，但 `Selector` 是 untagged enum，`JsonSchema` 对它不友好，agent 读到的 schema 会含糊。
- (c) **一个 tool + 扁平可选字段**（`{ id?, text?, label?, role?, ocrText? }`），镜像 yaml 短形式，内部构造 `Selector`。

**倾向 (c)**：agent 读到的 schema 自解释、与 yaml 表面同构、新增选择器只加一个可选字段。**恰好一个字段必须给** —— 零个或多个都是明确报错（不猜）。

**红**
- 文件：`crates/smix-mcp/tests/selector_params.rs`
- 断言：(a) `{id:"foo"}` → `Selector::Id`；(b) `{text:"Submit"}` → `Selector::Text`；(c) 空对象 → 明确错误且**指出该给哪些字段**；(d) `{id, text}` 同给 → 明确错误（不静默取其一）。

**绿**
- `smix-mcp`：`SelectorParams` + `fn to_selector(&self) -> Result<Selector, McpError>`。

### S2. 补齐驱动面

**红**
- 文件：`crates/smix-mcp/tests/tools.rs`（新，mock 或 schema 级）
- 断言：每个新 tool 的 schema 有非空 description（它就是 agent 的**唯一**文档）、参数可解析、错误经 `to_prompt()` 出（不是 `Debug`）。

**绿** —— 按「agent 跑完一次会话真正需要什么」补，不是按 dossier 清单照抄：
- `smix_tap`（收 SelectorParams —— **补上缺失的 by-id**）
- `smix_fill`（selector + text）
- `smix_swipe`（direction）· `smix_scroll`
- `smix_launch_app` / `smix_stop_app`（没有这两个，agent 无法起手）
- `smix_assert_visible` / `smix_assert_not_visible`
- `smix_diagnostic_dump`（出错时 agent 自救的入口）
- 关键点：每个 tool 映射一个已有 `App::` 方法，**不在 MCP 层写新逻辑**。

**不做，且记理由**：
- **不暴露 AI 断言层**。经 MCP 驱动的已经是一个 agent —— 它能直接看 `smix_screenshot`。让 agent 调一个 tool 去请另一个模型判断它自己看得见的屏幕，是绕路，不是能力。
- 不暴露 68 个方法。MCP 是驱动面，不是 SDK 镜像。

### S3. 文档 + 诚实收尾

**绿**
- 重写 MCP README（现为 stub）：连接模型、env、tool 全表、`.mcp.json` 拷贝即用。
- `docs/ai-guide/` 补 MCP 设置页（dossier 的 docs-web 已 spec 过）。
- **schema 里的英文即文档** —— C1 修 `跟` 那次教训：这些字符串直接送给每个连上来的 agent。

## Checkpoint C4 验收

```bash
cargo test -p smix-mcp 2>&1 | tail -3                              # 期望 all pass
cargo build --workspace 2>&1 | grep -c warning                     # 期望 0
python3 scripts/dev/hygiene-scan.py --noise-only                    # 期望 clean
bash scripts/dev/fence-check.sh                                     # 期望 clean
# 每个 tool 的 schema description 非空（它是 agent 唯一的文档）
cargo run -p smix-mcp -- --dump-schema 2>/dev/null | python3 -c "import sys,json; ..."  # 若无此 flag，S2 顺带加
```
期望：全部通过。

**真机端到端留 real-sim smoke**：schema 测试证明不了「agent 真能跑完一次会话」。C3 的教训 —— mock 会证明硬件给不了的东西。要么本 checkpoint 起一次 runner + 真 MCP 会话（起 runner 前必查 batch 占有者），要么明记「做了+未验证」。

## 完成后动作

1. 归档本文件到 `docs/plan-history/v2-c4-hot.md`
2. 生成新 `plan-hot.md`（到 C5：Android parity 门禁），见 CLAUDE.md §6

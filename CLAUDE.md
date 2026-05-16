# CLAUDE.md — simx 最高开发指导

> 任何对此项目代码 / 文档做修改的 AI 必须先读完此文件。规则冲突时以此文件为准；其他 docs/ 文件是此方法论的具体应用。

## 0. 文档分层（必须遵守的目录契约）

项目永远保持 **4 层信息**。任何时间点这 4 层都必须存在且互不重叠：

| 层 | 文件 | 范围 | 数量 |
|---|---|---|---|
| **[1] Roadmap** | `docs/roadmap.md` | v0.1 → v1.0 → v1.1 → v2 全版本路径，每版本 1 句话 | 1 |
| **[2] 当前版本边界** | `docs/v1.md`（当前是 v1）| 当前大版本要做什么 / 不做什么 | 1（每大版本 1 份） |
| **[3] 热计划** | `docs/plan-hot.md` | **现在 → 下一个 checkpoint**（不是到整个版本！）| **永远只有 1 个** |
| **[4] 冷计划** | `docs/plan-cold/v0.X.md` | 一个 minor 版本的全貌（含 checkpoint 概要列表） | 每个 minor 版本 1 份 |

完整目录契约：

```
CLAUDE.md                       ← 此文件，方法论 + 项目宪法
README.md                       ← 面向用户 / AI 测试作者
docs/
  design.md                     ← 设计决策（why）
  roadmap.md                    ← [1/4] 全版本路径
  v1.md                         ← [2/4] 当前版本边界
  plan-hot.md                   ← [3/4] 到下一个 checkpoint 的详尽计划（唯一）
  plan-cold/
    v0.1.md ...                 ← [4/4] 每个 minor 版本的冷计划
  plan-history/
    v0.X-cN-hot.md              ← 已归档的旧热计划（按 checkpoint 归档）
src/                            ← 实现
examples/                       ← AI 测试作者的黄金路径样本
```

**改动规则**：
- 任何不在 `v1.md` 范围内的功能 → 不写，写了会被砍回去
- 进入 step 前 `TaskUpdate in_progress`，checkpoint 通过后 `completed`
- checkpoint 通过 → **立即归档 `plan-hot.md` 到 `plan-history/`，立即热化下一段**
- 设计决策（why）改动 → 进 `design.md` 决策日志，不进 plan
- 版本边界改动 → 改 `v1.md`，不在 plan 里偷偷加

---

## 1. 核心原则：冷热分离

**只把眼前一步做细，远的留 placeholder。**

| 状态 | 范围 | 何时写 | 数量约束 |
|---|---|---|---|
| **热（hot）** | 现在 → **下一个 checkpoint**，可立刻动手 | 当前一段，永远只有 1 个 | 严格 1 |
| **冷（cold）** | 一个 minor 版本（含其 checkpoint 概要列表） | 每个 minor 版本 1 份，事先就要存在 | 每版本 1 |

**为什么粒度选"到下一个 checkpoint"而非"整个版本"**：远期细节会随近期发现而失效，过早详化等于"决策一次浪费一次"。一个 checkpoint 内的步骤少（1-3 个 step），完成快，反馈紧。每过一个 checkpoint，根据当时实际情况再生成下一段热计划。

**禁止**：
- ❌ 在冷计划里写代码骨架 / 具体 API 名 / 完整步骤分解
- ❌ 同时有两个 `plan-hot.md`
- ❌ 热计划跨越 checkpoint
- ❌ 在主对话里展开热计划（要污染上下文）—— 调 sub-agent

---

## 2. 热计划格式（强制）

`docs/plan-hot.md` 必须有以下章节，**顺序与命名固定**：

```markdown
# plan-hot — v0.X 到 C{N}：{一句话目标}

## 目标 checkpoint
C{N}：{这个 checkpoint 通过后世界变成什么样}

## 前置条件
{进入此热计划前必须满足的状态，用可执行命令表达}

## 步骤（线性，无分叉；通常 1-3 个）

### S1. {imperative 短标题}

**红（写测试）**
- 文件：`{path}`
- 断言：{具体行为}

**绿（实现）**
- 文件：`{path}`
- API：{签名}
- 关键点：{1-3 条}

**重构**
- {可选清理点}

### S2. ...

## Checkpoint C{N} 验收
```bash
{可执行命令}
```
期望：{具体可判断的输出 / 退出码}

## 完成后动作
1. 归档此文件到 `docs/plan-history/v0.X-c{N}-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 C{N+1}），见 §6
```

**严格要求**：
- 一份热计划只覆盖**一个** checkpoint。如果你想写多个 checkpoint，停下来——拆。
- 步骤必须线性。出现 "if A then B else C" → 上一层就要先决定走哪条
- Checkpoint 必须机器可判断（命令 + 期望输出 / 退出码）。"manually verify" / "looks correct" 不算
- 不准写"可选"步骤

---

## 3. 冷计划格式（强制，单文件 ≤ 100 行）

```markdown
# plan-cold/v0.X.md — {一句话目标}

## 为什么要做
{2-3 句话。这个版本不做的话，v1 缺什么}

## 入口条件
{前置 minor 版本 + 必须验证的事实。用可执行命令表达可验证项}

## 资源依赖
- 环境：{Xcode 版本 / iOS runtime / macOS 版本 / 工具 ...}
- 外部 API / 文档：{链接}
- 人 / agent：{是否需要专门 sub-agent}

## 已知风险
- {风险 1：对策}
- {风险 2：对策}

## TDD 要点
{这个版本的测试策略概要}

## Checkpoint 概要列表
- C1：{一句话}
- C2：{一句话}
- C3：{一句话}
- ...

## 出口验收
{所有 Cn 通过 + e2e smoke 描述}

## 触发热化的 prompt 模板
（§6 的标准模板。如有本版本特殊 context 在此追加）
```

冷计划**不**包含：
- 具体步骤 / 文件路径 / API 签名
- 代码示例
- 红绿测试细节

冷计划**包含但简短**：
- Checkpoint 概要（每个一行；具体如何到达，热化时生成）
- 验证入口条件的命令

---

## 4. TDD 三段式（每个 step 必须）

每个热计划 step 必须按红 → 绿 → 重构顺序，**一次只能在一个段位**。

### 红
- 测试先于实现写
- 测试**必须先失败一次**，证明它在测真实行为
- 跑测试：`bun x vitest run <file>`，应看到红色
- 一个 step 写 1-3 个 test case，不要堆 10 个

### 绿
- 最少代码让测试通过
- 不做"顺便重构"，不优化命名，不加额外抽象
- 跑测试：应看到绿色
- 如果绿了但引入了其他失败 → 立刻 stop，回到红

### 重构
- **可选**。只在代码有明显坏味时做
- 重构期间测试保持绿色
- 不引入新行为；只改结构

**违反三段式 = 这个 step 没做**。重做。

---

## 5. Checkpoint 设计原则

Checkpoint 不是"我觉得通过了"。Checkpoint 是 **"如果半年后重跑，能给出确定结论"**。

**好的 Checkpoint**：
```bash
bun x vitest run src/sim/__tests__/simctl.test.ts
# 期望：exit 0，输出含 "12 tests passed"
```

**坏的 Checkpoint**（拒绝）：
- "确认 simctl 能正确列出模拟器"
- "screenshot 看起来不糊"
- "性能足够"

每个 step 的 step-level 验证（红/绿）+ 该热计划末尾的 Checkpoint 验收 都要满足：
1. 单条命令可跑（或最多 2-3 条 pipe）
2. 退出码 / 输出有可机器判断的 pass 条件
3. 不依赖人工读图 / 主观判断（视觉回归是例外，v1 不做）

---

## 6. 阶段转换：何时 / 如何热化下一段

**永远不要自作主张展开下一段。** 触发条件全部满足才热化：

1. 当前 `plan-hot.md` 的 Checkpoint 验收命令通过
2. 用户 / 上层 agent 明确说"开始 C{N+1}"或同义
3. 下一段对应的冷计划"入口条件"可执行验证通过（如果跨 minor 版本边界）

满足后**立即做两件事**：
1. `mv docs/plan-hot.md docs/plan-history/v0.X-c{N}-hot.md`
2. **调 sub-agent** 生成新 `plan-hot.md`（不要在主对话里展开）

### 标准热化 prompt 模板

```
你需要生成 docs/plan-hot.md，覆盖范围：v0.X 的 C{N+1}。

约束：
- 完全遵守 CLAUDE.md §2（热计划格式）
- 完全遵守 CLAUDE.md §4（TDD 三段式）
- 完全遵守 CLAUDE.md §5（Checkpoint 设计原则）
- 只覆盖一个 checkpoint，1-3 个 step
- 线性、无分叉。任何 "如果...否则..." 必须事先决定走哪条

必须阅读的 context：
- CLAUDE.md（此方法论）
- docs/roadmap.md
- docs/v1.md（边界）
- docs/plan-cold/v0.X.md（含 checkpoint 概要列表）
- docs/plan-history/v0.X-c{N}-hot.md（上一段，看产出与遗留）
- 上一段产出的所有 src/ 文件（看实际接口形态）

必须执行的本机探测：
- `xcodebuild -version`
- `xcrun simctl list runtimes -j`
- {冷计划"入口条件"里的可执行命令}

输出位置：docs/plan-hot.md
完成后报告：
- 步骤数（应为 1-3）
- Checkpoint 验收命令
- 任何与冷计划假设不符之处（必须列出，不要隐瞒）
```

### 何时该拒绝热化

发现以下任一情况，**拒绝**展开，回报上层：

- 入口条件未满足（具体哪一条）
- 冷计划假设与本机实际探测不符（具体哪一项）
- 当前 `v1.md` 边界与冷计划范围冲突
- 上一段有未关闭的 known issue 影响本段

---

## 7. 任务跟踪（Task tool）

强制使用 `TaskCreate` / `TaskUpdate`：

- 每个热计划的每个 **Step** 对应 1 个 task
- 进入 step 前 `TaskUpdate status=in_progress`
- step 的红/绿/checkpoint 通过后 `TaskUpdate status=completed`
- 整个热计划 Checkpoint 通过 → 总 task `completed`，并自动触发 §6 检查

**一次只能有一个 task in_progress**。

---

## 8. 代码原则

### 8.1 范围纪律
- 不实现 `v1.md` 范围外的功能。看到诱惑 → 加一行到对应冷计划，不动当前
- 不"顺便修一下别的"。发现别的问题 → 加 task，不在当前 step 改
- 不写 "for future use" 代码

### 8.2 注释纪律
- 默认不写注释
- 只在 WHY 非显然时写：隐藏约束、不变量、绕开 OS bug、出乎意料的行为
- 永远不写 WHAT（命名负责）
- 永远不写 "added for X / used by Y / fixes #N"

### 8.3 类型纪律
- TS strict + `noUncheckedIndexedAccess` + `exactOptionalPropertyTypes` 全开
- `any` / `as any` 需在同一行写 WHY 注释
- 公开 API 必须显式标注返回类型

### 8.4 测试纪律
- simx 自身用 **vitest**（dev 依赖）
- 用户测试 iOS app 用 **simx 自家 runner**（`src/sdk/test.ts`）
- 不混
- 失败信息要可读：用 `ExpectationFailure.toPrompt()` 思路，不打 stack

### 8.5 错误纪律
- 业务失败抛 `ExpectationFailure`（结构化）
- 程序员错误抛普通 `Error`
- 不吞错误，除非明确写 "为什么吞"

### 8.6 命名纪律
- API 表面用 Playwright 同源名
- 内部不强制

### 8.7 依赖纪律
- **默认使用非 beta/alpha 的最新稳定版**。加新 dep 或升级时先查 npm latest tag
- 不 pin specific 老版本，除非有书面 known incompatibility
- 加 dep 前先评估能否用 stdlib / 已有 dep 完成
- 不引入 GPL / AGPL（避免传染到 simx 这个闭源候选）
- 用 bun，不混 npm/pnpm/yarn — lockfile 只 `bun.lock` / `bun.lockb`

---

## 9. 不变量（任何改动都不能违反）

1. **只支持模拟器**。任何引入真机代码路径的 PR 直接拒
2. **不引入 multi-provider VLM 抽象**。VLM 走本机 `claude` CLI
3. **不暴露 xpath / 坐标 selector 到 DSL 表面**
4. **不提供裸 `sleep` API**
5. **失败信息必须 AI-readable**（含 visibleElements / suggestions）
6. **私有符号必须 dlsym 动态加载**，不硬链接
7. **4 层信息结构始终保持**。任何时间点都有 roadmap / v{cur}.md / plan-hot.md / plan-cold/v0.X.md

违反任何一条 = 改动被拒。

---

## 10. 决策记录

非微小决策（影响 v1 范围 / API 表面 / 不变量）必须在 `docs/v1.md` 末尾"决策日志"段加一行（决策日志按当前活跃大版本归属：v1 cycle 内的决策都进 `docs/v1.md`，v2 cycle 起新建 `docs/v2.md` 的决策日志段）：

```
- {YYYY-MM-DD} {一句话决策} 理由：{1-2 句}
```

不补决策 = 决策不存在 = 后续可被任意推翻。

---

## 11. 与 Claude Code 的协同

simx **是** Claude Code 子产品。开发期间默认假设：
- 测试 / 集成时手边有 `claude` CLI 可用
- 文档语言可直接引用 Claude Code 概念（MCP / skill / hook）
- AI-readable 输出格式优先于人类阅读格式

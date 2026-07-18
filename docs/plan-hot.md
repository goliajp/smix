# plan-hot — v2 到 C16：docs 机械收尾（死链清零 + llms.txt 生成 + roadmap/宪法同步）

> **单 checkpoint 判定 + 拆分旗标（先读）**：冷计划 C16 一行塞了**四类工作**——(1) 死链清零、(2) `llms.txt`/`llms-full.txt` 生成、(3) roadmap/宪法同步、(4) docs 重构（insight-\* 去留 / MCP 指南 / 版本化）。本段实测后判定：**前三类是「有机器 gate 的 doc 机械/编辑工作」，风险性质一致，装得下一个 checkpoint（3 step 线性）；第四类是「无干净 gate 的判断密集型策展」（37+3 个 consumer 文件的去留、docs IA 重排是取舍题，不是 pass/fail），风险性质不同。**
>
> **按本 cycle 既有拆分判据（C6/C7/C8/C9/C12/C14 全部因「风险性质不同」而拆，非工作量），C16 应拆**：本段 plan-of-record = **C16 = 前三类机械/编辑块**（死链 gate `hygiene-scan.py` rc=0、`llms-fresh` rc=0、roadmap 认 v2 + 宪法零悬空指针，全部机器可判）；**建议新增 C17 = 判断密集型策展**（insight-\* 40 文件去留 + docs IA 重排 + 版本化策略），原 ship 由 C17 顺延 **C18**。**拆分属用户权力（§10）——本段先按此 plan-of-record 推进机械块；若用户否决拆分、要求四块并一段，回退到「与冷计划不符 #5」列的合并形态。** brief 建议的边界（「gated mechanical vs judgment-heavy curation」）与此判定一致。

## 目标 checkpoint

C16：**smix 的公开 doc 面对外自洽——零死链、有随代码刷新的 AI 索引、roadmap/宪法说的是 v2 的真相。** 通过后世界：
- `python3 scripts/dev/hygiene-scan.py`（**全扫，非 `--noise-only`**）**rc=0**——当前 43 处死指针（21 在公开 `CHANGELOG.md`、22 在被跟踪的 `.claude/rfcs/*.md`，均指向 consumer 私有仓的 `smix-feedback-*.md`）清零。
- `llms.txt` + `llms-full.txt` 存在，且**由脚本从单一真源生成**（verb 表来自 `smix_verbs::VERB_TABLE`、selector taxonomy 来自 `Selector` enum、evergreen 指南拼接），`gen-llms.py --check` rc=0 证其未 stale，gate 焊进 `ship.sh`。
- `docs/roadmap.md` 的 Shipped 段含 v2.0.0 条目（六项破坏性变更 + SDK 一份 wire client 经 FFI 的再架构），不再停在 v1.0.4。
- `.claude/CLAUDE.md` 零悬空文件指针：`v3.md` / `design.md` 引用消除（按 v2.md 已拍板方向指向 `docs/v{cur}.md`），layer-[2] 指针从 `v1.md` 改真为 `v2.md`。

## 前置条件

```bash
git branch --show-current                                        # feature/v2.0
git status --short | grep -c .                                   # 期望 0（干净树；入场实测 0）
test -f docs/plan-history/v2-c15-hot.md && echo "C15 archived"   # 已归档（实测在）
test ! -e docs/plan-hot.md && echo "no stale hot plan"           # 注：本文件即新 plan-hot，前置检查针对生成前状态
# 六项破坏性变更 #1–#6 + SimctlError 改名全部落地（v2.md 决策日志 2026-07-18 C15 收尾确认）
# 本段只动 docs/ + 新增一个 python gen 脚本 + ship.sh 一行；不碰任何 crate 源码
pgrep -fl "runner.ts|smix run|supervise|bun test:e2e" ; echo "batch rc=$?"   # in-house batch 不活动（守规程，虽本段不起 sim）
```

## 已确证的起点（本次热化实测，file:line / 计数，非转述）

**Block 1 — 死链（`hygiene-scan.py` 全扫实测 rc=1）**：
- **43 dead doc pointer(s) in 13 file(s)**，拆分：**`CHANGELOG.md` 21** + **`.claude/rfcs/` 22**（6+2+2+2+2+2+1+1+1+1+1+1，跨 12 个 rfc 文件含 `README.md`）。均引用 consumer 私有仓 `smix-feedback-*.md`（不随 smix 发布 → 外部读者点不开 → 死链）。
- `plan-history/` 的死指针**已被排除**（归档计划按原样保留）——brief 的「excluding plan-history 后 43/13」实测吻合。
- **scan 已把 CHANGELOG / `.claude/rfcs/` 的 *noise* 标为「editorial pass, not a sweep」而豁免**（输出顶部 106 / 102 行 noise 不计），但 *dead pointer* 仍计为 fail。即：噪声已宽恕，死链未宽恕。
- **修法是编辑，不是删链**（brief 明示 + §13 质量优先）：公开 `CHANGELOG.md` 21 处**去 consumer 化**（把「Insight round-N 做了 X」叙事改写为「改了什么」）；`.claude/rfcs/` 22 处的处置**是本段唯一需拍板的编辑决策**（见「决策落地形态 D-1」）。

**Block 2 — llms.txt（实测不存在）**：
- `ls llms.txt` / `ls llms-full.txt` 均 **No such file or directory**——从零建。
- 真源就位：`crates/smix-verbs/src/lib.rs:116` `pub static VERB_TABLE: &[VerbEntry]`（`maestro_name` / `smix_name` / `category` 字段，reviewer invariant 已写在 Cargo.toml:9「any new verb must land in VERB_TABLE first」）；`crates/smix-selector/src/lib.rs:316` `pub enum Selector`（decision log 记 11 变体 = 6 base + 5 L4-L7 层）；`docs/ai-guide/verb-parity.md`（手写但「each row checked against the code」，非生成）；`.claude/CLAUDE.md` §9 七条不变量。
- **现存 gate 先例**：`scripts/dev/route-conformance.py`（正则读 Rust 源算路由）、`scripts/dev/ffi-bindings-fresh.sh`（重生成 + 逐字节 diff，stale 即 rc=1）。llms 生成 + freshness gate 与它们同形。

**Block 3 — roadmap / 宪法（实测都停在 v1，且比 brief/冷计划所说更陈旧或更悬空）**：
- `docs/roadmap.md` Shipped 段止于 **v1.0.4（2026-07-11，line 17-21）**，Next patch = v1.0.5；v2.0 只作为 **「Next major — v2.0（target Q4 2026 to Q1 2027）」未来地平线（line 79）**，把六项破坏性变更列为**未来 runway**。**roadmap 完全不知道 v2 已做完。**
  - **brief 说「stuck at v1.0.2」= 错**（实测 Shipped 到 v1.0.4）；**冷计划说「1.0.5→1.0.27」= 错**（roadmap 从未记 1.0.5 之后）。两处都与代码不符——本段以实测 v1.0.4 为准。
- `.claude/CLAUDE.md` 悬空指针（grep 实测）：
  - `v3.md`：**line 7**（doc 类型列表）+ **line 332**（`详 docs/v3.md 2026-05-27 §9 #3 lift 决策日志`）。`docs/v3.md` **从不存在**（v2.md 决策日志 2026-07-16 已标「dangling，C7 修指针」——C7 后变 SDK 手术，此修从未落，滚到 C16）。
  - `design.md`：**line 34**（四层目录契约图列 `design.md ← 设计决策（why）`）+ **line 50**（§10「设计决策改动 → 进 design.md」）。`docs/design.md` **从不存在**（v2.md 2026-07-17 决策日志已定方向：**不建 design.md，改契约指向 `docs/v{cur}.md`**）。
  - `v1.md`：**line 24/36/47/51** layer-[2] 文件指针 = `docs/v1.md（当前是 v1）`。`docs/v1.md` **从不存在**（docs/ 只有 `ai-guide/ plan-cold/ plan-history/ roadmap.md v2.md`，实测）；当前大版本是 v2，真文件是 `v2.md`。line 336 已用泛指 `v{cur}.md`、line 345 已承认「v2 cycle 起新建 docs/v2.md」——宪法内部自相矛盾。
  - **敏感项旗标**：line 332 同一句还含 §9#3「其他坐标 API（swipe_at_coord …）不授权」——此措辞与 **v2.md 2026-07-16 C1 已拍板「swipe_at_coord 授权为第二 native escape hatch，待用户确认后落」**冲突。**改宪法不变量属敏感操作**（v2.md 明记）→ 死链指针修（安全）与 §9#3 措辞同步（用户 gated）**分开**，见「决策落地形态 D-3」。

**Block 4 — docs 重构（判断密集，本段不做，拆给 C17；仅列实测供拆分决策）**：
- `docs/ai-guide/` 共 **57 文件**，其中 **consumer correspondence 40 个**（`insight-*` 37 + `gol-611-*` 3），evergreen 编号指南 `01-*.md`…`11-mcp.md`（11 个）+ `wire-format.md` / `abi-stability.md` / `verb-parity.md` / `activate-header-lifetime.md` + 子目录 `patches/` `schemas/`。40 个 consumer 文件污染 AI 面索引。
- **brief 的「real MCP setup guide (the stub)」= 陈旧前提**：`docs/ai-guide/11-mcp.md` 实测 **136 行**，含 setup / runner up / MCP client JSON config / 「one server process binds one simulator」——**是成篇指南，不是 stub**（大概率 C4「MCP 设置指南」已写）。C17 只需 review 其时效，不需从 stub 起写。

## 决策落地形态（§10 —— 动手时若与实测冲突须回报）

- **D-1〔`.claude/rfcs/` 22 处死链的处置——本段唯一开放编辑决策，flagged 给用户〕**：RFC 是**被跟踪的历史设计记录**，合法地引用当时的私有 feedback 文档。三条路：
  - **(a) 去 consumer 化**（同 CHANGELOG，把「Insight feedback X」改写为「设计动因是 Y」）——保 rfc 可读、消死链，但**改写历史设计记录**。
  - **(b) 降级为非链接引用**（删 markdown 链接语法 `[..](smix-feedback-*.md)`，保散文提及「per the 2026-07-10 feedback」）——最小改动、消死链、不改语义。
  - **(c) 把 `.claude/rfcs/` 视同 `plan-history/`**（归档设计记录，其死链按原样保留）——在 `hygiene-scan.py` 的 `EXCLUSIONS` 加一条**带理由**的排除（scan 每次运行仍打印其剩余命中数，不隐形收窄覆盖面——这正是 C1 立的原则）。
  - **推荐 (b)**：RFC 该保历史真相（反对 a 的改写），但死链确实点不开（反对 c 的「装作没看见」）；(b) 让引用**仍指向那次 feedback 的事实**而不假装文件可达，且 gate 真清零而非豁免。**CHANGELOG 21 处走 brief 已定的去 consumer 化（面向 crates.io 公开读者，语气须是产品变更日志，不是 dogfood 通信）。** rfcs 走 (b) 或 (c) 请用户拍；三条路都能让 `hygiene-scan.py` rc=0。
- **D-2〔llms 生成 = 脚本，非手写——推荐〕**：`scripts/dev/gen-llms.py`（python，镜像 `route-conformance.py` 的「正则读 Rust 源」）：正则读 `VERB_TABLE`（emit verb 表）+ `Selector` 变体（emit selector taxonomy）→ 填一段 prose 模板（capabilities / install / MCP setup / links）→ 写 `llms.txt`；`llms-full.txt` = 按**显式 include-list** 拼接 evergreen 指南（`01`–`11` + `wire-format` + `abi-stability` + `verb-parity`，**显式排除** `insight-*` / `gol-611-*`）。`--check` 模式重生成到临时文件、与签入版 diff，stale 即 rc=1（同 `ffi-bindings-fresh.sh`）。**理由**：手写 llms.txt 会随 VERB_TABLE 漂移——正是本 cycle 一路在修的病（「注释是主张，代码是事实」翻车 13+ 次）；生成 + freshness gate 让它成为**被强制的单一真源投影**，与本 cycle 主题一致，且「机器 gate 胜过散文」（method note）。**llms-full 的 include-list 与 Block 4 的 insight-\* 去留解耦**——生成器只认 include-list，不依赖 C17 对 40 个 consumer 文件的最终裁决。
- **D-3〔宪法修——死链指针 vs §9#3 措辞，分离〕**：本段**只修悬空文件指针**（已被 v2.md 决策日志拍板的方向：`v3.md`/`design.md` → `docs/v{cur}.md`；layer-[2] `v1.md` → `v2.md`）——这是**修破损引用使宪法内部自洽**，非改不变量。**§9#3 `swipe_at_coord` 「不授权」措辞的同步**（与 C1 已拍板授权冲突）是**独立的、用户 gated 的不变量措辞变更**，本段**不静默改**——在验收后 flag 给用户单独拍（与 v2.md 2026-07-16 C1「待用户确认后落」一致）。line 332 的 v3.md 死链指针可安全修（把「详 docs/v3.md …§9#3 lift 决策日志」的指向改到该 lift 决策的真实归属；若查无归属，改为「该 escape hatch 决策见 docs/v2.md 决策日志」并在 C17/独立轨补记该历史决策）。

## 步骤（线性，无分叉；3 个）

### S1. 死链清零：CHANGELOG 去 consumer 化 + rfcs 死链处置（gate: `hygiene-scan.py` 全扫 rc=0）

**红（写测试 = 机器 gate 先红）**
- 命令：`python3 scripts/dev/hygiene-scan.py >/dev/null 2>&1; echo $?` → 当前 **rc=1**（43 dead pointers）。这是本 step 的失败态锚。
- 记录起点分解：`grep "dead doc pointer" <(python3 scripts/dev/hygiene-scan.py 2>&1)` = `43 ... in 13 file(s)`（CHANGELOG 21 + rfcs 22）。

**绿（实现）**
- 文件：`CHANGELOG.md`——21 处引用 `smix-feedback-*.md` 的死链**去 consumer 化**：每条改写为「变更本身」（面向 crates.io 公开读者的产品变更日志语气），删除指向私有仓的 markdown 链接。
- 文件：`.claude/rfcs/*.md`（12 个，22 处）——按 **D-1 拍板路径**（推荐 (b) 降级为非链接引用；或 (c) `hygiene-scan.py` `EXCLUSIONS` 加带理由排除）处置。
- 关键点：**修法是编辑不是删信息**（§13）；rfcs 处置路径若取 (c) 则改的是 `hygiene-scan.py` 而非 rfc 内容，且新 exclusion 必须每次运行打印剩余命中数（不隐形收窄，C1 原则）。

**重构**
- 无（编辑工作，无结构可重构）。

### S2. `llms.txt` / `llms-full.txt` 生成脚本 + freshness gate（gate: 文件存在 + `gen-llms.py --check` rc=0）

**红（写测试）**
- 命令：`test -f llms.txt && test -f llms-full.txt; echo $?` → 当前 **rc=1**（都不存在）。
- 命令：`python3 scripts/dev/gen-llms.py --check` → 当前脚本不存在（红）。

**绿（实现）**
- 文件：`scripts/dev/gen-llms.py`（python，镜像 `route-conformance.py`）：
  - 正则读 `crates/smix-verbs/src/lib.rs` 的 `VERB_TABLE` → emit verb 表（maestro↔smix↔category）；读到 < N 条即判定表形状变、rc=1 报错（不许「什么都没读到」而绿——同 route-conformance / bindings-fresh 的形状校验）。
  - 正则读 `crates/smix-selector/src/lib.rs:316` `Selector` 变体 → emit selector taxonomy。
  - prose 模板：capabilities（§9 能力面）/ install（crates.io+npm+Maven+SwiftPM）/ MCP setup（引 `11-mcp.md`）/ links。
  - 写 `llms.txt`（简洁索引）；`llms-full.txt` = 按显式 evergreen include-list（`01`–`11` + `wire-format` + `abi-stability` + `verb-parity`）拼接，**显式排除** `insight-*` / `gol-611-*`。
  - `--check`：重生成到临时、与签入版逐字节 diff，异即 rc=1 + 指出哪个源漂了。
- 文件：`scripts/release/ship.sh`——加一行 `gen-llms.py --check`（stale 即挡 ship，与既有 hygiene/route/bindings-fresh gate 同处）。

**重构**
- 若 verb 表 emit 逻辑与 `route-conformance.py` 的 VERB_TABLE 正则可共享，抽一个小 helper（**仅在真重复时**，§8.1 不顺手扩范围）。

### S3. roadmap v2 同步 + 宪法悬空指针清零（gate: roadmap 认 v2 + `.claude/CLAUDE.md` 零 v3.md/design.md 指针）

**红（写测试）**
- 命令：`grep -c "v2\.0\.0" docs/roadmap.md`（Shipped 段）→ 当前 **0**（roadmap 无 v2 shipped 条目）。
- 命令：`grep -Ec "v3\.md|design\.md" .claude/CLAUDE.md` → 当前 **4**（v3.md ×2 line 7/332 + design.md ×2 line 34/50）。

**绿（实现）**
- 文件：`docs/roadmap.md`——Shipped 段加 **v2.0.0** 条目：六项破坏性变更（sessions 强制 / wire schema 协商 / `SMIX_*`→config / `Modifier`+`open_url` 合并 / `smix-recorder-ir`→`smix-authoring-ir` / VERB_TABLE freeze）+ **SDK 再架构（一份 wire client 经 FFI，Swift/Kotlin 调之、TS 待 napi）** + `SimctlError`→`DeviceControlError`。把 v2 从「Next major 地平线」移到「Shipped」。**以实测 v1.0.4 为 Shipped 上一条**（不采信 brief 的 1.0.2 / 冷计划的 1.0.27）。
- 文件：`.claude/CLAUDE.md`——按 **D-3**：line 7 的 `v3.md` → `v2.md`；line 34 目录图删 `design.md` 行（或改注「设计详解见 `.claude/design/v2.0/`」，与 v2.md:5 一致）；line 50 §10「进 design.md 决策日志」→「进 `docs/v{cur}.md` 决策日志」（与 line 345 既有表述统一）；line 24/36/47/51 layer-[2] 指针 `docs/v1.md（当前是 v1）` → `docs/v2.md（当前是 v2）`；line 332 的 `docs/v3.md …§9#3 lift 决策日志` 死指针重指真实归属。
- 关键点：**§9#3 `swipe_at_coord` 措辞不在本 step 改**（D-3，用户 gated 不变量，验收后单独 flag）。

**重构**
- 无。

## Checkpoint C16 验收

```bash
# 1. 死链清零（S1）—— 全扫，非 --noise-only
python3 scripts/dev/hygiene-scan.py >/tmp/c16-hyg.out 2>&1; echo "hygiene-full rc=$?"
grep -c "dead doc pointer" /tmp/c16-hyg.out   # 期望 0（无该行 → grep rc=1 亦可，看 rc 为准）
# 2. llms.txt 存在 + fresh（S2）
test -f llms.txt && test -f llms-full.txt; echo "llms-exist rc=$?"
python3 scripts/dev/gen-llms.py --check >/dev/null 2>&1; echo "llms-fresh rc=$?"
grep -c "gen-llms" scripts/release/ship.sh   # 期望 ≥1（gate 焊进 ship）
# 3. llms.txt 的 verb 表真来自 VERB_TABLE（抽验 tap/swipe 在 llms.txt 且 count 与源同尺）
grep -c "^| \`" llms.txt   # 期望 >0（verb 表行存在，非空模板）
# 4. roadmap 认 v2（S3）
grep -c "v2\.0\.0" docs/roadmap.md   # 期望 ≥1
# 5. 宪法零悬空文件指针（S3）
grep -Ec "v3\.md|design\.md" .claude/CLAUDE.md   # 期望 0
grep -c "docs/v1\.md" .claude/CLAUDE.md          # 期望 0（layer-[2] 已改真为 v2.md）
# 6. 无回归（本段不碰 crate 源 —— 只 docs + 一个 py 脚本 + ship.sh 一行）
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene-noise rc=$?"   # 期望 0（不回退）
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$?"                    # 期望 0（不碰 wire）
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"             # 期望 0
```

期望，逐条：
1. `hygiene-full rc=0`（43 → 0，dead-pointer 行消失）。
2. `llms-exist rc=0`、`llms-fresh rc=0`、`ship.sh` 含 `gen-llms` ≥1。
3. `llms.txt` verb 表行 >0（证生成器真注入了 VERB_TABLE，非空模板）。
4. roadmap `v2.0.0` 命中 ≥1（Shipped 段真有 v2 条目）。
5. 宪法 `v3.md|design.md` 命中 **0**；`docs/v1.md` 命中 **0**。
6. `hygiene-noise` / `route` / `bindings-fresh` **rc=0**（本段不碰 crate 源/wire/FFI，理应全不动）。**cargo / swift 本段零改动，不复跑**（与 C15 同理——无 Rust/Swift 改动不重跑）。

**仪器纪律**（本 cycle 反复吃亏，每条都是 v2.md 决策日志记过的实伤；本次热化我自己又在 hygiene-scan 上踩了一次 `| tail; echo $?` → 量到 `tail` 的 rc=0 而非脚本的 rc=1，重测才得真值）：
- **测退出码不接管道** —— `cmd | tail; echo $?` 量的是 `tail`。rc 单独 `>/dev/null 2>&1; echo "rc=$?"` 或落 `/tmp`。**本段验收全部照此写。**
- **gate/`grep -c` 报的是「命中/排版」不是「工作」** —— 确认 llms.txt 的 verb 行真来自 VERB_TABLE（S2 的 `--check` 才是真裁判），roadmap 的 v2 条目真描述六项 break（不是塞个字符串迎合 grep）。
- glob/正则必带引号，zsh 否则 `no matches found` 整条不执行（本次 `--include=*.rs` 已中招一次，改用 `git grep`）。
- **绿 ≠ 已做对**：`hygiene-full rc=0` 只证死链清零，**不证** CHANGELOG 改写质量（去 consumer 化读起来是否像产品日志）——那是编辑判断，gate 兜不住，收尾自陈。

## 未被本 checkpoint 覆盖的（写在明处）

1. **Block 4 判断密集型策展全部不做** —— 40 个 `insight-*`/`gol-611-*` consumer 文件的去留、docs IA 重排、`11-mcp.md` 时效 review、版本化策略：**建议成 C17**，无干净机器 gate（是取舍题）。**本段的 `llms-full.txt` include-list 已与之解耦**（显式排除 consumer 文件），故 C16 不阻塞、不依赖 C17。
2. **§9#3 `swipe_at_coord` 不变量措辞同步**（与 C1 已拍板授权冲突）—— 用户 gated 的宪法不变量变更，本段只修死指针不碰措辞（D-3），验收后单独 flag。
3. **`metroLog`/`fixturesRegistry` hint 的旧账**（C15 flag 结转）—— hint 现指 `config.yaml` 但该侧无对应 loader；是否改指真 `--metro-log-url` flag 或补 yaml reader，归 C17 docs 轨/独立决策，本段不扩范围（§8.1）。
4. **`.claude/rfcs/` 死链处置路径（D-1 a/b/c）需用户拍** —— 三条都能让 gate rc=0；本段 plan-of-record 取 (b)，若用户择 (a)/(c) 按其落。
5. **cargo-semver-checks**（证 v2 的 API 破坏）本机未装，属 C18 ship gate。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c16-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 **C17：docs 判断密集型策展** —— insight-* 40 文件移到 `docs/dogfood-archive/` + docs IA 重排。拆分已由用户 2026-07-18 拍板），见 CLAUDE.md §6。

## 与冷计划不符之处（必须先读，不要隐瞒）

1. **冷计划 C16「roadmap 1.0.5→1.0.27」= 陈旧** —— 实测 roadmap Shipped 止于 **v1.0.4**（2026-07-11），从未记 1.0.5 之后；v2 只在「Next major 地平线」。**brief 的「stuck at v1.0.2」同样错**（少数了两条）。本段以实测 v1.0.4 为准，roadmap 需**大改**（补整个 v1.0.5→v1.0.27 空档 + v2）而非「同步一个版本号」——**若用户希望 roadmap 也补齐 v1.0.5–v1.0.27 的 patch 史**（当前完全缺），是额外工作量，flag：本段 S3 只保证「Shipped 段认 v2.0.0 + 前一条是真实的 v1.0.4」，补 v1.0.5–27 patch 史属可选扩展，请用户定。
2. **冷计划 C16「~40 insight-\* 文件清理」= 判断题，本段不做** —— 实测 `docs/ai-guide/` 有 40 个 consumer 文件（insight 37 + gol-611 3）。删/归档/留是取舍，无机器 gate → 拆给 C17。
3. **冷计划 C16「45 处死链（CHANGELOG 21 + rfcs ~16）」的 rfcs 数偏低** —— 实测 rfcs 是 **22** 处（跨 12 文件），总 **43**（非 45）。brief 的「43/13」实测吻合，冷计划的「45 / rfcs ~16」偏差已更正。
4. **brief 的「real MCP setup guide (the stub)」= 陈旧前提** —— `11-mcp.md` 实测 136 行成篇指南（非 stub，C4 已写）。C17 只 review 时效，不从 stub 起写。
5. **C16 = 建议拆成机械块（本段）+ 判断块（C17）** —— 冷计划 C16 是四合一单行 scope；本段实测判定前三类（死链/llms/roadmap-宪法）风险性质一致且各有机器 gate、第四类（策展）判断密集无 gate，按本 cycle 既有拆分判据（C6/C7/C8/C9/C12/C14 全因「风险性质不同」拆）应拆。**拆分属用户权力（§10），本段未擅自合并四块**——plan-of-record 覆盖机械块，判断块 flag 给用户。若用户否决拆分，四块并一段将超出「1-3 step 线性」（策展的「删 vs 归档 40 文件」无法写成线性 gate 步），届时需先与用户敲定 Block 4 的每个取舍再线性化。

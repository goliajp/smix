# plan-hot — v2 到 C14：破坏性变更 #1 sessions 强制（去 iOS 隐式 no-session 驱动路径）

> **规模警告（先读，需用户拍板）**：冷计划把 C14 写成 **#1 sessions 强制 + #3 `SMIX_*`→config** 两项破坏性变更。实测后二者各自是一个 checkpoint 的体量、且**风险性质不同**（#1 = 行为/类型收紧；#3 = 从零建 config 子系统），塞不进「1-3 step 线性」。**本热计划只覆盖 #1**，`#3 → 新 C15`（docs/ship 顺延 C16/C17）。判据同 C6/C7/C8/C9/C12 五次拆分 —— 见文末「与冷计划不符之处」+ 报告。**用户可随时否决合并回一段。**

## 目标 checkpoint

C14：**iOS 驱动只能经一个 live session 进行** —— `smix run`（iOS）在 `/session/open` 失败时**响亮失败并退出**，不再静默降级到无 session 的「每请求 resolveApp + 限流 activate」旧路径；仓内所有 iOS 驱动入口（`smix run` / MCP / real-sim 测试 / recorder 生成样本）在动作前都持有 session；无 session 的 iOS 驱动在 SDK 边界变成**具名错误**而非静默旧路径。通过后世界：iOS 侧「隐式 no-session」这条路径在 Rust/CLI 表面**不存在**；Android 因 wire 本身无 `/session/*` 路由而保持 sessionless（记录在案的 carve-out，非 §9 违反）。

## 前置条件

```bash
git branch --show-current                                    # feature/v2.0
git status --short | grep -c .                               # 0（干净树）
# C13 已归档、#4 死类型已删
test -f docs/plan-history/v2-c13-hot.md && echo "C13 archived"
grep -rc "enum Modifier\b\|sealed interface Modifier\b" swift-bridge/Sources/SmixSDK/ android-runner/sdk/src/main/ 2>/dev/null | awk -F: '{s+=$2} END{print s}'  # 0
# in-house batch / gradle 不活动（本段只动 Rust，不必起 emulator，但仍守规程）
pgrep -fl "runner.ts|smix run|supervise|bun test:e2e" ; echo "batch rc=$?"
```

基线测试数（**取自 C13 close 决策日志实测，入场须复跑复核，不得凭记忆**）：cargo `132 ok / 0 failed`（`smix-ai-tier` 6 个 stub-CLI 测试偶发超时，非本段回归，见 v2.md 2026-07-18 C9 旁证）、swift `318/0`、route `rc=0`、clippy/hygiene/bindings-fresh `rc=0`。

## 已确证的起点（本次热化实测，file:line，非转述）

**#1 的操作性定义 = `AppHolder` 枚举 + 客户端 `session_id: Option`。** `smix-adapter-maestro/src/entry.rs:175-206`：

- `FlowPlatform::Ios`（:188）→ `app.open_session(...)` → `AppHolder::Session`。**但 open 失败时（:190-203）warn 一句后回落 `AppHolder::Loose(App::connect_to_runner(...))`** —— 一个 `session_id=None` 的裸 `App`，驱动请求**不带 `Session-Id` 头**，iOS runner 走 legacy 每请求 `resolveApp()` + 限流 activate（`smix-runner-client/src/lib.rs:557-561` 注释确证）。**这就是「隐式 no-session 驱动路径」的确切载体，且是活的、承重的 fallback，不是残迹。**
- `FlowPlatform::Android`（:205）→ **无条件 `AppHolder::Loose(app)`** —— Android **从不开 session**，永远 sessionless 驱动。

**决定性实测：Android runner 服务 0 个 `/session/*` 路由。** `android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt:88-116` 全 18 条路由（`/tree` `/tap-at-norm-coord` `/press-key` …）**没有一条是 session 路由**。session 是 **iOS 独有概念**（`/session/*` 只在 iOS runner，用于消除 XCUITest activation storm）。故「Option→required」**在 Android 上无对象** —— 它无 session 可要求。

**客户端 `session_id` 必须保持 `Option`。** `smix-runner-client/src/lib.rs:372` 字段 `Option<String>`，:431 默认 `None`，:632 仅在 `Some` 时发 `Session-Id` 头。它被三处合法置 `None`：(a) Android 全程；(b) `smix-sdk/src/lib.rs:689` `Session::close`、(c) `:700` `Session::drop`（App 生命周期长于 Session，close 后归还裸 App）。**故 dossier/brief 的「`Option<String>` → required 类型层强制」不可字面实现** —— 这是本段最重要的「brief 与代码相悖」。

**iOS 侧已有单个 verb 硬要求 session 的先例。** `smix-sdk/src/lib.rs:960-984` `clear_app_data_with_launch` 已在 `runner.session_id()` 为 `None` 时返回具名错误（「run `smix run`（auto-opens a session）or call `App::open_session` first」）。**#1 = 把这条「iOS 驱动 verb 需 session」的要求从一个 verb 推广到全部 + 删掉 entry 的 Loose fallback。** 收紧模式已存在，是推广不是发明。

**无 session 的 iOS `App` 生产者（真血缘半径）**：`App::connect_to_runner`（`smix-sdk/src/lib.rs:810`，返回 `session_id=None` 的裸 App）被调于 —— `entry.rs:129`（iOS 主路径，随后 open_session）、`entry.rs:195`（iOS Loose fallback）、`smix-mcp/src/main.rs:396`（**MCP 驱动面，C4；实测经裸 App 驱动 iOS，从不开 session**）、`smix-adapter-maestro/tests/real_sim_device_detail.rs:39`、`smix-recorder/src/generator_rust.rs:52`（生成样本文本）。`set_session_id`/`clear_session_id` 公开于 `runner-client:546/553`，经 driver trait（`smix-driver/src/{ios,android,traits}.rs`）与 SDK（lib.rs:689/700/791）转发。

**#1 的 codemod 无 YAML 对象。** 破坏性变更表（v2.md:33）记 #1 迁移 = 「codemod 包裹调用」。但 session 是 **wire/SDK 概念，无 YAML 表面**（maestro yaml 里没有 session 动词），`smix migrate`（YAML codemod）**无可转换项**。真实迁移 = 对 downstream Rust 调用方的**弃用说明**；而唯一消费者 insight 走 CLI（自动开 session），codemod 近乎空转。**又一处 brief/表格与代码相悖。**

## 决策 C14 必须做并记录（§10 —— 本段动手前请用户拍板）

- **D-a〔Android carve-out〕**：Android runner 无 `/session/*` 路由（实测 18 路由 0 session），「sessions 强制」**由 wire 物理决定为 iOS-scoped**。Android 驱动保持 sessionless，**记录在案，不读作逃逸的隐式路径**。
- **D-b〔"类型层强制"的可实现形态〕**：`HttpRunnerClient.session_id: Option<String>` **不可改成 required**（Android + Session close/drop 合法需 None）。可实现的收紧 = ①删 iOS Loose fallback（open 失败即致命）②在 SDK 边界给 iOS 驱动加 session 守卫（无 session → 具名错误，推广 lib.rs:974 既有先例）。**完全的「session-scoped 驱动类型」**（把 iOS 驱动方法移到 `Session` 类型上）会让共享的 `App` 按平台分叉（Android 直接在 App 上驱动），是大 reshape —— **是否走到这一步请用户定**；本段 plan-of-record 取 ①②，不做全 reshape。
- **D-c〔弃用 vs 硬删〕**：iOS Loose fallback **硬删 + 响亮失败 + upgrade hint**（推荐，与 C6 wire 协商的「无公共版本则响亮失败」同形），而非留一条带 warn 的降级路径。理由：fallback 正是 #1 要去掉的隐式路径，留着 warn 等于没去掉。

## 步骤（线性，无分叉）

### S1. `smix run` iOS：session-open 失败即致命，删 `AppHolder::Loose` 隐式回落

**红（写测试）**
- 文件：`crates/smix-adapter-maestro/tests/`（新测试，或在既有 entry 测试文件加）
- 断言：iOS 平台 + 一个 `/session/open` 返回失败的 mock runner → 取 iOS 驱动 handle 的路径返回 `Err`（结构化、含 hint），**不产出可继续驱动的 Loose handle**。当前红：`entry.rs:190-203` warn 后回落 Loose 并继续。
- 为可测，把 entry.rs 里「iOS 取驱动 handle」这段从 `run_flow` 抽成一个返回 `Result<AppHolder, _>` 的函数（iOS 分支不再有 Loose 臂），红测试打这个函数。

**绿（实现）**
- 文件：`crates/smix-adapter-maestro/src/entry.rs`
- 动作：iOS 分支（:188-204）删去 `AppHolder::Loose(fresh)` 回落，改为 `open_session` 失败即返回响亮错误 + hint（`runner 不支持 session / 太旧 — smix runner install --force`）+ 非零 ExitCode。**Android 分支（:205）不动** —— 记 D-a carve-out doc comment，写明「Android wire 无 `/session/*`，sessionless 是它的唯一路径，非隐式逃逸」。
- 关键点：这是 break，不是 bug fix —— 删的是承重的隐式 no-session 路径本身。

**重构**
- 若抽出的 handle 函数让 `AppHolder::Loose` 只剩 Android 一个构造点，收紧其可见性/注释以反映「Loose = Android-only」。

### S2. iOS 驱动边界守卫 + 所有仓内 iOS 驱动入口持 session

**红（写测试）**
- 文件：`crates/smix-sdk/tests/`（新）
- 断言：一个 iOS `App`（`connect_to_runner`，无 session）上调一个驱动动作（如 `tap`）→ 返回具名 `DriverError`（message 含 "session"，形如 lib.rs:974 既有 `clear_app_data` 守卫），**不静默走无头请求**。当前红：iOS 裸 App 驱动会静默走 legacy 路径。
- 文件：`crates/smix-mcp/`（测试或 grep 断言）
- 断言：MCP 的 iOS 驱动路径在动作前 `open_session`（否则 S2 守卫会让它响亮失败）。

**绿（实现）**
- 文件：`crates/smix-sdk/src/lib.rs`（iOS 驱动路径）+ `crates/smix-mcp/src/main.rs`
- 动作：(a) 在 iOS 驱动边界加 session 守卫 —— iOS 且 `session_id()` 为 `None` 时，驱动动作返回具名错误（推广既有 lib.rs:974 模式，不新发明文案风格）；Android 不受影响（其 driver 无此守卫）。(b) `smix-mcp/src/main.rs:396` 的 iOS 驱动改为 `connect_to_runner` 后 `open_session`。(c) `real_sim_device_detail.rs:39` 测试、`generator_rust.rs:52` 生成样本文本同步（样本是给读者的黄金路径，须示范 open_session）。
- 关键点：守卫在 iOS driver 一处落，自动覆盖所有 iOS 驱动调用方 —— 不逐个调用方打补丁（§12.2）。

**重构**
- `set_session_id`/`clear_session_id` 的 doc comment 若仍写「rarely called directly」等已过时表述，改为反映「session 是 iOS 驱动的前提」。不「顺便」碰 #3 的 `SMIX_*` / config（§8.1，归 C15）。

## Checkpoint C14 验收

```bash
# 1. iOS Loose 隐式回落已删（Android Loose 仍在，故不能全删 Loose）
grep -n "AppHolder::Loose(fresh)\|connect_to_runner(args.runner_port)" crates/smix-adapter-maestro/src/entry.rs | grep -c "fresh"   # 期望 0（iOS fallback 构造点消失）
# 2. iOS session-open 失败即致命（S1 新测试）
cargo test -p smix-adapter-maestro --test '*' 2>&1 | grep "^test result:" | tail -5
# 3. iOS 无 session 驱动 = 具名错误（S2 新测试）
cargo test -p smix-sdk 2>&1 | grep "^test result:" | tail -3
# 4. MCP iOS 驱动前开 session（grep 断言：main.rs 的 iOS 路径含 open_session）
grep -c "open_session" crates/smix-mcp/src/main.rs   # 期望 ≥1
# 5. 无回归（rc 单独取，不接管道）
cargo test --workspace >/tmp/c14.out 2>&1; echo "cargo rc=$?"
grep -c "^test result: ok" /tmp/c14.out; grep -c "^test result: FAILED" /tmp/c14.out
cargo clippy --workspace --all-targets >/dev/null 2>&1; echo "clippy rc=$?"
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$?"
( cd swift-bridge && swift test >/tmp/c14sw.out 2>&1; echo "swift rc=$?"; grep "Executed .* tests" /tmp/c14sw.out | tail -1 )
```

期望，逐条：
1. 计数 **0**（iOS 的 Loose fallback 构造点消失；Android 的 `AppHolder::Loose(app)` 不受影响）。
2. `test result: ok`、`0 failed`（含 S1 新测试）。
3. `test result: ok`、`0 failed`（含 S2 守卫测试）。
4. 计数 **≥1**（MCP iOS 路径开 session）。
5. cargo `rc=0`、`ok` **≥132**、`FAILED` 与基线一致（`smix-ai-tier` 6 stub 偶发超时非本段，见 v2.md C9 旁证）；clippy/hygiene/bindings-fresh/route **rc=0**（route rc=0 = SDK 手术收口不回退）；swift 那行读作 `Executed 318 tests … 0 failures`（**≥318 且 0 failures**，基线 318）。

**仪器纪律**（本 cycle 反复吃亏；每条都是 v2.md 决策日志记过的实伤）：
- **测退出码不接管道** —— `cmd | head; echo $?` 量的是 `head`（本 cycle 3 次）。rc 单独 `>/dev/null 2>&1; echo "rc=$?"` 或落 `/tmp`。
- **不在编译未完成时读测试输出** —— 曾读到假的 `exit=101 / 22 buckets`，真值 132/0。
- `swift test` 读 XCTest `Executed N tests` 行，不读 swift-testing `0 tests … passed` 行。
- `--include='*.rs'` / glob 必带引号，否则 zsh `no matches found` 整条不执行。
- **绿 ≠ 已测**：数从 `test result:` 报告取，不估。

**未被本 checkpoint 覆盖的**（写在明处）：
1. **#3 `SMIX_*`→config 完全不在本段** —— 已拆为 C15。故本段验收**不含**「4 个 `SMIX_*` 不再经 `env::var` 直读 / 经 config loader 读」。见「与冷计划不符」。
2. **无真设备证据** —— S1/S2 的测试全 mock runner；「iOS 必开 session」在真 sim 上的端到端行为属 C16 ship gate（`xcodebuild build-for-testing` + 四 SDK smoke）。mock 证明不了真设备（本 cycle C3/C4/C5/C6 同一教训）。
3. **完整「session-scoped 驱动类型」**（把 iOS 驱动方法移到 `Session` 类型）**未做** —— D-b 取守卫式收紧；全 reshape 是否做请用户拍（会平台分叉共享 `App`）。
4. **cargo-semver-checks**（证公开 API 收紧是 semver break）本机未装，属 C16 ship gate。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c14-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 **C15：#3 `SMIX_*`→config** —— 建统一 config loader + 消解 `.smix/config.json`（**实测从不被读，仅存在于 hint/doc 字面量**）与 `.smix/config.yaml`（interactiveProbe，唯一真实 reader，schemaless serde_norway）的裂缝 + 迁移 4 个行为开关 + env 具名弃用 warn + 保留 parse 时 thread-local 测试缝），见 CLAUDE.md §6。**前提：用户已拍板 C14/C15 拆分。**

## 与冷计划不符之处（必须先读，不要隐瞒）

1. **C14 = 两项破坏性变更，是一个 checkpoint 装不下的量** —— 冷计划 C14 行写 `#1 + #3`。实测 #1（iOS 驱动收紧 + Android carve-out 设计分叉 + 跨 runner-client/driver/sdk/mcp/adapter 血缘）与 #3（从零建 config 子系统 + env 弃用 + 测试缝）各自 checkpoint 量级、风险性质不同。**建议拆：C14=#1、C15=#3，docs/ship 顺延 C16/C17**（判据同 C6/C7/C8/C9/C12 —— 风险性质不同、一个 gate 只在最后响一次正是冷热分离要避免的）。**未自行落定，属用户权力（§10）；请拍板。**
2. **「`session_id: Option` → required 类型层强制」不可字面实现** —— 客户端 `Option` 必须保留（Android 全程 None + Session close/drop 归 None，实测 file:line 见起点）。dossier/brief 的这句是 **iOS 视角外推到不存在 session 的 Android**。可实现形态 = 删 iOS 隐式驱动路径 + 边界守卫（D-b）。
3. **「sessions 强制」由 wire 物理决定为 iOS-scoped** —— Android runner 服务 **0 个 `/session/*` 路由**（实测 RunnerTest.kt:88-116）。故对 Android 而言 sessionless 不是「隐式路径」而是**唯一路径**，「去隐式」在 Android 上无对象。冷计划/dossier 未区分平台。
4. **#1 的「codemod 包裹调用」无 YAML 对象** —— session 无 maestro yaml 表面，`smix migrate` 无可转换项；真实迁移是 Rust-API 弃用说明，且唯一消费者 insight 走自动开 session 的 CLI，codemod 近乎空转。
5. **冷计划 C14 行的 `#3` 前提「`.smix/config.json`（metroLog / fixturesRegistry）」与 `.smix/config.yaml` 并存需「消解裂缝」—— 但 `.smix/config.json` 实测从不被任何代码读取**（9 处 `config.json` 全在 doc/hint 字面量里，`metro_log_url` 来自 `--metro-log-url` CLI flag、`fixture_registry` 来自 `args.fixture_registry` 结构字段，均无 config.json loader）。故 #3 的「裂缝」半虚构：只有 yaml 有真 reader，json 是「文档承诺了但从未实现」的 config 表面（又一处「注释是主张」）。此点归 C15 处置，但**必须提前记账 —— 它改变 #3 的形状**。

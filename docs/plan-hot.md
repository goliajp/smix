# plan-hot — v2.9 到 C5：TS 经真 napi addon 驱动真 sim 一条 flow + 四 SDK 驱动 parity（napi 轴）闭合

## 目标 checkpoint

C5：**`npm/smix-rn` 的 TS SDK 经 `loadNodeDriver()` 载入的真 `@goliapkg/smix-node` addon，打真 `smix-runner-client` wire、到真 runner、到真 sim（iOS 26.5，显式 UDID），跑通一条真 flow —— `Smix.launchApp(bundleId('com.apple.Preferences'), runtime.resolver, { driver })` 在真 sim 上 launch，`app.snapshotTree()` 回真树，`app.tap(Selector.id('<stable com.apple.settings.* id>'))` 走 snapshot→resolve→`tapById` 在真 sim 上真发生并命中，`app.find(Selector.id('<drilled-in sub-screen stable id>')).toBeVisible()` 在导航后的真树上 resolve 成功。gate = 机器可判：一个 e2e 脚本退出码 0 + tap 后 sub-screen 节点 `toBeVisible` 断言通过（导航效果 = tap 真命中的证据，不依赖读图）。同时四 SDK 驱动 parity（napi 轴）正式闭合，由一道 device-free 源级 gate 锁死「TS 驱动面无残留 `'napi'` 桩」（Swift/Kotlin/Rust 早已能驱动，TS 是最后一条腿，C3/C4 已退桩、C5 用真 sim e2e 兑现 capability + 源级 gate 防回归）。C3 记的「两个 Session 并存」finding 读码定论 + 决策（已入 `docs/v2.md`）。** 全程零 publish；设备腿在 mini（iOS 26.5）跑，sim-guard 显式 UDID、batch-owner 让位、收尾 sim-sweep。不碰 screenshot/openUrl/launchFresh wire/host 缺口（独立后续）、不碰 v2.10+。

## 前置条件

**device-free 前置（任意 host 可跑）：**
```bash
test -f docs/plan-history/v2.9-c4-hot.md                                   # C4 热计划已归档
grep -q 'loadNodeDriver' npm/smix-rn/src/loadNodeDriver.ts                 # C4 真工厂在（e2e 经它载真 addon）
grep -q 'options?:' npm/smix-rn/src/Smix.ts                                # launchApp 现签名 (target, resolver, options?) —— e2e 传 { driver }
grep -q 'HttpSimRuntime' npm/smix-rn/src/HttpRunner.ts                     # 真 fetch resolver 在（/select/resolve，stateless stone）
! grep -q "SmixNotImplementedError('napi'" npm/smix-rn/src/App.ts          # C3 已退净 napi 桩（C5 源级 gate 的对象；grep 命中=退桩回归）
python3 scripts/dev/route-conformance.py                                   # 基线 rc=0（终端直读退出码，非管道）
```

**device 腿前置（在 mini / iOS 26.5 host，实现期实跑，规划期不跑）：**
```bash
# ① sim-guard 铁律：显式 UDID，禁 booted/占位符/all。实查专属 dev sim（名可能变、UDID 必变）：
xcrun simctl list devices | grep -iE 'sim-simx-001|iPhone 17 Pro'          # 取一台 iOS 26.5 sim 的显式 UDID；不接受 booted 通配
# ② batch-owner 让位：确认本机无他人活动 runner batch（含并发 .claude-profile-3 会话 / insight dogfood）
pgrep -fl 'runner.ts|smix run|supervise' || echo 'no active runner'        # 有 owner → 停手回报，不干扰（[[runner_ops_check_batch_owner_first]]）
xcrun simctl list devices booted                                           # 若 sim-insight 等他人 sim 已 Booted，只钉自己的显式 UDID，绝不动他人 sim
# ③ 真 addon 就位：darwin-arm64 `.node` 必须在本 host 构建过（C4 分发机制只搭、未 publish）
( cd crates/smix-node && bun run build ) && bun install                     # 本机产 darwin-arm64 .node + workspace symlink，供 loadNodeDriver 真载
command -v smix >/dev/null                                                  # 真 runner CLI 在 PATH（smix runner up / down）
```

## 已经查清、不必重查的事实

- **真 flow 用系统 app `com.apple.Preferences`，无自定义 fixture app 预装（读码核实）**：仓库无自定义 dev.smix iOS fixture app 装到 sim（`grep fixture` 命中的是 maestro `- fixture:` yaml 动词 + `FixtureRegistry`，与「装到 sim 的 app」无关）。v2.8-C5 20 流 corpus 全用 **`com.apple.Preferences`（Settings）+ locale-independent `com.apple.settings.*` stable a11y id**（Preferences tree dump 得，见 `docs/v2.md` 2026-07-23 C5 闭合条）。C5 复用同一范式：Preferences 是系统 app 恒在，**无需任何 app install**（sim-guard 面更干净）。具体 tap 目标 id（如 General 行）+ drilled-in sub-screen 断言 id 在**实现期由一次 Preferences `/tree` dump 取真值**，不在规划期臆造。

- **真 runner 起法（`smoke-v1.smoke.sh` 范式核实，C5 复用）**：`smix runner up <UDID> --bundle <bundle>`（§D8 硬要求 `--bundle`，不给会 loud fail）→ runner 监听默认端口 **22087**（`crates/smix-cli` `DEFAULT_RUNNER_PORT`，C4 核实）。收尾 `smix runner down`。C5 = `smix runner up <显式UDID> --bundle com.apple.Preferences`。`loadNodeDriver()` 默认端口 = `SMIX_RUNNER_PORT` env 否则 `22087` **恰匹配 runner up 默认端口**（无需显式传 port；e2e 可显式 `loadNodeDriver(22087)` 求稳）。

- **napi 客户端对 act/sense 动词是 context-less —— 靠 runner 的 `--bundle` 绑定寻的（读码核实，关键）**：`SmixNodeDriver::new(port)` → `HttpRunnerClient::new(port)`（`with_base`），构造出的 client **`target_bundle_id: None` / `auto_activate: false` / `session_id: None`**（`crates/smix-runner-client/src/lib.rs:565-568`）。`apply_context`（:764）据此**不发** `App-Bundle-Id`/`App-Activate`/`Session-Id` 任一头。故 napi 的 `snapshotTree`/`tapById`/`inputText`/`pressKey`/`swipe`/`systemPopups` 全走 runner 的**默认绑定 app**（即 `runner up --bundle` 绑的那个）。C5 e2e 因此**必须** `runner up --bundle com.apple.Preferences`，让 runner 绑定权威指向 Preferences；`session.launchApp()`（napi `openSession('com.apple.Preferences')` → `/session/launch-app`，session_id 在 body）也 launch Preferences —— 二者指向同一 app，一致。**这是 e2e 能通的前提，也是设备腿要实证的第一件事**（实现期 e2e 第一步 `/tree` 应回 Preferences 树，非空、非他 app）。

- **「两个 Session 并存」finding —— 读码定论 = 无 Session-Id「打架」，真限制在 napi act/sense 无 session 亲和**（决策已入 `docs/v2.md` 2026-07-24，**C5 内不解决、记为已知限制单列后续**）：
  - **fetch-side `Session`（`Session.ts` / `HttpSimRuntime`）**：TS/`fetch` 客户端，`/session/open` 得 session_id_B 后置 `Session-Id` 头于后续 `fetch` 请求。C5 的 resolver = `HttpSimRuntime.resolver` 打 `/select/resolve`（`HttpRunner.ts:70`），而 `/select/resolve` 是**无状态 stone 计算**（对调用方传入的 `treeJson` resolve `selectorJson`），**不需 session / 不需 XCUIApplication 绑定**。故 **C5 e2e 根本不 open fetch-side `Session`** —— resolver 无状态，直用 `new HttpSimRuntime('http://127.0.0.1:22087').resolver`。
  - **napi-side `NodeSession`（smix-node）**：Rust `HttpRunnerClient`，`open_session` 得 session_id_A **只存进 `SmixNodeSession` 结构、只在 launch/terminate/relaunch 的 request BODY 里带**（`crates/smix-node/src/lib.rs:129-193`），**从不调 `client.set_session_id()`** → napi client 的 `Session-Id` 头恒 `None`，act/sense 全走前一条说的 context-less 路径。
  - **结论**：两者是**两条独立 HTTP 客户端**，永不共享同一 `Session-Id` 头、永不互相覆盖 —— **无字面「打架」**。C5 flow 里只有一条 napi session（仅生命周期用），fetch resolver 无状态，act/sense 靠 runner `--bundle` 绑定寻的 → **零 Session-Id 竞争**。真正的架构缺口是：napi 的 act/sense 动词**不携带 session 亲和**，长 flow 上会退回 per-request rebind（`Session.ts` 顶注所述「activation storm」，runner 侧 2s 限流 + XCTest 仲裁压力）；且**同型缺口在 UniFFI `smix-ffi/driving.rs`**（同样一个 `HttpRunnerClient`、session_id 只进 body）—— 是 **napi+UniFFI 跨 SDK 的 session-亲和** 项，非 TS 独有、非 C5-local。C5 e2e 是**短 flow**（launch→snapshot→tap→assert），per-request 路径可承受，flow 能通；把 session 亲和塞进 C5 = 膨胀 checkpoint + 混入跨 SDK wire 重构 → **拒，记为后续 checkpoint**（与 screenshot/openUrl wire 缺口同 bucket，§13 质量/架构 clean >> 成本）。

- **四 SDK 驱动 parity（napi 轴）闭合怎么表达（读码核实现有 gate + 缺口）**：
  - **现有 parity gate**（全绿、非本段新增）：`crates/smix-error/tests/sdk_failure_code_parity.rs`（四 SDK failure 词汇表一致，Rust 为源）、`sdk_readme_api_exists.rs`（README 点名符号在源存在）、`scripts/dev/route-conformance.py`（无源调用未服务路由）。**这些是 vocabulary / README / route 层 parity，不是「四 SDK 各驱动同一 flow」**。
  - **无「四 SDK 驱动同一 flow」机械 gate，且加一个完整的需在一个测试里跑起四种语言运行时对真设备** —— 超出 C5（C5 只兑现 TS 这条腿的设备 e2e）。故 C5 **不**造那种大 harness。
  - **capability 闭合的真相**：Swift/Kotlin（UniFFI）+ Rust（`smix-runner-client` 直用）**早已能驱动**（v2.9 前既有）；**TS 是唯一缺的腿**，C3 退桩 + C4 真工厂 + **C5 真 sim e2e** 兑现之。机械表达 = **① device-free 源级 gate 锁死「TS 驱动面无残留 `'napi'` 桩」**（`App.ts` + `Smix.ts` 含零 `SmixNotImplementedError('napi'`，防退桩回归；`sdk_failure_code_parity` 同源手法：`include_str!` TS 源 + 断言 + extractor-can-fail 自测）**② TS 真 sim e2e 通过**（capability 证据）。保留的 `'wire'`/`'host'` 桩（screenshot/openUrl/launchFresh）**不算 napi-轴回归**（C3 决策已定，且 Swift `App.swift` 本就无这三方法 —— parity 参照里不存在，`sdk_readme_api_exists` 已守）。**完整的跨 SDK live-flow parity harness（四 SDK 跑同一 corpus flow）= 记为后续**，不在 C5。

- **e2e harness 落位（`smoke-v1.smoke.sh` 范式，机器可判）**：新建 `scripts/release/ts-driving-e2e.sh`（bash，`set -euo pipefail`，退 0=绿）：preflight（`smix`/`xcrun` 在、显式 UDID 必给否则 fail、batch-owner 检查）→ `smix runner up <UDID> --bundle com.apple.Preferences` → 跑 TS e2e entry → 断言其退 0 → **`scripts/dev/simx-sweep.sh` 收尾（永不 shutdown all）**。TS e2e entry = `npm/smix-rn/e2e/drive-preferences.mjs`（Node ESM，import 编译后 `dist/` 或经 `bun` 直跑 src；用真 `HttpSimRuntime.resolver` + `loadNodeDriver(22087)` + `Smix.launchApp` + `app.tap` + `Locator.toBeVisible`；成功打 `TS-DRIVE-E2E-PASS` 退 0，任一步抛则非 0）。**不进 vitest 常规套件**（那是 device-free 单测面；device e2e 由 env `SMIX_E2E_UDID` 显式门控、独立脚本跑，CI 常规不触发）。

- **`Locator.toBeVisible` 作机器 gate（读码核实可判）**：`app.find(sel).toBeVisible()` 轮询 `app.snapshotTree()`（经真 napi /tree）→ resolver resolve → 命中即 resolve、超时抛 `ExpectationFailure{TIMEOUT/NOT_VISIBLE}`。tap General 行后，drilled-in sub-screen 的 stable id `toBeVisible` 通过 = **tap 真命中 + 导航真发生**的机器可判证据（不读图、locale-independent）。

## 步骤（线性，2 个）

### S1. device-free 源级 gate：锁死「TS 驱动面无残留 `'napi'` 桩」（napi-轴 parity 闭合防回归）

**红（写测试，先失败一次）**
- 文件：`crates/smix-error/tests/sdk_driving_parity.rs`（新建，`include_str!` TS 源，同 `sdk_failure_code_parity.rs` 手法/同 crate）。
- 断言：
  1. `no_napi_stub_remains_in_ts_driving_surface`：`include_str!("../../../npm/smix-rn/src/App.ts")` 与 `Smix.ts` 里 **`SmixNotImplementedError('napi'` 出现次数 == 0**（napi 轴退桩闭合、防回归）。
  2. `ts_still_declares_the_wired_driving_verbs`：`App.ts` 源含九个已 wire 驱动动词方法名（`tap`/`fill`/`pressKey`/`swipe`/`tapAtCoord`/`terminate`/`relaunch`/`snapshotTree`/`systemPopups`）各出现，`Smix.ts` 含入口 `launchApp` —— 断言「退桩没把方法删空」（防「gate 靠一无所知而通过」）。
  3. `an_injected_napi_stub_is_caught`（extractor-can-fail 自测，镜像 `sdk_failure_code_parity::an_emptied_declaration_is_caught`）：对一段**含** `SmixNotImplementedError('napi', 'App.tap')` 的 fixture 串跑同一计数逻辑 → `catch_unwind` 断言 panic（证 gate 真能抓到 napi 桩，非恒绿）。
- 跑：`cargo test -p smix-error --test sdk_driving_parity` → 期望**红**（文件不存在 → 编译/测试失败）。

**绿（实现，最少代码转绿）**
- 落 `crates/smix-error/tests/sdk_driving_parity.rs`：三测如上；计数用一个 `count_napi_stubs(src) -> usize` 小 helper（`src.matches("SmixNotImplementedError('napi'").count()`），断言 helper 复用于主测与自测。
- 关键点：① 单一真源 = TS 源本身，不手抄第二份清单（吸取 `docs/v2.md` 2026-07-23 C14-pre 两 SDK 硬编码清单教训）；② 与既有 3 道 parity gate 同 crate 同风格，正统隔离。
- 跑：`cargo test -p smix-error --test sdk_driving_parity` → 期望**绿**（3 测过）。

**重构（可选）**
- 无。

### S2. TS 经真 addon 驱动真 sim 一条 flow：`ts-driving-e2e.sh` + `drive-preferences.mjs`（设备腿，mini）

**红（写测试，先失败一次）**
- 文件：`npm/smix-rn/e2e/drive-preferences.mjs`（新建，Node ESM e2e entry）+ `scripts/release/ts-driving-e2e.sh`（新建，bash harness）。
- 断言（entry 内，全经真 `.node` + 真 runner + 真 sim）：
  1. `const runtime = new HttpSimRuntime('http://127.0.0.1:22087')`；`const driver = await loadNodeDriver(22087)`（真 addon）。
  2. `const tree0 = JSON.parse(await driver.snapshotTree())` 非空且是 Preferences 树（`rawType` 顶层 application；证 runner `--bundle` 绑定权威指向 Preferences —— 已查清风险点的实证）。
  3. `const app = await Smix.launchApp(bundleId('com.apple.Preferences'), runtime.resolver, { driver })` 得 `App` 实例（napi openSession→launchApp 在真 sim 上真发生）。
  4. `await app.tap(Selector.id('<Preferences General 行 stable id>'))` 不抛（snapshot→`/select/resolve`→真 `/tap-by-id` 命中）。
  5. `await app.find(Selector.id('<General sub-screen stable id>')).toBeVisible()` 通过（导航后真树上 resolve；tap 真命中的机器可判证据）。
  6. 全通 → `console.log('TS-DRIVE-E2E-PASS')` 退 0；任一步抛 → 非 0（Node 默认 unhandled rejection 退非 0）。
- 红的观察方式（实现期）：先跑 entry **不起 runner**（或对错端口）→ 真 addon `snapshotTree` transport error（connection refused）→ entry 非 0、无 `TS-DRIVE-E2E-PASS`。证 e2e 真打 wire、非自证。
- 跑：`node npm/smix-rn/e2e/drive-preferences.mjs`（无 runner）→ 期望**红**（非 0，transport 报错）。

**绿（实现，起真 sim + runner 后转绿）**
- 实现 `scripts/release/ts-driving-e2e.sh`（镜像 `smoke-v1.smoke.sh`）：
  - preflight：`command -v smix`/`xcrun`；`SMIX_E2E_UDID` 必给否则 `fail`（**不 fallback booted/all** —— sim-guard 铁律，[[simx_sim_guard_hook]]）；`pgrep -fl 'runner.ts|smix run|supervise'` 有 owner 则 `fail` 让位（[[runner_ops_check_batch_owner_first]]）。
  - `smix runner up "$SMIX_E2E_UDID" --bundle com.apple.Preferences`。
  - 实现期先对该 sim dump 一次 Preferences `/tree` 取 General 行 + sub-screen 的 stable `com.apple.settings.*` id，填进 entry（不臆造）。
  - `node npm/smix-rn/e2e/drive-preferences.mjs`（经 workspace 解析真 `@goliapkg/smix-node`；`SMIX_RUNNER_PORT` 默认 22087 匹配）→ 断言退 0 + 输出含 `TS-DRIVE-E2E-PASS`。
  - **收尾 `bash scripts/dev/simx-sweep.sh`（一键全清、永不 shutdown all，[[simx_teardown_sweep_discipline]]）+ `smix runner down`**（即便中途 fail 也走 trap 清理）。
- 实现 `drive-preferences.mjs`（断言如红）。
- 关键点：① 只钉显式 UDID、只 sweep 自己的 sim；② 真 resolver 无状态、不 open fetch-side Session（两 Session finding 的 C5 落法）；③ 设备腿在 **mini（iOS 26.5）** 跑，`.node` 本 host 已 build。
- 跑（mini 上）：`SMIX_E2E_UDID=<实查UDID> bash scripts/release/ts-driving-e2e.sh` → 期望**绿**（打 `TS-DRIVE-E2E-PASS` + 脚本退 0）。

**重构（可选）**
- 若 preflight/sweep 与 `smoke-v1.smoke.sh` 重复，抽 `scripts/release/_e2e-lib.sh` 共用；不改断言。

## Checkpoint C5 验收

**device-free 部分（任意 host，先过）：**
```bash
cd /Users/doracawl/workspace/goliajp/smix \
  && python3 scripts/dev/route-conformance.py \
  && cargo test -p smix-error --test sdk_driving_parity \
  && ( cd npm/smix-rn && bun run typecheck && bun run test ) \
  && echo C5-DEVICE-FREE-PASS
```
期望：stdout 末尾 `C5-DEVICE-FREE-PASS`，exit 0。含义（`&&` 链任一非零即断）：route-conformance rc=0（parity 基线守住）；`sdk_driving_parity` 三测绿（TS 驱动面零残留 `'napi'` 桩 + 九动词在 + extractor 能抓桩）；smix-rn typecheck + vitest 全绿（C3/C4 面无回归）。

**device 腿（mini / iOS 26.5，实查显式 UDID，batch-owner 让位）：**
```bash
cd /Users/doracawl/workspace/goliajp/smix \
  && ( cd crates/smix-node && bun run build ) && bun install \
  && SMIX_E2E_UDID=<xcrun simctl list devices 实查的显式 UDID> \
       bash scripts/release/ts-driving-e2e.sh \
  && echo C5-DEVICE-PASS
```
期望：stdout 含 `TS-DRIVE-E2E-PASS` 与末尾 `C5-DEVICE-PASS`，exit 0。含义：真 darwin-arm64 `.node` 本 host 构建 + workspace symlink；`ts-driving-e2e.sh` 起真 runner（Preferences 绑定）+ 真 sim，`drive-preferences.mjs` 经真 addon → 真 wire → 真 sim 完成 launch/snapshot/tap，tap 后 sub-screen `toBeVisible` 通过（tap 真命中的机器可判证据）；收尾 sim-sweep + runner down。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.9-c5-hot.md`。
2. 「两 Session finding 决策（记为已知限制、单列后续）」+「四 SDK 驱动 parity（napi 轴）闭合表达（源级 gate + 设备 e2e，完整跨 SDK live-flow harness 顺延）」+「C5 fixture = `com.apple.Preferences` 系统 app、无自定义 install」已在本段执行前写入 `docs/v2.md` 决策日志（2026-07-24 两行）；无需重复。
3. v2.9 收官：`App.ts` 零 `'napi'` 桩、route-conformance rc=0、四 parity gate 绿、TS 真 sim e2e 与 Swift/Kotlin driving 行为 parity、跨 triple `.node` 预构建机制在（C4）。**发布仍顺延**（随 v2.9–v2.12 全完 + 用户显式授权；ship.sh 的 smix-node prebuild+prepublish DAG 待授权时接线，C5 零 publish）。
4. 调 sub-agent 热化 **v2.10**（下一 minor；冷计划入口条件届时验），见 CLAUDE.md §6。后续 checkpoint bucket 里挂着两个已记的 wire/亲和缺口：① screenshot/openUrl（runner 双端 wire 路由）+ launchFresh（host 侧清态/安装）；② napi+UniFFI act/sense 的 session 亲和（长 flow 防 activation storm）—— 两者都是「补 core/wire 能力」而非 SDK 补丁（§12.2 / §13）。

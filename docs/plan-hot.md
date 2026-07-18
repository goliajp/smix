# plan-hot — v2 到 C18：ship-readiness（版本 lockstep 2.0.0 + 两道新 ship gate + Swift 6 就绪度评估）

> **本 checkpoint 的第一性约束（先读，贯穿 目标 / 步骤 / 验收）：C18 是 ship-READINESS，不是 publishing。**
> 通过 = 仓库达到「可以 ship」的状态、每一道 gate 全绿；**不是**任何东西被发布。
> **`git tag` / `cargo publish` / `npm publish` / `gradle publish` 全部不在本 checkpoint 内** —— 用户既定硬规则「最后 tag 和 pub 先不做」。本段不含、不计划、验收也不触发任何发布路径；tag + publish 待用户单独 go-ahead。
>
> **单 checkpoint 判定**：本段四类工作（版本 lockstep / cargo-semver-checks / xcodebuild gate / Swift 6 评估）都是「ship gate 加固 + 版本对齐」，共享同一出口「仓库 ship-ready、gate 全绿」，装得下一个 checkpoint（3 step 线性）。**唯一被降级为 assess-only 的是 Swift 6**：UITest target 有 63 处 async-context `lock/unlock`，在 Swift 6 语言模式下成 error，真修是 actor 隔离重设计的兔子洞 —— C18 **只评估并记录**，不修（详见 S3 与「决策落地形态 D-3」）。两处「比 brief 大」的发现见文末「与 brief / 冷计划不符」。

## 目标 checkpoint

C18：**smix 仓库达到 v2.0.0 可发布状态 —— 四个 SDK 版本 lockstep 到 2.0.0、cargo-semver-checks 确认破坏是 semver-major、ship gate 现在会编译真正分发给用户的 runner 主体（`SmixRunnerUITests`）、Swift 6 就绪度有书面结论。** 通过后世界：
- workspace + npm + Kotlin runner + gradle SDK + README + 生成的 llms.txt 全部报 **2.0.0**，且 workspace 内 78 处跨 crate 版本约束不再卡在 `^1.x`（否则 2.0.0 一发布即 pull 回 1.0.x 兄弟 crate）。
- `scripts/release/ship.sh` 多两道 gate：`cargo semver-checks`（证六项破坏是 major，rc=0 = 2.0.0 的 major bump 足以覆盖）+ `xcodebuild build-for-testing -scheme SmixRunner`（编译 `SmixRunnerUITests` —— 此前只 `swift test`、这个真分发的 runner 主体从不进门禁）。
- `docs/v2.md` 决策日志有一条 **Swift 6 就绪度**结论：v2.0.0 以 Swift 5 语言模式发布、Swift 6 迁移的具体阻塞项（63 处 `lock/unlock`）与其归属版本已记录在案。
- publish DAG 完整：`smix-ai-tier`（被已发布的 `smix-adapter-maestro` 依赖）进 ship.sh 发布名单（实测发现的真 blocker，brief 未列）。
- **仓库 ship-ready，但没有 tag、没有 publish** —— 那一步是用户在本 checkpoint 通过之后单独拍的。

## 前置条件

```bash
git branch --show-current                                        # feature/v2.0
git status --short | grep -c .                                   # 期望 0（干净树；入场实测 0）
test -f docs/plan-history/v2-c16-hot.md && echo "C16 archived"   # 已归档（实测在）
test ! -e docs/plan-hot.md && echo "no stale hot plan"           # 注：本文件即新 plan-hot，前置针对生成前状态；C17 无独立热计划（v2.md 2026-07-18 已记账契约空档，C18 起草即恢复）
# 六项破坏性变更 #1–#6 + SimctlError→DeviceControlError 全部落地（v2.md 决策日志 C6/C12/C13/C14/C15 收尾确认）
# 本段动：Cargo.toml/各 crate Cargo.toml 版本 req/npm-kotlin-gradle-README 版本串/ship.sh/docs/v2.md；重生成 llms.txt。不碰任何 crate 业务逻辑源码。
pgrep -fl "runner.ts|smix run|supervise|bun test:e2e" ; echo "batch rc=$?"   # in-house batch 不活动（守规程，虽本段不起 sim）
```

## 已确证的起点（本次热化实测，file:line / 计数，非转述）

**Block 1 — 版本面（比 brief 大：不是 ~5 个串，是 5 + 78 跨 crate 约束）**：
- `Cargo.toml:8` `version = "1.0.27"`，位于 `[workspace.package]`；**29 个 crate 全部 `version.workspace = true`**（实测计数 29），改这一行即级联全 crate 版本。
- **78 处跨 crate 路径依赖版本约束卡在 `^1.x`**：`grep 'path = "../..." version = "1.x"'` 实测 **76 处 `version = "1.0.0"` + 2 处 `version = "1.0.3"`**，散在 **19 个 crate 的 Cargo.toml**。`^1.0.0` = `>=1.0.0,<2.0.0` —— **不满足 2.0.0**。若不改：`cargo publish` 的 smix-cli@2.0.0 会 pull 满足 `^1.0.0` 的**旧** smix-error 1.0.27（crates.io 上仍在），产出一个依赖 1.0.x 兄弟 crate 的破碎 2.0.0 release。这是**只在 publish 才炸、`cargo build` 看不见**（路径依赖本地按 path 解析）的 latent 失败 —— 正是 ship-readiness 必须现在堵的洞。**gate = 这类残留 req 计数归 0。**
- 四个非 Rust 版本串（ship.sh:85-109 逐一 gate）：`npm/smix-rn/package.json:3` `"1.0.27"` · `android-runner/app/src/main/kotlin/dev/smix/runner/SmixRunner.kt:13` `const val VERSION = "1.0.27"` · `android-runner/sdk/build.gradle.kts:119` `mavenCentralVersion = "1.0.27"` · `README.md:36` `# implementation("jp.golia.smix:smix-sdk:1.0.27")`（注释行，但 ship.sh:107 照 grep 它）。
- `llms.txt:90` / `llms-full.txt` 的 gradle 坐标是**生成物**：`gen-llms.py:83 workspace_version()` 读 `Cargo.toml`、`:206` emit 坐标。**不手改** —— S1 bump 后重跑 `gen-llms.py` 即随 2.0.0 刷新；`--check` 在 bump 后、regen 前会红（本 cycle C16 焊的 freshness gate）。
- **版本串的角色决定它动不动（code is truth）**：release 版本串移；`docs/ai-guide/{02-yaml-reference:40,verb-parity:78,wire-format:58}` 的 `v1.0.27`、`roadmap.md:22` 的「v1.0.5→v1.0.27」、`v2.md` 多处、`llms-full.txt:165/2637/2898` 是**「特性落在哪一版」的历史标注**，**留不动**。gate 因此不能用 `git grep -c 1.0.27 == 0`（会误伤历史标注），必须精确打 release 串（见验收）。

**Block 2 — cargo-semver-checks（实测未装）**：
- `which cargo-semver-checks` **rc=1（not found）**；`cargo semver-checks --version` = `no such command`。**本机没有此工具** —— S2 必须先 `cargo install cargo-semver-checks --locked`（编译耗时，§13 成本不计）。
- ship.sh **无 semver gate**（`grep semver ship.sh` rc=1）。
- 机制预判（执行时核）：bump 到 2.0.0 后 `cargo semver-checks` 对每个 crate 比 current(2.0.0) vs crates.io baseline(1.0.27)。有 in-place break 的（`smix-simctl`：`SimctlError`→`DeviceControlError`）会被检出为 major break，而 1.0.27→2.0.0 是 **major bump → 足以覆盖 → rc=0（PASS）**。**gate = rc=0**（major bump adequate）。破坏被检出并列在输出里 = 对「破坏是 major」的验证（执行时把该列表记进 semver log）。
- **工具盲区，必须诚实标注**：`smix-authoring-ir`（从 `smix-recorder-ir` 改名，#5）在 crates.io **无新名 baseline** → semver-checks 判「new crate、skip」，**看不见**这次 rename break；旧 `smix-recorder-ir` 已从 workspace 消失 → 也不被 check。改名破坏（#5）与 `smix-ai-tier`（C2 新 crate）**都不在 semver-checks 的可比范围**。别指望工具「证明」rename —— 它只证 in-place API 破坏。

**Block 3 — xcodebuild gate（实测缺；scheme 名已核）**：
- ship.sh:54 **只跑 `swift test`**（SPM，覆盖 `SmixRunnerCore`/`SmixSDK` 等），`grep build-for-testing ship.sh` rc=1（无）。`SmixRunnerUITests` 是 xcodegen target（`swift-bridge/project.yml`）、不是 SPM target → **真正分发给用户、在用户机 `smix runner up` 才 `xcodebuild` 的 runner 主体从不进门禁**（v2.md:70 C1 已记，指定 C8 补、实际滚到 C18）。
- **scheme 名 = `SmixRunner`**（`project.yml:50` `schemes: SmixRunner:`，build targets `SmixRunner: all` + `SmixRunnerUITests: [test]`；shared scheme 实测在 `swift-bridge/SmixRunner.xcodeproj/xcshareddata/xcschemes/SmixRunner.xcscheme`）。冷计划/决策日志的 `-scheme SmixRunner` **正确**。
- `xcodegen` 实测已装（`/opt/homebrew/bin/xcodegen`）；`xcodebuild` = Xcode 26.6。签入的 `.xcodeproj` 由 `project.yml` 生成 —— gate 前 `xcodegen generate` 一次（从单一真源 project.yml 刷新 xcodeproj，同 ffi-bindings-fresh「regen 再用」的门禁范式），再 `build-for-testing`。决策日志(:70)注「本 cycle 已实测可跑通、无需启动模拟器」（`-destination 'generic/platform=iOS Simulator'`）。

**Block 4 — Swift 6 就绪度（实测：Swift 5 模式 + 63 处 lock/unlock）**：
- `swift-bridge/Package.swift:1` `swift-tools-version: 5.9` → SPM 侧当前 **Swift 5 语言模式**；本机 toolchain 是 **Apple Swift 6.3.3**（能选 Swift 6 模式，但没选）。
- `SmixRunnerUITests/` 实测 **63 处 `.lock()`/`.unlock()`**，主集中在 `EventRecorder.swift`（:96-99,267-304,457-460,638-641,735-738 等）+ `SmixRunnerUITests.swift:214`。Swift 6 严格并发下 async 上下文里裸 `NSLock` lock/unlock 成 error（Sendable / actor 隔离）。**这是评估对象，不是修复对象**（S3）。

**Block 5 — publish DAG 完整性（实测发现真 blocker，brief 未列）**：
- `smix-ai-tier`（C2 新增，**publishable**、无 `publish = false`）被 **`smix-adapter-maestro`** 依赖（`crates/smix-adapter-maestro/Cargo.toml:30` `smix-ai-tier = { path=..., version="1.0.0" }`，fence test 依赖）。而 `smix-adapter-maestro` **在** ship.sh CRATES 名单（:122）、`smix-ai-tier` **不在**（`grep -c smix-ai-tier ship.sh` = 0）。→ `cargo publish -p smix-adapter-maestro` 会因 `smix-ai-tier` 未发布而 **失败**。**真 ship blocker，S1 一并堵**（加进 CRATES，DAG 序放在 smix-adapter-maestro 前）。
- 另有 4 个 publishable-但-孤儿 crate（`smix-core` / `smix-server` / `smix-ffi` / `smix-core-conformance`），实测**无任何已发布 crate 依赖它们**（`smix-ffi` 只被同样未发布的 `smix-core-conformance` 依赖）→ **不是 ship blocker**，但「publishable 却从不发布」是意图不清的味道（该标 `publish = false`）。**这是决策不是机械改**，flag 给用户（D-4），本段不擅自改。

## 决策落地形态（§10 —— 动手时若与实测冲突须回报）

- **D-1〔tag + publish 绝对不做——最高约束〕**：本段任何 step、任何验收命令**都不含** `git tag` / `cargo publish` / `npm publish` / `gradle publish` / `gradlew :sdk:publish`。ship-readiness 的验证全部走「不触发发布路径」的等价物：版本串用 grep 核、cargo-semver-checks 直接跑（读 crates.io baseline，不上传）、xcodebuild 只 `build-for-testing`。**不跑 `ship.sh <version>` 全流程**（它 smoke→gate→**publish**）—— 只单独跑其 gate 段的等价命令。发布是用户在 C18 通过后的独立动作。
- **D-2〔cargo-semver-checks 装 + 焊入 ship.sh——推荐〕**：本机未装 → `cargo install cargo-semver-checks --locked`（§13：成本不计，ship gate 需要它）。ship.sh 加一道 gate（形态镜像既有 clippy/llms/ffi-bindings gate：写 `/tmp` log、失败 `fail`）。**位置**：放在 version-match 段**之后**（语义上「版本已对齐，再证破坏配得上 major bump」）。**gate 判据 = `cargo semver-checks check-release --workspace` rc=0**（2.0.0 major 覆盖所有检出破坏）。执行时确认该工具当前 CLI 子命令/flag（`check-release` 是否需显式、`--workspace` 是否支持全量）——工具的接口以其 `--help` 为准，不照抄本文。
- **D-3〔Swift 6 = 评估并记录，不修——推荐，且是本段唯一 assess-only scope〕**：63 处 async-context lock/unlock 的真修 = 把 `EventRecorder` 等改成 actor 隔离 / `@unchecked Sendable` / `OSAllocatedUnfairLock` 迁移，跨多个类、是独立 checkpoint 量级的兔子洞。**C18 只产出书面结论**：执行时用 Swift 6 模式真编一次 UITest target（`xcodebuild ... SWIFT_VERSION=6` 或 `SWIFT_STRICT_CONCURRENCY=complete`，或 SPM 侧 `swift build -Xswiftc -swift-version -Xswiftc 6`），**实测** error 数（不猜），把「v2.0.0 以 Swift 5 语言模式发布 / Swift 6 阻塞项 = N 处 async lock（主 `EventRecorder.swift`）/ 迁移归 v2.x」写进 `docs/v2.md` 决策日志。**gate = 该条目存在且含实测 error 数**（评估质量本身机器兜不住，收尾自陈——同 C16 对 CHANGELOG 改写质量的处理）。
- **D-4〔4 个孤儿 publishable crate 的 `publish = false`——用户 gated，本段不改〕**：`smix-core`/`smix-server`/`smix-ffi`/`smix-core-conformance` 无已发布依赖方，标不标 `publish = false` 是意图声明（§10 决策），**不是 ship blocker**。本段只堵真 blocker（`smix-ai-tier` 进 CRATES），孤儿标记 flag 给用户单独拍。**理由**：错标 `publish = false` 会让某个其实该发的 crate 掉出 DAG；错不标则留 4 个孤儿。实测这 4 个当前无依赖方 → 两种错都不炸本次 ship，故不擅自动。

## 步骤（线性，无分叉；3 个）

### S1. 版本 lockstep 到 2.0.0 + publish DAG 补全（gate: 所有 release 版本串 == 2.0.0 + 跨 crate `^1.x` req 归 0 + `cargo build` 绿 + llms fresh + smix-ai-tier 进 CRATES）

**红（写测试 = 机器 gate 先红）**
- `grep '^version' Cargo.toml | head -1` → 当前 `version = "1.0.27"`（非 2.0.0）。
- `grep -rn 'path = "\.\./[^"]*", version = "1\.' crates/*/Cargo.toml | wc -l` → 当前 **78**（应归 0）。
- `grep -c "smix-ai-tier" scripts/release/ship.sh` → 当前 **0**（应 ≥1）。
- `python3 scripts/dev/gen-llms.py --check; echo $?` → bump 前是 rc=0（llms 现映射 1.0.27）；bump Cargo.toml 后、regen 前会转红，regen 后回 0。

**绿（实现）**
- `Cargo.toml:8` `[workspace.package] version` `1.0.27` → `2.0.0`（级联 29 crate）。
- 19 个 crate Cargo.toml 里 **78 处**跨 crate 路径依赖版本约束（76×`version = "1.0.0"` + 2×`version = "1.0.3"`）→ `version = "2.0.0"`。**只改 smix-* 路径依赖行**，不动第三方依赖的版本约束（`serde = "1"` 等留原样）。
- `npm/smix-rn/package.json:3` · `android-runner/.../SmixRunner.kt:13` · `android-runner/sdk/build.gradle.kts:119` · `README.md:36` → `2.0.0`。
- `scripts/release/ship.sh` CRATES 名单加 `smix-ai-tier`，DAG 序在 `smix-adapter-maestro` **之前**（adapter 依赖它）。
- 重跑 `python3 scripts/dev/gen-llms.py`（无 `--check`）重写 `llms.txt` / `llms-full.txt`（坐标随 2.0.0 刷新；历史 `v1.0.27` 标注不动）。
- 关键点：**历史「vX.Y 落地」标注留不动**（ai-guide/roadmap/v2.md/llms-full 的 `v1.0.27`）；只移 release 版本串。改完 `cargo build --workspace` 必须绿（证 Cargo.toml 无 typo、路径依赖按 2.0.0 解析通过）。Cargo.lock 随之更新（入场已 M，属预期）。

**重构**
- 无（版本对齐，无结构可重构）。

### S2. 两道新 ship gate：cargo-semver-checks + xcodebuild build-for-testing（gate: 两工具各自 rc=0 + 两 gate 焊入 ship.sh）

**红（写测试）**
- `which cargo-semver-checks; echo $?` → 当前 rc=1（未装）。
- `grep -c "semver-checks" scripts/release/ship.sh` → 当前 **0**。
- `grep -c "build-for-testing" scripts/release/ship.sh` → 当前 **0**。

**绿（实现）**
- `cargo install cargo-semver-checks --locked`（装工具）。跑 `cargo semver-checks check-release --workspace`（bump 后 = 2.0.0 vs 1.0.27 baseline）→ 期望 **rc=0**（major 覆盖破坏）；把输出里列出的 major 破坏（`smix-simctl` 的 `SimctlError` 移除等）记进 semver log 作「破坏是 major」的证据。
- `ship.sh` 加 semver gate 段（D-2 位置/形态：写 `/tmp/smix-ship-semver.log`，失败 `fail`）。
- `swift-bridge/` 下 `xcodegen generate`（从 project.yml 刷新 xcodeproj）→ `xcodebuild build-for-testing -scheme SmixRunner -destination 'generic/platform=iOS Simulator'`（编译 `SmixRunner` app + `SmixRunnerUITests`）→ 期望 **rc=0**。
- `ship.sh` 加 xcodebuild gate 段（非 bypassable 侧，紧邻既有 `swift test`；写 `/tmp/smix-ship-xcodebuild.log`，失败 `fail`）。**只 `build-for-testing`，不 `test`（不启模拟器不跑用例，编译即门禁目标）、绝不 publish。**

**重构**
- 若 xcodegen regen 步骤与既有某脚本可共用，抽小 helper（**仅真重复时**，§8.1）。

### S3. Swift 6 就绪度评估（assess-only；gate: `docs/v2.md` 有含实测 error 数的 Swift 6 结论条目）

**红（写测试）**
- `grep -c "Swift 6 就绪度" docs/v2.md` → 当前 **0**（无结论条目）。

**绿（实现）**
- Swift 6 模式真编一次 UITest target（D-3：`xcodebuild ... SWIFT_VERSION=6` 或 `SWIFT_STRICT_CONCURRENCY=complete`；SPM 侧可辅以 `swift build -Xswiftc -swift-version -Xswiftc 6`），**实测** error 数，日志落 `/tmp`。
- `docs/v2.md` 决策日志加一条 `2026-07-18 [C18·Swift 6 就绪度]`：结论 = **v2.0.0 以 Swift 5 语言模式发布**（`Package.swift` swift-tools 5.9）；阻塞项 = **N 处** async-context `lock/unlock`（实测数，主 `EventRecorder.swift`）+ 任何 Sendable 报告；**迁移归 v2.x**（不塞进 v2.0.0，真修是 actor 隔离重设计）。
- 关键点：**不修 lock/unlock**（D-3 assess-only）；N 是**实测**不是估。

**重构**
- 无。

## Checkpoint C18 验收

```bash
# 1. 版本 lockstep 全部 2.0.0（S1）—— rc 单独测，不接管道
grep '^version' Cargo.toml | head -1                                              # 期望 version = "2.0.0"
grep -rn 'path = "\.\./[^"]*", version = "1\.' crates/*/Cargo.toml | wc -l        # 期望 0（无跨大版本残留 req）
node -p 'require("./npm/smix-rn/package.json").version'                           # 期望 2.0.0
grep 'const val VERSION' android-runner/app/src/main/kotlin/dev/smix/runner/SmixRunner.kt   # 期望 "2.0.0"
grep 'val mavenCentralVersion' android-runner/sdk/build.gradle.kts               # 期望 "2.0.0"
grep 'jp.golia.smix:smix-sdk:' README.md                                         # 期望 2.0.0
grep -c "smix-ai-tier" scripts/release/ship.sh                                   # 期望 ≥1（publish DAG 补全）
python3 scripts/dev/gen-llms.py --check >/dev/null 2>&1; echo "llms-fresh rc=$?" # 期望 0（llms 已随 2.0.0 regen）
cargo build --workspace >/dev/null 2>&1; echo "cargo-build rc=$?"                # 期望 0（版本 req 改动无 typo）
# 2. 两道新 ship gate（S2）
which cargo-semver-checks >/dev/null 2>&1; echo "semver-installed rc=$?"         # 期望 0
cargo semver-checks check-release --workspace >/tmp/c18-semver.out 2>&1; echo "semver rc=$?"   # 期望 0（major 覆盖破坏）
grep -c "semver-checks" scripts/release/ship.sh                                  # 期望 ≥1（gate 焊入）
grep -c "build-for-testing" scripts/release/ship.sh                              # 期望 ≥1（gate 焊入）
( cd swift-bridge && xcodegen generate >/dev/null 2>&1 && xcodebuild build-for-testing -scheme SmixRunner -destination 'generic/platform=iOS Simulator' ) >/tmp/c18-xcb.out 2>&1; echo "xcodebuild rc=$?"   # 期望 0（SmixRunnerUITests 编译过）
# 3. Swift 6 就绪度已记录（S3）
grep -c "Swift 6 就绪度" docs/v2.md                                              # 期望 ≥1（含实测 error 数）
# 4. 无回归（版本改动后重跑既有 gate；本段不碰 wire/FFI 逻辑）
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$?"      # 期望 0
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"  # 期望 0
python3 scripts/dev/hygiene-scan.py >/dev/null 2>&1; echo "hygiene rc=$?"         # 期望 0
```

期望，逐条：
1. workspace `2.0.0`、跨 crate `^1.x` req 计数 **0**、npm/Kotlin/gradle/README 全 `2.0.0`、`smix-ai-tier` 进 CRATES、`llms-fresh rc=0`、`cargo-build rc=0`。
2. `semver-installed rc=0`、`semver rc=0`（`/tmp/c18-semver.out` 里列出 `smix-simctl` 等的 major 破坏 = 破坏是 major 的证据）、ship.sh 含 `semver-checks` 与 `build-for-testing` 各 ≥1、`xcodebuild rc=0`。
3. `docs/v2.md` 有 Swift 6 就绪度条目 ≥1，且条目含**实测** error 数。
4. `route` / `bindings-fresh` / `hygiene` 全 **rc=0**（版本 bump 不改 wire/FFI 逻辑/文档噪声，理应不动；若 `bindings-fresh` 因版本被生成物携带而红，S1 内重生成两侧 bindings 后回 0）。

**tag / publish 明确不在验收内**：以上全绿 = **仓库 ship-ready**。`git tag swift-v2.0.0` + `cargo publish` + `npm publish` + `gradle :sdk:publish` **不跑、不计划**，待用户在本 checkpoint 通过后单独 go-ahead。**「ship-ready」≠「shipped」是本 checkpoint 的定义线。**

**仪器纪律**（本 cycle 反复吃亏，每条都是 v2.md 决策日志记过的实伤）：
- **测退出码不接管道** —— `cmd | tail; echo $?` 量的是 `tail`。本段验收 rc 全部单独 `>/dev/null 2>&1; echo "rc=$?"` 或落 `/tmp`。
- **`grep -c` 报的是命中/排版不是工作** —— `semver rc=0` 只证 major bump adequate；破坏真被检出要看 `/tmp/c18-semver.out` 的破坏列表。`xcodebuild rc=0` 才是「UITest 主体真编过」的裁判，不是 `grep build-for-testing ship.sh`（那只证 gate 文本在）。
- **版本串按角色区分** —— 不用 `git grep -c 1.0.27 == 0`（误伤历史「落在 v1.0.27」标注）；只打 release 串（见验收精确 grep）。
- **绿 ≠ 已做对** —— `semver rc=0` 不证「改名破坏被证明」（工具看不见 rename，Block 2 已标）；Swift 6 条目存在不证评估质量（收尾自陈实测数来自真编一次，非估）。

## 未被本 checkpoint 覆盖的（写在明处）

1. **tag + publish**（跨 4 生态发布 + `git tag swift-v2.0.0` + push）—— **用户既定硬规则「最后 tag 和 pub 先不做」**，C18 通过后由用户单独拍。这是 v2 cycle 的最后一步，不在任何 checkpoint 的自动路径里。
2. **Swift 6 真迁移**（63 处 async lock/unlock 改 actor 隔离）—— D-3 assess-only；真修归 v2.x，是独立 checkpoint 量级。
3. **4 个孤儿 publishable crate 的 `publish = false` 标注**（`smix-core`/`smix-server`/`smix-ffi`/`smix-core-conformance`）—— D-4，用户 gated 的意图声明，非 ship blocker（实测无已发布依赖方），本段不擅自改。
4. **v2.md:42「Checkpoint 概览」行仍停在旧编号**（写「C14 ship」，是 C6-C16 六次拆分前的文本）—— 与本段 C18 编号不符。属 doc 内部陈旧，非本段 scope（本段只加 Swift 6 决策条目，不重写概览行）；若用户要一并同步，flag 为小编辑。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c18-hot.md`
2. **C18 是 v2 cycle 最后一个 checkpoint** —— 通过后**不自动热化下一段**：v2 计划的 checkpoint 到 C18 为止（冷计划 `plan-cold/v2.md` C18 = ship）。收尾向用户报「仓库 ship-ready、所有 gate 全绿」，并**等用户对 tag + publish 的单独 go-ahead**。若用户要开 v2.x，另起冷计划（§6）。

## 与 brief / 冷计划不符之处（必须先读，不要隐瞒）

1. **brief「~5 个版本串」= 低估** —— 实测 release 版本面是 **5 个直接串 + 78 处跨 crate `^1.x` 版本约束**（76×1.0.0 + 2×1.0.3，跨 19 crate）。后者跨大版本边界不改 → 2.0.0 发布即 pull 回 1.0.x 兄弟 crate（只在 publish 炸、`cargo build` 看不见）。本段 S1 把它列为主体工作、gate 判其归 0。
2. **brief 未列的真 ship blocker：`smix-ai-tier` 不在 ship.sh 发布名单** —— 它 publishable、被已发布的 `smix-adapter-maestro` 依赖，却不在 CRATES → `cargo publish smix-adapter-maestro` 会失败。S1 一并堵（进 CRATES，DAG 序在 adapter 前）。另 4 个孤儿 publishable crate 是味道非 blocker（D-4 flag）。
3. **cargo-semver-checks 对 rename 破坏（#5）+ 新 crate（`smix-ai-tier`）盲** —— 工具比的是「同名 crate current vs crates.io baseline」；改名 crate 无新名 baseline、旧名已消失 → 都 skip。**别指望 semver-checks「证明」rename 破坏**，它只证 in-place API 破坏（如 `SimctlError`→`DeviceControlError`）。gate 判 rc=0（major 覆盖），rename 破坏的证据在决策日志（C12/C13）不在工具。
4. **冷计划 C18「评估 Swift 6 就绪度」= assess-only，本段照此** —— 63 处 async lock/unlock 真修是兔子洞，D-3 定 assess-only（实测 error 数 + 书面结论 + 迁移归 v2.x）。若用户要 C18 内真修 Swift 6，那是另一个 checkpoint 量级、需先拍板，本段不擅自扩。
5. **是否 C18 塞得下一个 checkpoint** —— 判定**塞得下**（3 step 线性：版本 lockstep / 两 gate / Swift 6 assess-only），前提是 Swift 6 锁死为 assess-only。**两处让它「比 brief 大」的是 78 跨 crate req + publish DAG 补全**，但两者仍属「让 release 版本连贯」的机械同类工作，不改变 checkpoint 形状。**若用户认为「版本连贯（S1）」与「gate 加固（S2）」应各自成段**（本 cycle C6-C16 皆因「风险性质不同」拆过），可拆 —— 拆分属用户权力（§10），本段 plan-of-record 取「一段 3 step」，flag 供用户否决。

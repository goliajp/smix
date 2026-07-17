# plan-hot — v2 到 C7：让 FFI 边界可重建 + 长出驱动面

## 目标 checkpoint

C7：这条 uniffi 边界**可以被重建**（今天不能 —— `scripts/sdk/` 下三个被引用的构建脚本不存在，xcframework 与 `.so` 是无源签入 blob），且 `smix-ffi` 长出 runner 驱动面 = Rust client 的薄封装。

通过后世界：**加一条 route 只改一处**。四份 wire 实现里三份是互为 mirror 的移植（继承同一套虚构，加一条路由要改 4 处，于是没人改）—— 这个成因被结构性消除，而不是被一次性修好又等着重新漂移。

同时 `smix.udl` 里「驱动不在这个边界上，这让它免于 async 与 cancellation」那句被删——**它准确描述了当时的实现，但它陈述的理由正是病因**（v2.md:119）。

## 前置条件

```bash
git branch --show-current                                 # 期望 feature/v2.0
git log --oneline -1                                      # 期望 C6 已归档（210548024 或其后）
pgrep -fl "runner.ts|smix run|supervise"                  # 期望空（in-house batch 不活动）
pgrep -fl "gradle|mobilegate|emulator"                    # 期望空（S1/S3 要动 gradle + cargo-ndk）
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "rc=$?"   # 期望 rc=0
bash scripts/dev/fence-check.sh >/dev/null 2>&1; echo "rc=$?"                    # 期望 rc=0
cargo test --workspace 2>&1 | grep -c "^test result: ok"                         # 期望 131（本段基线，实测）
cargo clippy --workspace --all-targets 2>&1 | grep -cE "^(error|warning): "      # 期望 0
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "rc=$?"           # 期望 rc=1（C7 的起点即欠债）
```

以上全部本次热化实测通过（`cargo test` 131 / clippy 0 / hygiene 0 / fence 0 / 两个 pgrep 空 / route-conformance rc=1）。

## 已确证的起点（本次热化实测，非转述）

### ① route-conformance 的欠债精确为 13 route / 40 place / 3 SDK

实测 `python3 scripts/dev/route-conformance.py` → **rc=1**，报 `13 route(s) no runner serves, in 40 place(s)`。载体逐字：
`npm/smix-rn/src/HttpRunner.ts`（237 行）· `android-runner/sdk/src/main/kotlin/dev/smix/sdk/HttpSmixSimRuntime.kt`（240 行）· `swift-bridge/Sources/SmixSDK/HttpSmixSimRuntime.swift`（307 行）—— **合计 784 行**（`wc -l` 实测，与冷计划的数字一致）。

**C7 的出口条件是 rc=0**，即这 40 处必须消失，不是改字符串。

### ② uniffi 0.29.5 给 async，**不给 cancellation** —— 这决定了整个设计

用户拍板明写「必须正面解决 async / cancellation，不能绕」。正面解决的第一步是**先量清楚工具真给什么**，而不是照 v7.0 的计划复述。读 `~/.cargo/registry/.../uniffi*-0.29.5/` 源码（`Cargo.lock` 实测 uniffi 全家 **0.29.5**）：

| 问题 | 实测答案 | 依据（file:line） |
|---|---|---|
| UDL 支持 `[Async]` 吗？ | **支持** | `uniffi_udl-0.29.5/src/attributes.rs:43,72` |
| UDL 能要 tokio runtime 吗？ | **不能** | UDL scaffolding 模板只 emit 裸 `#[::uniffi::export_for_udl]`（`uniffi_bindgen-0.29.5/src/scaffolding/templates/TopLevelFunctionTemplate.rs`）；`async_runtime` 在整个 `uniffi_bindgen` 里**一次都不出现** |
| 谁才给 tokio runtime？ | **只有 proc-macro** | `#[uniffi::export(async_runtime = "tokio")]` → `uniffi_macros-0.29.5/src/export/scaffolding.rs:282-283` 才 wrap `::uniffi::deps::async_compat::Compat::new(...)` |
| Swift 侧 `Task.cancel()` 能取消在途调用吗？ | **不能** | `uniffi_bindgen-0.29.5/src/bindings/swift/templates/Async.swift` 的 `uniffiRustCallAsync` 用 `withUnsafeContinuation`（不可取消），**无** `withTaskCancellationHandler` |
| Kotlin 侧协程取消能传过去吗？ | **不能** | `bindings/kotlin/templates/Async.kt` 的 `uniffiRustCallAsync` 只 `suspendCancellableCoroutine` + `finally { freeFunc }`，**从不调 cancel** |
| `rust_future_cancel` 存在吗？ | **存在但没人调** | 定义 `uniffi_core-0.29.5/src/ffi/rustfuture/mod.rs:156`，符号生成 `uniffi_bindgen/src/interface/mod.rs:566`。`grep -rn "future_cancel" bindings/swift/` **唯一命中是 `Async.swift:120` 的一句注释，且讲的是反方向**（Rust 取消 Swift callback）；`bindings/kotlin/` **零命中** |

**两条硬结论，直接定型 S2**：

1. **驱动面必须走 proc-macro，不能走 UDL。** `smix-runner-client` 是 tokio + reqwest（`Cargo.toml` 实测；`src/lib.rs` 44 个 `async fn`）。UDL `[Async]` 拿不到 `Compat`，reqwest future 会被 uniffi 自己的 scheduler 在无 reactor 的上下文里 poll → 运行时 panic。**这个坑不会被编译器抓，只会在真机上炸** —— 所以 S2 的红必须有一个「真跑一次 tokio 依赖的 future」的测试，而不是只断言签名。

2. **cancellation 必须显式，且必须在 Rust 侧。** uniffi 0.29.5 不把外语的取消传过边界。**若我们 export `async func tap()` 并让调用方以为 `Task.cancel()` 有用，那就是本 cycle 的病原样复发**——一个宣称自己会做而实际不做的表面，与那三个 SDK 打 404 是同一种谎。v7.0 UDL 注释计划的 `cancel_{op}(handle)` sibling **方向是对的，而且在 0.29.5 上它是唯一诚实的选项**。

### ③ 「Rust client 的薄封装」这个措辞与代码有出入，但**用户的拍板结论仍然成立**

逐个核 `SmixSimRuntime` 的 13 个方法（`swift-bridge/Sources/SmixSDK/SimRuntime.swift`，`grep -cE "^\s+func "` 实测 **13**）：

- **6 个干净对得上**：`snapshotTree`→`GET /tree` · `sendString`→`/input-text` · `pressKey`→`/press-key` · `swipe`→`/swipe-once` · `systemPopups`→`GET /system-popups` · `synthesizeTapAtNormalized`→`/tap-at-norm-coord`。
- **2 个要 session**：`launch`/`terminate` → `/session/launch-app` / `/session/terminate-app`。
- **1 个形状错**：`synthesizeTap(at: CGPoint)` 收**绝对像素**并 POST `/input/tap`（`HttpSmixSimRuntime.swift:80-84` 实测）；runner 只有 `/tap`（selector）与 `/tap-at-norm-coord`（归一化），**没有绝对像素 tap**。
- **4 个无路由**：`screenshot` / `openUrl` / `launchFresh` / `launchFromPath`。

这 4 个 `smix_runner_client` 确实给不了。但 **`smix-sdk::App` 给得了**——`screenshot`（`smix-sdk/src/lib.rs:1272`）· `open_url`（:1247）· `launch_fresh`（:1091，编排在 `plan_launch_fresh_calls_v2`，:397）。于是「薄封装 client 还是封装 App」看起来是个真分岔。

**它不是分岔——实测把它关死了**：`smix-sdk` 依赖 `smix-simctl`（spawn `xcrun simctl`）+ `smix-adb`（spawn `adb`），**两者都是宿主侧工具**；而 `smix-ffi` 的产物是**设备侧**的：`SmixCoreFFI.xcframework` 只有 `ios-arm64-simulator` / `macos-arm64` 两个 slice（`libsmix_ffi.a`），Android 侧是 `com.android.library`（minSdk 33）里的 `jniLibs/{arm64-v8a,x86_64}/libuniffi_smix.so`。**模拟器里没有 `xcrun`，Android 设备上没有 `adb`。** 把 `smix-sdk` 拖进这条边界 = 往 on-device .aar 里塞 simctl 调用。

而 `smix-runner-client` 是 reqwest → localhost:28080，**三种部署都通**（宿主 macOS / iOS Simulator 内 / Android 设备上——Android runner 本来就在设备上）。

**所以用户拍的「薄封装 Rust client」是对的**，代价是那 4 个方法**离开 SDK 的 runtime 协议**——它们是宿主侧编排，不是 wire。**这不是能力倒退：它们从来就打 404，一次都没工作过**。计入 v2 破坏性变更。

### ④ **C7 的真起手是：这条边界现在根本重建不了**

C7 要往 uniffi 边界加函数，就必须重新生成 `libsmix_ffi.a` × 2 slice + `libuniffi_smix.so` × 2 ABI + `smix.swift` + `smix.kt`。实测：

```
MISSING: scripts/sdk/build-xcframework.sh        ← Package.swift 的 binaryTarget 注释引用它
MISSING: scripts/sdk/build-android-aar.sh        ← android-runner/sdk/build.gradle.kts 引用它
MISSING: scripts/sdk/run-cross-binary-harness.sh ← Package.swift 引用它
```

`scripts/sdk/` **整个目录不存在**（`find scripts -maxdepth 2 -type f` 实测只有 `dev/` 4 个 + `release/` 5 个）。xcframework 与 `.so` 是**签进仓库的预编译 blob，没有任何可复现的构建路径**。

**本 cycle 第八次「注释是主张，代码是事实」，而这次那句假话是个硬 blocker** —— 不先补它，S2 一行都动不了。按 §12.2，这是能力缺位，补它，不绕。

连带两处同型失真（S1 一并修）：
- `build.gradle.kts:7` 写 `libsmix_ffi.so`，实际是 **`libuniffi_smix.so`**（uniffi Kotlin binding 按 `libuniffi_<namespace>.so` 载入，名字必须如此）。
- `smix-bindgen-swift.rs` 的用法注释写 out-dir `swift-bridge/Sources/SmixCoreFFI/Generated`，实际是 **`Sources/SmixCoreFFIBindings/Generated`**（`Package.swift` + `ls` 实测）。

### ⑤ Kotlin 的 bindgen 走 UDL 模式，会把 proc-macro 导出**静默吞掉**

`smix-bindgen.rs` 的用法注释是 `generate crates/smix-ffi/src/smix.udl --language kotlin` = **UDL 模式**，只认 UDL 里声明的东西。而 `smix-bindgen-swift.rs` 走 `uniffi_bindgen_swift(<library path>)` = **library 模式**，读编译产物里的 metadata，proc-macro 导出会被收进来。

即：②的结论（驱动面必须 proc-macro）落地后，**Swift 会有驱动面，Kotlin 会没有，而且不报错**——生成一份缺一半的 binding，编译期才在 SDK 侧炸。Kotlin 侧必须切 `--library` 模式。**这正是 gate 要盯的形状**，不是靠我记得。

### ⑥ TS 没有任何 native 通路，而 CLI 给不了动作级驱动

- `grep -rln "napi\|wasm-bindgen\|neon"`（`*.toml` / `*.json` / `*.sh`，排除 node_modules/target）→ **零命中**。`npm/smix-rn/package.json` 无 native dep；`src/` 无 `child_process` / `dlopen`（实测零命中）。
- `smix` CLI 是 **flow 级**的（`smix run flow.yaml`），没有动作级表面。让 TS 的 Playwright-shape `App` 走 CLI = 每个动作 spawn 一次进程，且要先发明一套动作级 CLI 协议——**那比 napi 更大，而且是凭空多一个产品表面**。
- npm 现在**唯一真的** 3 条路由是 `/select/resolve{,-count,-labels}`（`SelectorResolver.ts` 是个函数类型 seam，真实现由 `HttpRunner.ts` 用 HTTP 供）。**按「一份 wire client」这条也得走**——SDK 说 HTTP 就是在实现 wire。

**决策：TS 走 napi**（§2 要求事先决定，不留分叉）。理由：它是唯一能同时给动作级驱动 + 消掉最后一份 wire 实现的路。**代价诚实记账：这是一条新的分发轴**（per-triple 预编译 `.node`），S3 承担。

### ⑦ session 不可分割（C6 已记，本次复核）

`SessionAppLifecycleRequest.session_id` 是必需字段——**实测在 `crates/smix-runner-wire/src/lib.rs:675`**（v2.md:129 记的 `:632` 是错的，那一行实际是 `SessionCloseAllResponse` 的 `pub closed: u32`；`grep -n "pub session_id"` 得 6 处：456/476/499/645/675/768）。三个 SDK 无 session 概念，故 C7 的修复内含「SDK 获得 session handle」。break #1 的另一半（去掉 Rust/CLI 侧的隐式 no-session 路径）仍属 C8。

## 步骤（线性，无分叉）

> S3（三个 SDK 删掉 784 行虚构 wire、改调唯一那一份）已移出本段，成为 C8 —— 判据同 C6 那次拆分：本段修的是**已经坏掉 / 根本不存在**的东西，C8 动的是**发布物的公开 API**，风险性质不同。见 v2.md 决策日志 2026-07-17「拍板·拆 C7」。

### S1. 先让这条边界可重建 —— 三个构建脚本不存在

**红（写测试）**

- 文件：`scripts/dev/ffi-bindings-fresh.sh`（新）
- 断言：从 `crates/smix-ffi` 重新生成 Swift + Kotlin bindings，与仓库里签进去的 `swift-bridge/Sources/SmixCoreFFIBindings/Generated/smix.swift`、`android-runner/sdk/src/main/kotlin/uniffi/smix/smix.kt` **逐字节相同**；不同即 rc=1。当前**红**：生成路径不存在（`scripts/sdk/` 缺 3 个脚本）。
- **失败优先于无知**：生成步骤任一环节失败（cargo build / bindgen / 目标文件读不到）→ **rc=1 并说明哪一环**，不许因为「什么都没生成出来所以没有 diff」而绿。这是 C6 那条 Swift wireSchema gate 的同一形状（v2.md:103），也是本 cycle「green ≠ tested」教训的直接应用。

**绿（实现）**

- 文件：`scripts/sdk/build-xcframework.sh`（新）—— `cargo build -p smix-ffi --release` × {`aarch64-apple-ios-sim`, `aarch64-apple-darwin`} → `xcodebuild -create-xcframework` → `swift-bridge/SmixCoreFFI.xcframework`（**只这两个 slice**；真机 slice 永不加，§9#1）。
- 文件：`scripts/sdk/build-android-aar.sh`（新）—— `cargo ndk -t arm64-v8a -t x86_64 -o android-runner/sdk/src/main/jniLibs build --release`。产物名 **`libuniffi_smix.so`**（uniffi Kotlin binding 按 `libuniffi_<namespace>` 载入）。
- 文件：`crates/smix-ffi/src/bin/smix-bindgen.rs`
- 动作：Kotlin 生成**切 library 模式**（读编译产物的 metadata），不再走 UDL 路径。依据起点 ⑤ —— UDL 模式会把 S2 的 proc-macro 驱动面静默吞掉。
- 关键点：三处失真注释一并改真（`build.gradle.kts` 的 `libsmix_ffi.so` → `libuniffi_smix.so`；`smix-bindgen-swift.rs` 的 out-dir → `SmixCoreFFIBindings/Generated`；`Package.swift` 与 `build.gradle.kts` 里指向这两个新脚本的引用现在**真的指得到**）。

**重构**

- `scripts/dev/ffi-bindings-fresh.sh` 焊进 `scripts/release/ship.sh`。**干净的时刻正是设门禁的时刻**（C5 对 clippy 的同一判断，v2.md:88）——否则到 C10 又会长回来。

### S2. `smix-ffi` 长出驱动面：proc-macro + tokio runtime + 显式 cancel

**红（写测试）**

- 文件：`crates/smix-ffi/tests/driving.rs`（新）
- 断言 1（**reactor 真的在**）：起一个 `wiremock` 服务，经 FFI 驱动面发一次真实 HTTP 往返并拿到应答。**这条测试的全部意义是抓起点 ② 的坑** —— 若驱动面被声明成 UDL `[Async]`，reqwest future 会在无 tokio reactor 的上下文被 poll 而 panic，而**签名断言看不到这一点**。当前红（驱动面不存在）。
- 断言 2（**cancel 真的取消**）：对一个永不应答的 endpoint 发起调用 → 调 `cancel()` → 调用以 `Cancelled` 结束，且**不是**靠超时到点。当前红。
- 断言 3（**session 是必需的**）：不带 session handle 的 launch/terminate 在**编译期**不可表达（handle 是参数而非可选字段），运行期 wire 收到的 `sessionId` 非空。
- **不写 mock-only 的断言**：本 cycle 的账已经算得很清楚——三个 SDK 的测试全注入 mock，**它们验证的是 SDK 跟自己说话**，那正是虚构 wire 得以出厂的方式（v2.md:156）。`wiremock` 验的是真 HTTP 字节，不是我自己的替身。

**绿（实现）**

- 文件：`crates/smix-ffi/src/driving.rs`（新）+ `crates/smix-ffi/src/lib.rs`
- API：`#[derive(uniffi::Object)] struct SmixDriver`，`#[uniffi::export(async_runtime = "tokio")] impl SmixDriver { ... }` —— 薄封装 `smix_runner_client::RunnerClient`（起点 ③：只封 client，不封 `smix-sdk::App`）。
  - `SmixDriver::new(port: u16) -> Arc<Self>`
  - `SmixSession`（uniffi Object）承载 `session_id`；launch/terminate 挂在它上面（起点 ⑦）。
  - `CancelToken`（uniffi Object，内含 `tokio_util::sync::CancellationToken`）+ `CancelToken::cancel()`；每个驱动方法收 `Option<Arc<CancelToken>>`，实现用 `tokio::select!` 在请求与 `token.cancelled()` 之间选。
- 关键点（三条，逐条对着起点 ② 的实测）：
  1. **必须是 `#[uniffi::export(async_runtime = "tokio")]`，不能进 `smix.udl`。** UDL 拿不到 `Compat`（`uniffi_bindgen` 里没有 `async_runtime` 这个概念）。UDL 继续只放同步的 selector 核心。
  2. **cancel 显式，因为 0.29.5 不传外语的取消。** 不 export 一个假装能被 `Task.cancel()` 取消的 async 方法——那是把这次翻车的病原样再犯一遍。
  3. `Cargo.toml` 加 `smix-runner-client` + `tokio`（`rt-multi-thread`）+ `tokio-util` + dev-dep `wiremock`。
- 文件：`crates/smix-ffi/src/smix.udl`
- 动作：删掉「Driving the device is not on this boundary — that goes over HTTP to the runner, which keeps this one free of async and cancellation」。**这句话被用户拍板直接推翻**（v2.md:119：它准确描述了当时的实现，但它陈述的理由恰恰是病因）。改成陈述现在的真实分界：UDL 侧 = 同步纯函数的 selector 核心；驱动面 = proc-macro 侧，async，cancel 显式且**为什么显式**（uniffi 0.29.5 不传外语取消——写下这个理由，免得它变成下一条陈旧注释）。

**重构**

- 跑 `scripts/dev/ffi-bindings-fresh.sh` 重生成两侧 bindings 并签入。Kotlin 侧的 `smix.kt` 必须真的含驱动面 —— 起点 ⑤ 说的静默吞掉，此刻就会被 S1 的 gate 抓住。

## Checkpoint C7 验收

```bash
# 1-2. 三个 SDK 改调唯一那一份 wire client = C8。本段的路由 gate 仍红，
#      只要求它在岗报数（rc=0 是 C8 的出口条件）：
python3 scripts/dev/route-conformance.py 2>&1 | grep -c "no runner serves"
# 3. FFI 驱动面：真 HTTP 往返 + 真 cancel（wiremock，非 mock 替身）
cargo test -p smix-ffi 2>&1 | grep "^test result:"
# 4. 边界可重建：重生成 bindings 必须与签入的逐字节相同（生成失败 = rc=1，不是绿）
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"
# 5. Kotlin 侧真的拿到了驱动面（起点 ⑤：UDL 模式会静默吞掉它）
grep -c "class SmixDriver\|fun cancel" android-runner/sdk/src/main/kotlin/uniffi/smix/smix.kt
# 6. UDL 那句被推翻的理由已删
grep -c "free of async and cancellation" crates/smix-ffi/src/smix.udl
# 7. 无回归
cargo test --workspace 2>&1 | grep -c "^test result: ok"
cargo clippy --workspace --all-targets 2>&1 | grep -cE "^(error|warning): "
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"
# swift：读 XCTest 的 "Executed N tests" 行。不要读 "Test run with N tests ... passed" ——
# 那是 swift-testing harness 的行，实测报 0 tests，真身在另一行。
( cd swift-bridge && swift test 2>&1 | grep "Executed .* tests" | tail -1 )
# android：BUILD SUCCESSFUL 不是证据 —— 实测 `./gradlew test` 会打 BUILD SUCCESSFUL +
# "54 up-to-date" 却一个测试都没跑。数 XML 里的真数字，且强制重跑。
( cd android-runner && ./gradlew test --rerun-tasks --console=plain >/dev/null 2>&1
  find . -name "TEST-*.xml" | xargs grep -ho 'tests="[0-9]*"'    | grep -o '[0-9]*' | paste -sd+ - | bc
  find . -name "TEST-*.xml" | xargs grep -ho 'failures="[0-9]*"' | grep -o '[0-9]*' | paste -sd+ - | bc )
# npm
( cd npm/smix-rn && bun x vitest run 2>&1 | tail -3 )
```

期望，逐条：

1. **`rc=0`** —— 这是 C7 的出口条件（C6 出口是 rc=1 + gate 在岗报数，v2-c6-hot.md:84）。
2. 无 `STILL PRESENT` 行，只有 `runtime-files-checked`。
3. `test result: ok`，`0 failed`。
4. **`rc=0`**。
5. 计数 **≥1**（Kotlin binding 真含驱动面）。
6. 计数 **0**（那句理由已删）。
7. `test result: ok` 计数 **≥131**（本段基线实测 131，不回退）；clippy **0**；hygiene `rc=0`；swift 那行 **≥360 且 0 failures**；android 两数字 **≥134** 与 **0**；npm vitest **0 failed**。

**仪器纪律**（下列每条本次热化都亲手复现过，不是转述）：

# 路由 gate 仍红（三个 SDK 改调 = C8）。本段只要求它在岗报数：
- **`--include='*.rs'` 必须带引号**。本次热化第二次踩：不带引号 zsh 直接 `no matches found`，整条 grep 不执行（与 v2-c6-hot.md:168 记的同一处）。
- `swift test` 同时给两个「通过」（swift-testing 的 `0 tests` + XCTest 的 `Executed 360 tests`）；grep 错行就是拿 0 个测试的绿冒充 360 个的绿。
- `./gradlew test` 的 `BUILD SUCCESSFUL` 可在零测试执行时打印；数 XML，且 `--rerun-tasks`。
- 第 2/5/6 组量的是**文件与代码里的计数**，不是文档排版（v2.md:78 的教训：别把验收命令写成量自己的排版）。
- 第 4 组的 gate **必须在生成失败时 rc=1**，不许「没生成出来所以没 diff」而绿 —— 与 C6 那条「读不到列表就判定形状变了并失败」同规格（v2.md:103）。

**未被本 checkpoint 覆盖的**：三个 SDK 接回真 wire 后**仍无真设备证据**。gate 证明「不再引用不存在的路由」，`wiremock` 证明「FFI 真的发出了 HTTP 且 cancel 真的取消」，但**都不证明请求在真 sim / 真 emulator 上被正确应答**。四 SDK 的真设备 smoke 属 C10 的 ship gate（v2.md:70 已记 C10 要补 `xcodebuild build-for-testing -scheme SmixRunner`）。**按 C3/C4/C5/C6 的同一条教训写在明处：mock 与 schema 都证明不了真设备上的事** —— 而这三个 SDK 正是靠 mock 出厂了一套虚构的 wire。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c7-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 C8：三个 SDK 删掉 784 行虚构 wire、改调唯一那一份；`route-conformance.py` rc=0 是它的出口条件），见 CLAUDE.md §6

## 与冷计划不符之处（必须先读，不要隐瞒）

1. **冷计划写「`smix-ffi` 新增 runner 驱动面（Rust client 的薄封装）」，但没预料到薄封装吃不下 4 个方法。** `screenshot` / `openUrl` / `launchFresh` / `launchFromPath` 的编排在 `smix-sdk::App`（simctl + adb，宿主侧），而 `smix-ffi` 的产物是设备侧的（xcframework 只有 ios-sim + macos slice；Android 是 minSdk 33 的 .aar）。**结论仍是薄封装 client（用户拍板成立），但代价是那 4 个方法离开 SDK 协议 = 额外的 v2 破坏性变更**，冷计划的 C7 行没记这一项。
2. **冷计划没预料到这条边界现在重建不了。** `scripts/sdk/` 三个被 `Package.swift` / `build.gradle.kts` 引用的构建脚本**全部不存在**，xcframework 与 `.so` 是无源的签入 blob。**加一个函数到 uniffi 边界的前提不存在**，故 S1 是补它，而非直接动驱动面。冷计划的 C7 行假设边界是活的。
3. **冷计划写「TS 走 napi 或 CLI」——这是分叉，§2 不允许，且 CLI 那条实测走不通。** 仓库无任何 napi/wasm/neon 基础设施（零命中），`smix` CLI 是 flow 级、无动作级表面。**已定：napi**，并诚实记账它是一条新的 per-triple 分发轴（S3 承担）。
4. **冷计划写「必须正面解决 async / cancellation」，但 v7.0 计划的 `cancel_{op}(handle)` 前提需要更正一半。** uniffi 0.29.5 实测：async **能**做（但必须 proc-macro，UDL 拿不到 tokio runtime）；cancellation **做不到自动传递**（Swift/Kotlin binding 从不调 `rust_future_cancel`）。**即 v7.0 的显式 cancel 方向是对的，但理由不是「我们选择不用 async」，而是「工具根本不给」** —— 这两句话导出同一个设计，却导出完全不同的注释。S2 写的是后者。
5. **`docs/v2.md:129` 的 `lib.rs:632` 引用错了**（实际 `:675`；`:632` 是 `SessionCloseAllResponse.closed`）。断言本身（session_id 必需）为真。
6. **C7 是拆过之后仍然最大的一段** —— S3 单步跨 3 种语言 + 一条新分发轴 + 3 处 API 破坏 + 三套测试重写。**这与 C6 被拆前的形状同型**（8 项压 3 step，一个 gate 只在最后响一次）。**没有自行再拆**（拆 checkpoint 是 §10 决策，属用户权力，不内部消化）——但据 v2.md:139「拆的理由不是工作量，是风险性质不同」这条判据：本段 S1+S2（补构建路径 + 长驱动面）修的是**已经坏掉/不存在的东西**，S3（三个 SDK 改调）动的是**发布物的公开 API**，两者风险性质确实不同。**是否拆 = 用户拍板。**

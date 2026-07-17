# plan-hot — v2 到 C8：补齐 smix-ffi 驱动面

## 目标 checkpoint

C8：C7 的 `smix-ffi` 驱动面只有 4 个方法（tree + session open/launch/terminate），SDK 的 App 层要不了这么点。补齐 SDK 需要的 ~10 个（tap_by_id / tap_at_norm_coord / input_text / press_key / swipe_once / system_popups / session close / renew_activation / relaunch_app / list_sessions），各带 wiremock 真往返测试，重生成两侧 bindings 并让二进制携带它们。

**纯增量：不碰任何已发布物。** 三个 SDK 改调 = C9（那才破坏公开 API）；`route-conformance` 在 C8 出口**仍红**，它的 rc=0 是 C9 的出口条件。

## 前置条件

```bash
git branch --show-current                                 # 期望 feature/v2.0
git log --oneline -1                                      # 期望 C7 已归档（8ea111f0d 或其后）
pgrep -fl "runner.ts|smix run|supervise"                  # 期望空（in-house batch 不活动）
pgrep -fl "gradle|mobilegate|emulator"                    # 期望空（S1/S2 要动 cargo-ndk + gradle）
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"   # 期望 rc=0
bash scripts/dev/fence-check.sh >/dev/null 2>&1; echo "fence rc=$?"                      # 期望 rc=0
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"      # 期望 rc=0（C7 焊入的 gate 现绿）
cargo test --workspace >/tmp/c8_base.out 2>/dev/null; echo "cargo rc=$?"
grep -c "^test result: ok" /tmp/c8_base.out                                              # 期望 132（本段基线，实测）
grep -c "test result: FAILED" /tmp/c8_base.out                                           # 期望 0
cargo clippy --workspace --all-targets 2>&1 | grep -cE "^(error|warning): "              # 期望 0
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$?"             # 期望 rc=1（C8 的起点即欠债，rc=0 是出口）
```

以上全部本次热化实测通过（cargo 132/0 · clippy 0 · hygiene 0 · fence 0 · bindings-fresh 0 · route-conformance rc=1 · 两个 pgrep 空）。

## 已确证的起点（本次热化实测，非转述）

### ① route-conformance 的欠账精确为 13 route / 40 place / 3 个 SDK 文件

实测 `python3 scripts/dev/route-conformance.py` → **rc=1**，`13 route(s) no runner serves, in 40 place(s)`。40 处全部落在三个文件：`npm/smix-rn/src/HttpRunner.ts`（237 行）· `android-runner/sdk/src/main/kotlin/dev/smix/sdk/HttpSmixSimRuntime.kt`（240 行）· `swift-bridge/Sources/SmixSDK/HttpSmixSimRuntime.swift`（307 行），合计 **784 行**。**C8 的出口条件是 rc=0** —— 这 40 处必须消失。

### ② C7 建的驱动面是**骨架**，不是全集 —— 这是本段最大的、prompt 的"measured facts"没写清的事实

实测 `crates/smix-ffi/src/driving.rs` + 两侧 bindings，驱动面当前只暴露 **4 个 wire 方法**：

- `SmixDriver`：`tree`（→ `GET /tree`）· `open_session`（→ `/session/open`）· `new` · `cancel_token`
- `SmixSession`：`launch_app`（→ `/session/launch-app`）· `terminate_app`（→ `/session/terminate-app`）· `id`
- `CancelToken`：`cancel` · `is_cancelled`

而三个 SDK 的 `App` + `Session` 层实际调用的 wire 方法远不止这 4 个。实测 Swift `App.swift` 用到 runtime 的 **12 个方法**：`launch` `terminate` `snapshotTree` `synthesizeTap` `synthesizeTapAtNormalized` `sendString` `pressKey` `swipe` `systemPopups` `screenshot` `openUrl` `launchFresh`；`Session.swift` 另用 `/session/{open,close,list,relaunch-app,renew-activation}`（`grep -noE` 实测）。

**所以 C8 的第一步不是"改调"，是先把驱动面长齐** —— SDK 不可能调用 FFI 对象上还不存在的方法。缺的、且**有真 route** 的方法（映射见 §③）由 S1 补上；缺的、且**无 route**（宿主侧编排）的方法由 S2 移出协议。

### ③ 逐个 SDK 方法核 runner 真实注册表，分三堆

`smix_runner_client`（`crates/smix-runner-client/src/lib.rs`，`pub async fn` 实测）是干净的 wire 参考实现，含全部所需方法。把 SDK 的 13 个 runtime 方法 + 5 个 session route 按"有无真 route"分堆：

**A. 有真 route、C7 已封（4 个）**：`snapshotTree`→`SmixDriver.tree` · `launch/terminate`→`SmixSession.launch_app/terminate_app` · `open`→`SmixDriver.open_session`。

**B. 有真 route、S1 需补到驱动面（10 个）**：
- acting（挂 `SmixSession`，session-scoped）：`sendString`→`input_text` · `pressKey`→`press_key` · `swipe`→`swipe_once` · `systemPopups`→`system_popups` · `synthesizeTapAtNormalized`→`tap_at_norm_coord` · `synthesizeTap`（绝对像素，**无绝对像素 route**）→ App 已解析出目标 id，改走 `tap_by_id`（`client.rs:1210`；App 的 tap 路径本就是 snapshot→FFI resolveSelector→拿到节点，见 `App.swift:2,39,50`）。
- session 生命周期（挂 `SmixDriver`/`SmixSession`）：`close`→`close_session` · `list`→`list_sessions` · `relaunch-app`→`relaunch_session_app` · `renew-activation`→`renew_session_activation`。

**C. 无 route、S2 移出 SDK 协议（4 个，v2 break，C7 已记）**：`screenshot` · `openUrl` · `launchFresh` · `launchFromPath` —— 编排在 `smix-sdk::App`（依赖 `xcrun`/`adb`，宿主侧），而 FFI 产物在设备侧（xcframework 只有 ios-sim + macos slice；Android 是 minSdk 33 的 .aar）。**它们从未工作过**（都在那 13 个 404 里）；移出不是能力倒退。

### ④ resolver 已经在进程内、不走 HTTP —— 只有 TS 例外

**Swift 与 Kotlin 的 selector 解析早已走 FFI 进程内核心，不是 HTTP。** 实测：`App.swift:5` `import SmixCoreFFIBindings` + `:67` `resolveSelector(treeJson:selectorJson:)`；Kotlin `App.kt:37` `resolver.resolve(...)` 默认 `DefaultFfiResolver`（`SelectorResolver.kt:5` 注释：wraps `uniffi.smix.resolveSelector`）。UDL 同步侧 `resolve_selector{,_count,_labels}`（`smix.udl:26,36,39`）就是这个核心。**两个 SDK 都已依赖生成的 bindings 模块**（Swift `Package.swift:38` binaryTarget + `:45` `SmixCoreFFIBindings`；Kotlin `build.gradle.kts:68` jniLibs）—— 所以 S2 的 cutover 是**换驱动 transport**（把 `HttpSmixSimRuntime` 换成 `SmixDriver`/`SmixSession`），不是引入新依赖。

**TS 没有 FFI 通路**（§⑤），所以 TS 的 resolver 还在走 HTTP `/select/resolve{,-count,-labels}` ——**这 3 条是被 runner 真正服务的**（不在那 13 个欠账里），故 route-conformance 不报它们。TS 保留这个 3-route resolver 即可 rc=0。

### ⑤ TS 无任何 native 通路；napi 是一条**尚未搭建**的新分发轴

实测 `grep -rln "napi\|wasm-bindgen\|neon\|wasm-pack"`（`*.toml`/`*.json`/`*.sh`，排除 node_modules/target）→ **零命中**。`npm/smix-rn` 无 native dep、无 prebuilt `.node`、无 cross-compile 脚本。C7 的三方决策（`v2.md`）把 TS 定为 napi，并**明确记它是"新的 per-triple 分发轴"**。

**从零搭 napi = 跨 triple 预编译 `.node` 矩阵 + 构建脚本 + CI**，本身是 checkpoint 量级的分发工程，且与 C11 的 ship / SDK lockstep / 分发工作重叠。**本段不把它塞进来**（理由见文末"与冷计划不符" #3，且这是需用户拍板的 scope 决策，不是我省工）。故 C8 的 TS 半段 = 删掉 `HttpRunner.ts` 里那 13 条虚构驱动 route（它们从未工作过），保留 3-route resolver；TS 的动作级驱动**移除并标注 pending napi 轴**。

### ⑥ 测试策略：wire 只在 Rust 证一次，三个 SDK 停止 mock wire

本 cycle 反复记的账（`v2.md`）：三个 SDK 的驱动测试全注入 mock —— **它们验证的是 SDK 跟自己说话**，那正是虚构 wire 得以出厂的方式。cutover 后 `HttpSmixSimRuntime` 不复存在，注入式 mock transport **连编译都过不了**，是自然的强制函数。

- **驱动 wire 的真覆盖只有一处**：`crates/smix-ffi/tests/driving.rs`（C7 已建，`wiremock` 打真 HTTP 字节）。S1 补的每个方法在这里加真往返断言。
- **Swift SDK 测试**（`swift-bridge/Tests/SmixSDKTests/*`）可在 host macOS 加载 xcframework 的 macos-arm64 slice，覆盖 **FFI 之上** 的逻辑（selector 构造 / App 形状 / `ExpectationFailure` 文案）；驱动不再在此重测。
- **Kotlin SDK 单测**受平台限制：`SelectorResolver.kt:7` 注释实测 —— host JVM **加载不了 `libuniffi_smix.so`**（那是 Android-only jniLibs），故 Kotlin 单测**天然无法**在 host 驱动 FFI，只能覆盖 FFI 之上的逻辑、resolver 以平台原因保留 stub。**这不是"mock wire"反模式，是平台约束** —— 要写进测试注释区分清楚。

### ⑦ session_id 落点复核

`SessionAppLifecycleRequest.session_id` 必需字段实测在 `crates/smix-runner-wire/src/lib.rs:675`（`v2.md:129` 记的 `:632` 是错的，C7 已更正）。C7 的 `SmixSession` 已持 id 并在 launch/terminate 塞进 body；S1 补的 session 方法沿用同一形态。

## 步骤（线性，无分叉）

> 判据同 C6/C7 两次拆分（`v2.md`：拆的理由是**风险性质不同**，不是工作量）。本段三步的风险性质一致 —— 都是**发布物公开 API 的手术**，故不再拆为独立 checkpoint。**但本段确实是拆过之后仍偏大的一段**（跨 3 语言 + 一条被推迟的分发轴 + 多处 API break + 三套测试重写），是否再拆属 §10 用户权力，见文末 #6，不内部消化。

### S1. 补齐 `smix-ffi` 驱动面 —— 骨架吃不下 SDK 要的方法

**红（写测试）**

- 文件：`crates/smix-ffi/tests/driving.rs`（扩展 C7 的 wiremock 套件）
- 断言：对 §③-B 的 **10 个新驱动方法**，各加一条经 FFI 驱动面发真实 HTTP 往返的断言（`wiremock` 校验 method + path + body，acting 方法用 `body_string_contains` 断言 runner **真收到** `"sessionId"`）。当前红（方法不存在）。
- **不写 mock-only 断言**：`wiremock` 验真 HTTP 字节，非替身（`v2.md` 记的三 SDK 出厂虚构 wire 的病根）。
- 复用 C7 已证的两条形态：`until_cancelled` 的 `tokio::select!` cancel 路径、`#[uniffi::export(async_runtime = "tokio")]` 的真 reactor（UDL `[Async]` 会在无 reactor 上下文 panic 且编译期看不到）。

**绿（实现）**

- 文件：`crates/smix-ffi/src/driving.rs`
- API（薄封装 `smix_runner_client::HttpRunnerClient`，只封 client 不封 `smix-sdk::App`）：
  - `SmixSession` 上新增 acting 方法，各收 `Option<Arc<CancelToken>>`、经 `until_cancelled` 转发：
    `input_text(text) · press_key(key) · swipe_once(direction) · system_popups() -> String(JSON) · tap_at_norm_coord(nx, ny) · tap_by_id(id) -> bool`
  - session 生命周期：`SmixSession::close() · renew_activation() · relaunch_app()`；`SmixDriver::list_sessions() -> Vec<...>`
  - 无绝对像素 route：不提供 `synthesize_tap(px)`；App 的 center-tap 改用 `tap_by_id`（已解析出 id）。
- 关键点：acting 挂 `SmixSession`（携 session 绑定），`tree` 维持 C7 放在 `SmixDriver` 的位置；请求走 header 还是 body 由实现定，契约是"runner 收到 session_id"。

**重构**

- 跑 `bash scripts/dev/ffi-bindings-fresh.sh` 重生成两侧 bindings 并签入。C7 焊入的 gate 会验 bindings ⟷ xcframework/.so 符号一致；新方法必须同时出现在 `smix.swift`、`smix.kt` 与两个二进制的导出符号里，否则 rc=1（C7 已实测这条 gate 会因符号缺失报红）。

## Checkpoint C8 验收

```bash
# 1. 驱动面长齐：~10 个新方法真出现在 driving.rs（从源码数，不数记性）
grep -cE "pub (async )?fn " crates/smix-ffi/src/driving.rs
# 2. wiremock 真往返测试通过（真 HTTP 字节，非 mock 替身）
cargo test -p smix-ffi --test driving >/tmp/c8drv.out 2>&1; echo "driving rc=$?"
grep "^test result:" /tmp/c8drv.out
# 3. 新方法真进两侧 bindings 且二进制携带（C7 gate：符号缺失 = rc=1）
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"
grep -cE "func tapById|func inputText|func pressKey|func swipeOnce" \
     swift-bridge/Sources/SmixCoreFFIBindings/Generated/smix.swift
grep -cE "fun `?tapById|fun `?inputText|fun `?pressKey|fun `?swipeOnce" \
     android-runner/sdk/src/main/kotlin/uniffi/smix/smix.kt
# 4. route-conformance 仍红（三个 SDK 改调 = C9；本段不碰已发布物）
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$? (预期 1)"
# 5. 无回归
cargo test --workspace >/tmp/c8.out 2>&1; echo "cargo rc=$?"
grep -c "^test result: ok" /tmp/c8.out; grep -c "^test result: FAILED" /tmp/c8.out
cargo clippy --workspace --all-targets >/dev/null 2>&1; echo "clippy rc=$?"
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"
bash scripts/dev/fence-check.sh >/dev/null 2>&1; echo "fence rc=$?"
# 6. swift SDK 仍编译链接（xcframework 携带新符号；读 XCTest 行不是 swift-testing 行）
( cd swift-bridge && swift test >/tmp/c8sw.out 2>&1; echo "swift rc=$?"
  grep "Executed .* tests" /tmp/c8sw.out | tail -1 )
```

期望，逐条：

1. `driving.rs` 的 `pub fn` 计数 **≥14**（C7 基线 9；补齐的 ~10 个 acting/session 方法只增不减）。
2. **`driving rc=0`**；`test result: ok`，`0 failed`（每个新方法一条 wiremock 真往返 + 至少一条 cancel 断言）。
3. **`bindings-fresh rc=0`**（重生成 + 二进制符号一致，C7 已实测这条 gate 会因符号缺失报红）；Swift 计数 **≥2**、Kotlin 计数 **≥2**（新驱动方法真进 bindings）。
4. **`route rc=1`** —— 本段**故意不动** SDK,route-conformance 仍报 13 路由；它的 rc=0 是 **C9** 的出口,不是 C8 的。写在明处防止把 C9 的工作偷进本段。
5. `cargo rc=0`；`test result: ok` 计数 **≥132**（本段基线 132，S1 新增 driving 测试只增不减）；`FAILED` **0**；clippy `rc=0`；hygiene `rc=0`；fence `rc=0`。
6. **`swift rc=0`** 且 `Executed 360 tests … 0 failures`（本段不删 SDK 测试，总数不降；改的是 `smix-ffi`，Swift SDK 只需仍能链接到携带新符号的 xcframework）。

**仪器纪律**（本 cycle 反复吃亏，下列每条本 session 都亲手复现过，非转述）：

- **测退出码不接管道** —— `cmd | head; echo $?` 量的是 `head`（本 session 已犯三次，写在 `perf-decomposition-vs-polish.md` §1）。所有 rc 都 `>/dev/null 2>&1; echo "rc=$?"` 单独取，或落 `/tmp` 再读。
- grep 里的 `` fun `?tap `` 反引号 / glob **必须防 zsh word-split** —— 不带引号 zsh 直接 `no matches found`，整条不执行。
- `swift test` 同时打 swift-testing 的 `0 tests` 与 XCTest 的 `Executed N tests` —— grep 错行就是拿 0 个测试的绿冒充真身。
- **不在编译未完成时读测试输出** —— 本 session 踩过 `exit=101 / 22 buckets` 的假读数，真值是 132/0；落 `/tmp` 等命令整体结束再 grep。
- 第 1/3 组量的是**源码计数 / 符号**,不是文档排版。

**未被本 checkpoint 覆盖的**（写在明处，同 C3-C7 的教训：mock 与 gate 都证明不了真设备上的事）：

1. **`wiremock` 只证"FFI 真发出 HTTP + cancel 真取消 + runner 收到 session_id"，不证"请求在真 sim / 真 emulator 上被正确应答"。** 真设备 smoke 属 C12 ship gate。
2. **三个 SDK 本段一行不改** —— 删虚构 wire、改调驱动面全是 C9。C8 只让"改调"有对象可调。
3. **Kotlin 单测无法在 host 驱动 FFI**（平台约束，`SelectorResolver.kt:7`），故驱动正确性的真覆盖**收敛到一处**：本段的 Rust `wiremock`。三个 SDK 在 C9 之后都不再 mock wire。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c8-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 C9：三个 SDK 改调 / 删虚构 wire；Swift/Kotlin 换驱动 transport + reshape App，TS 删 13 route；`route-conformance.py` rc=0 是它的出口），见 CLAUDE.md §6

## 与冷计划不符之处（必须先读，不要隐瞒）

1. **冷计划把驱动面当已完整** —— C7 只落了 4 个方法（`tree` + session open/launch/terminate），实测 `driving.rs` 9 个 `pub fn`（含 3 个 CancelToken/id 辅助）。SDK App 层要 ~14 个。**所以"改调唯一那份 wire client"之前，必须先有 wire client 可调 —— 本段就是补齐它**，而冷计划把它和"三 SDK 改调"合成了一段。已拆：C8 补驱动面（本段），C9 三 SDK 改调（下一段），见 v2.md 决策日志 2026-07-18「拍板·拆 C8」。
2. **本段一行不改任何已发布 SDK** —— route-conformance 因此在 C8 出口**仍红**（rc=1，13 路由）。这不是遗漏，是边界：补能力（增量、不破坏公开面）与改 SDK（破坏公开 API）分属两段，gate 各自独立。route-conformance rc=0 是 C9 的出口。
3. **驱动正确性的真覆盖收敛到一处**（Rust `wiremock`）—— Kotlin host 单测加载不了 `libuniffi_smix.so`（`SelectorResolver.kt:7`），故不能靠"三语言各 mock 一套"。这比 mock 自证更诚实（mock 自证正是三 SDK 出厂虚构 wire 的病因）。C9 起三个 SDK 都不再 mock wire。

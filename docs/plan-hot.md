# plan-hot — v2 到 C9：Swift SDK 改调 FFI 驱动面

## 目标 checkpoint

C9：Swift SDK(`SmixSDK`)删掉 `HttpSmixSimRuntime.swift`(那 307 行虚构 wire),App 改持 `SmixDriver`(拿树)+ `SmixSession`(动作)双 handle、驱动全走 C8 补齐的 FFI 边界。6 处公开 API break 逐条落地并记决策日志。

**只做 Swift。** Kotlin = C10、TS = C11(用户 2026-07-18 拍板拆三段)。`route-conformance` 在 C9 出口**仍红**(Kotlin/TS 还引着虚构 route),它的 rc=0 是 **C11** 的出口 —— 整个 SDK 手术的收口。

## 前置条件

```bash
git branch --show-current                                 # 期望 feature/v2.0
git log --oneline -1                                      # 期望 694eb90cb（C8 hot plan 已归档）或其后
pgrep -fl "runner.ts|smix run|supervise"                  # 期望空（in-house batch 不活动）
pgrep -fl "gradle|mobilegate|emulator"                    # 期望空（S1/S2 要动 swift/gradle 构建）
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"       # 期望 0
bash scripts/dev/fence-check.sh >/dev/null 2>&1; echo "fence rc=$?"                          # 期望 0
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"          # 期望 0
python3 scripts/dev/route-conformance.py >/dev/null 2>&1; echo "route rc=$?"                 # 期望 1（起点即欠债，rc=0 是本段出口）
cargo test --workspace >/tmp/c9_base.out 2>/dev/null; echo "cargo rc=$?"
grep -c "^test result: ok" /tmp/c9_base.out                                                  # C8 基线 132
grep -c "test result: FAILED" /tmp/c9_base.out                                               # 期望 0
```

本次热化亲手实测通过的:`branch=feature/v2.0` · `git log -1 = 694eb90cb` · 两个 pgrep 空 · **hygiene 0 · fence 0 · bindings-fresh 0** · **route-conformance rc=1(13 route / 40 place)** · `driving.rs` **19 个 `pub fn`**。**`cargo test --workspace` / `clippy` 本次未重跑**,沿用 C8 实测基线(132 / 0 · clippy 0);执行者进入本段前须自行确认。

## 已确证的起点（本次热化实测，非转述）

### ① route-conformance 的 13 route,哪几个有 FFI 家、哪几个必须离开协议

实测 `route-conformance.py` 报的 13 route,逐个核 C8 的 `crates/smix-ffi/src/driving.rs`(19 个 `pub fn`)分三堆:

**A. 有 FFI 家,Swift/Kotlin 直接换调(9 个)**:
`/a11y/snapshot`→`SmixDriver.tree`(driving.rs:132)· `/sim/launch`→`SmixSession.launch_app`(:194)· `/sim/terminate`→`terminate_app`(:204)· `/input/send-string`→`input_text`(:239)· `/input/press-key`→`press_key`(:250)· `/input/swipe`→`swipe_once`(:266)· `/sim/system-popups`→`system_popups`(:276)· `/input/tap-normalized`→`tap_at_norm_coord`(:229)· `/input/tap`(绝对像素)→ **无绝对像素家**,改走 `tap_by_id`(:219)(App 已 resolve 出目标 id,不必再合成中心坐标)。

**B. 无 FFI 家,离开 SDK 协议(4 个宿主侧编排,v2 break,C7/C8 已记)**:
`/sim/screenshot` · `/sim/open-url` · `/sim/launch-fresh` · `/sim/launch-from-path` —— 编排在 `smix-sdk::App`(依赖 `xcrun`/`adb`,宿主侧),FFI 产物在设备侧,拿不到这些工具。**它们从来只打 404,一次没工作过**;移出不是能力倒退。

**5 处 API break** = 上面 B 的 4 个 + `synthesizeTap`(绝对像素,A 堆里那个"无家"的)—— 与 prompt 的 5 处一致(`v2.md` 2026-07-18 记)。

### ② `tree` 在 `SmixDriver` 上,不在 `SmixSession` 上 —— App 必须同时持有 driver + session

这是 prompt 的 measured-fact 没写清的一件事,C9 的 reshape 必须据此定形。`App.tap` 的流水是 **snapshot → resolve → act**:
- snapshot(`runtime.snapshotTree()` → `/a11y/snapshot`)映射到 **`SmixDriver.tree`**(`driving.rs:132`,在 driver 上);
- resolve(`resolveSelector(treeJson:selectorJson:)`,`App.swift:67`)走 FFI 自由函数,**不变**;
- act(`runtime.synthesizeTap(center)` → `/input/tap`)映射到 **`SmixSession.tap_by_id(firstId)`**(`driving.rs:219`,在 session 上)。

即 App 的一次 tap 同时用到 driver(拿树)与 session(动作)。`list_sessions` 也在 driver 上(`driving.rs:166`)。**所以 App 要持有 `SmixDriver` + `SmixSession` 两个 FFI handle,不是单个 session。** `Smix.launchApp` 的构造顺序:`SmixDriver::new(port)` → `driver.open_session(bundleId)` 得 `SmixSession` → `session.launch_app()` → `App(driver, session)`。

### ③ Swift/Kotlin 已依赖生成的 bindings,cutover 不引新依赖

`App.swift:5` `import SmixCoreFFIBindings`;`SmixDriver`/`SmixSession`/`tapById`/`inputText`/`openSession` 实测已在生成的 `swift-bridge/Sources/SmixCoreFFIBindings/Generated/smix.swift`(:717/:1002/:1262/:1088/:818)与 `android-runner/sdk/src/main/kotlin/uniffi/smix/smix.kt`(:1881/:2291/:2277/:2420/:2023)。C8 已让二进制携带这些符号(`bindings-fresh` 本次 rc=0)。**cutover 是换 transport,不是引入新依赖。**

### ④ Swift/Kotlin 的 `Session` 类还带一个 FFI 给不了的特性:X-Sim-Health 状态流

`Session.swift:85` 的 `stateStream`(Kotlin `Session.kt` 的 `stateFlow`)靠解析每个 HTTP 响应的 `X-Sim-Health` 头驱动(`HttpSmixSimRuntime.swift:246`)。**FFI 边界不透出 HTTP 响应头** —— `DriveError`(`driving.rs:22`)只带一个 message 字符串。所以换到 FFI `SmixSession` 后,**session 健康状态流这个特性会丢**,是 prompt 5 处 break 之外的**第 6 处 break**。Session 的其余方法都有 FFI 家:`stillValid`→`SmixDriver.list_sessions` · `relaunchApp`→`SmixSession.relaunch_app` · `renewActivation`→`renew_activation` · `close`→`close`。**本段把这处丢失记入 v2 决策日志,不试图在 FFI 上重造一套头透传**(那是 observability 的独立特性,§8.1 范围纪律)。

### ⑤ TS:删虚构驱动 route + 留 resolver;动作级驱动 pending napi

`HttpRunner.ts` 的 `HttpSimRuntime`(237 行)里 **13 个驱动方法**(`grep` 实测)打的全是虚构 route;而它的 `resolver` / `resolveCount` / `labelsResolver` 打的是 **被真正服务的** `/select/resolve{,-count,-labels}`(runner 为"TS 无 FFI"专门在 HTTP 上托管的 selector 核心,route-conformance 不报它们)。`__tests__/HttpRunner.test.ts` 同一文件里两类测试并存:resolver 测试(`/select/resolve*`,**保留**)+ 驱动 route 的 mock 测试(`/input/tap` 等,**删**)。

**诚实 scope(§13,非省工)**:仓库零 napi/neon/wasm 基础(C7/C8 实测),从零搭跨 triple 预编译 `.node` 矩阵是 checkpoint 量级的独立分发工程,与 ship 段重叠。**本段不交付可用 napi binding**。TS 半 = 删 13 条虚构驱动 route(达成 TS 那部分 rc=0)、保留 3-route resolver、动作级驱动移除并标注 pending napi 轴。**更深一层实情(记入报告)**:TS 的 sense 路径也依赖虚构的 `/a11y/snapshot` 取树,故删掉驱动后 TS 事实上无 live driving,只剩类型 + `Selector` + resolver seam ——这就是"从未工作过"的诚实收场。

### ⑥ 测试策略:wire 只在 Rust 证一次;SDK 停止 mock wire

cutover 后 `HttpSmixSimRuntime` 不复存在,注入式 mock transport **连编译都过不了**,是自然的强制函数。
- **Swift SDK 测试**(`swift-bridge/Tests/SmixSDKTests/*`)里 `*MockTests`(`AppTapMockTests` / `AppFillPressKeyMockTests` / `AppSwipeScreenshotMockTests` / `AppTapAtCoordAndAppPathMockTests` / `AppSenseExtMockTests` / `LocatorMockTests`)靠 `MockSimRuntime` 驱动 App —— App 改持**具体 FFI `SmixSession`**(非 protocol,无法 mock),这些**删**。保留 FFI 之上的纯逻辑测试(`SelectorFullSchemaTests` / `ExpectationFailureContractTests` / `MvpApiShapeTests` 收敛为 shape 断言)。
- **Kotlin SDK 单测**天然无法在 host 驱动 FFI(`SelectorResolver.kt:7`:host JVM 加载不了 `libuniffi_smix.so`),同样删 `*MockTest`,保留 selector/shape 纯逻辑。
- **TS**:删 `HttpRunner.test.ts` 的驱动 route mock,保留 resolver + `SelectorFullSchema` + `MvpApiShape`。
- **SDK 测试总数会降 —— 门是 0 failure,不是数目不减**。不许为凑数留一个 mock-wire 测试(那是 gaming,`v2.md` C8 教训)。

## 步骤（线性，无分叉）

> Kotlin(C10)/ TS(C11)已移出 —— 用户 2026-07-18 拍板拆三段。本段只做 Swift。

> 判据同 C6/C7/C8 三次拆分(`v2.md`:拆的理由是**风险性质不同**,不是工作量)。本段三步(Swift/Kotlin/TS cutover)风险性质**一致**——都是发布物公开 API 的手术,故同属 C9。**但本段确实跨 3 语言 + 5-6 处 API break + 三套测试重写,是拆过之后仍偏大的一段**;是否再拆为 C9a/b/c 属 §10 用户权力,见文末,不内部消化。route-conformance rc=0 只在三步全落后达成(任一 route 只要还有一个 SDK 引用就仍报)。

### S1. Swift SDK 换 FFI 驱动面 + reshape App/Session/Smix.launchApp

**红（写测试）**
- 文件:`swift-bridge/Tests/SmixSDKTests/`
- 断言:删除靠 `MockSimRuntime` 驱动 wire 的 `*MockTests`(见起点 ⑥);把保留的 shape/selector/ExpectationFailure 测试改到新构造形态(App 由 FFI `SmixDriver`+`SmixSession` 构造)。当前红:`SmixSimRuntime` 删除后旧 mock 测试引用不存在的类型,编译失败。
- **不写 mock-wire 断言**:驱动真覆盖在 Rust wiremock(C8),Swift 侧只证 FFI 之上的逻辑。

**绿（实现）**
- 删 `swift-bridge/Sources/SmixSDK/HttpSmixSimRuntime.swift`(307 行,13 虚构 route 载体)、`SimRuntime.swift`(`SmixSimRuntime` protocol)。
- `App.swift`:改持 `SmixDriver` + `SmixSession`(起点 ②)。`tap`/`fill` 的 snapshot 走 `driver.tree()`(得 JSON,直接喂 `resolveSelector`,再 parse 回 `A11yNode` 供 `findById`/`visibleElements`),act 走 `session.tap_by_id(firstId)`;`pressKey`/`swipe`/`fill` 的键入走 `session.press_key`/`swipe_once`/`input_text`;`tapAtCoord`→`session.tap_at_norm_coord`;`systemPopups`→`session.system_popups`;`terminate`→`session.terminate_app`;`relaunch`→`session.relaunch_app`。
- **移除 5 处 API break**(起点 ①-B + synthesizeTap):`App.screenshot()` · `App.openUrl(_:)` · `App.launchFresh(...)` 删除;`AppTarget.appPath`/`Smix.launchApp(.appPath)` 因 `launchFromPath` 无 FFI 家而破坏(仅留 `.bundleId` 路径,或改为宿主侧编排 —— 本段只做移除,不重造)。
- `Session.swift`:HTTP `Session` 类由 FFI `SmixSession` 顶替。`stillValid`→`driver.list_sessions`、`relaunchApp`→`session.relaunch_app`、`renewActivation`→`renew_activation`、`close`→`close`;**X-Sim-Health `stateStream` 丢失**(起点 ④,记决策日志)。
- `Smix.swift`:`launchApp(_ target:, runtime: SmixSimRuntime)` 改签名为经 FFI 构造(`SmixDriver::new(port)` → `open_session` → `launch_app`),返回持 driver+session 的 `App`。
- 关键点:App 现在同时握 `SmixDriver`(tree/list)与 `SmixSession`(act);`driver.tree()` 返回 JSON String,与 `resolveSelector` 的 `treeJson` 入参同形,省一次 re-encode。

**重构**
- 无新增结构坏味则跳过。不"顺便"改 Kotlin/TS(§8.1)。

## Checkpoint C9 验收

```bash
# 1. Swift SDK 的虚构 wire 已删(从源码数,不数记性)
test -e swift-bridge/Sources/SmixSDK/HttpSmixSimRuntime.swift && echo "STILL PRESENT" || echo "deleted"
# 2. Swift App 真的走 FFI 驱动面(SmixDriver/SmixSession 引用出现在 App/Session/Smix)
grep -rcE "SmixDriver|SmixSession" swift-bridge/Sources/SmixSDK/App.swift swift-bridge/Sources/SmixSDK/Smix.swift 2>/dev/null | grep -v ":0$"
# 3. Swift SDK 只对 SDK 自己的路由说话:HttpSmixSimRuntime 删除后 SmixSDK 不再含任何虚构驱动 route
grep -rcE "/sim/launch|/a11y/snapshot|/input/tap|/sim/screenshot|/sim/open-url" swift-bridge/Sources/SmixSDK/ 2>/dev/null | grep -v ":0$" || echo "SmixSDK: no fictional routes"
# 4. Swift SDK 测试:0 failure(读 XCTest "Executed N" 行,不读 swift-testing 的 "0 tests" 行)
( cd swift-bridge && swift test >/tmp/c9sw.out 2>&1; echo "swift rc=$?"
  grep "Executed .* tests" /tmp/c9sw.out | tail -1 )
# 5. route-conformance 仍红(Kotlin/TS 未改 = C10/C11;rc=0 是 C11 出口,不是 C9)
python3 scripts/dev/route-conformance.py >/tmp/c9route.out 2>&1; echo "route rc=$? (预期 1)"
grep -oE "[0-9]+ route\(s\)" /tmp/c9route.out | head -1
# 6. 无回归:Rust / clippy / hygiene / fence / bindings-fresh
cargo test --workspace >/tmp/c9.out 2>&1; echo "cargo rc=$?"
grep -c "^test result: ok" /tmp/c9.out; grep -c "^test result: FAILED" /tmp/c9.out
cargo clippy --workspace --all-targets >/dev/null 2>&1; echo "clippy rc=$?"
python3 scripts/dev/hygiene-scan.py --noise-only >/dev/null 2>&1; echo "hygiene rc=$?"
bash scripts/dev/fence-check.sh >/dev/null 2>&1; echo "fence rc=$?"
bash scripts/dev/ffi-bindings-fresh.sh >/dev/null 2>&1; echo "bindings-fresh rc=$?"
```

期望，逐条:

1. `deleted`(307 行虚构 runtime 已删)。
2. App.swift + Smix.swift 的 `SmixDriver|SmixSession` 引用计数 **≥1**(App 真持双 handle、驱动走 FFI)。
3. `SmixSDK: no fictional routes`(整个 SmixSDK 源码树不再含任何 runner 不服务的驱动 route)。
4. **`swift rc=0`** 且 `Executed N tests … 0 failures`。**N 会低于 C8 的 360**(SmixSDKTests 的 mock-wire 测试已删);门是 **0 failures**,不是 N 不减(留 mock 测试凑数 = gaming)。
5. **`route rc=1`** —— 本段**只改 Swift**,Kotlin/TS 仍引虚构 route,route-conformance 仍报红。它的 rc=0 是 **C11**(TS)的出口,写在明处防止把 C10/C11 的工作偷进本段。
6. `cargo rc=0`;`test result: ok` ≥ **132**(Rust 不动,不回退)、`FAILED` **0**;clippy `rc=0`;hygiene `rc=0`;fence `rc=0`;**bindings-fresh `rc=0`**(本段不碰 `smix-ffi`,bindings 逐字节不变)。

**仪器纪律**（本 cycle 反复吃亏，下列本 session 已亲手复现）:
- **测退出码不接管道** —— `cmd | head; echo $?` 量的是 `head`（`perf-decomposition-vs-polish.md` §1；本 cycle 已犯多次）。所有 rc `>/dev/null 2>&1; echo "rc=$?"` 单独取,或落 `/tmp` 再 grep。
- `swift test` 同时打 swift-testing 的 `0 tests` 与 XCTest 的 `Executed N tests` —— grep 错行就是拿 0 个测试的绿冒充真身。
- **不在编译未完成时读测试输出**(C7 踩过 `exit=101 / 22 buckets` 假读数);落 `/tmp` 等命令整体结束再 grep。
- **SDK 测试总数会降是预期的**,别把"数目下降"读成回归 —— 判据是 failures=0。

**未被本 checkpoint 覆盖的**（写在明处,同 C3-C8 教训:mock 与 gate 证明不了真设备上的事）:
1. **本段只做 Swift** —— Kotlin(C10)/ TS(C11)一行不改,route-conformance 因此仍红。这不是遗漏,是拆分边界。
2. **驱动真覆盖在 C8 的 Rust wiremock,不在 Swift 侧** —— Swift SDK 测试只证 FFI 之上的逻辑(shape/selector/failure),停止 mock wire。
3. **X-Sim-Health session 健康状态流丢失**(起点 ④)—— FFI 不透传 HTTP 头,该特性无 FFI 家;重造它是 observability 独立特性,不塞进本段。screenshot/openUrl/launchFresh 移除、走 CLI(用户 2026-07-18 拍板)。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c9-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 C10:Kotlin SDK 改调 FFI 驱动面,同 C9 逐项对应),见 CLAUDE.md §6

## 与冷计划不符之处（必须先读，不要隐瞒）

1. **驱动 handle 是两个,不是一个** —— `tree`/`list_sessions` 在 `SmixDriver`(`driving.rs:132,166`),acting 在 `SmixSession`,App 一次 tap 同时用到两者。冷计划说"换到 `SmixSession`"不准,**App 必须持双 handle**。
2. **API break 是 6 处**(冷计划记 5):第 6 处 = `Session` 的 **X-Sim-Health 状态流丢失**(`Session.swift:38` 的 `stateStream`/`state`)——FFI 边界不透 HTTP 响应头,该特性无 FFI 家。用户 2026-07-18 拍板:screenshot/openUrl/launchFresh 一并移除、走 CLI(它们本就依赖从未工作过的虚构 wire)。
3. **本段只做 Swift** —— 用户 2026-07-18 拍板拆三段(C9 Swift / C10 Kotlin / C11 TS)。route-conformance rc=0 是 C11 的整体收口,不是 C9 的出口。
4. **驱动真覆盖不在 Swift 侧** —— 在 C8 的 Rust wiremock。Swift SDK 测试删 mock-wire(那正是虚构 wire 出厂的病因),只测 FFI 之上的逻辑。

# plan-hot — v2.10 到 C1：跨平台 recorder —— 先调研 Android 事件采集可得性

## 目标 checkpoint

C1：**「Android 能不能捕获对等 iOS `EventRecorder` 的 tap/input 事件流、并重建成 `IRAction` 序列?」这个 v2.10 冷计划头号风险，被一份带证据的调研文档有据地回答**，落成三选一 verdict（`OBTAINABLE` / `NOT-OBTAINABLE` / `PARTIAL`），并把由此得出的 tier 决策写进 `docs/v2.md` 决策日志。

C1 是**研究先行 checkpoint**（同 v2.8-C7 遮挡 z 序调研范式），只回答可得性、不做实现、不跑设备。verdict 出来后走 Android 采集腿实现（C2）还是诚实 re-tier（把 Android 采集标 structurally-blocked），由**下一段热计划**按结论热化 —— 本段**不写任何实现步骤、不建任何 record 采集代码**（§2 无分叉铁律：`OBTAINABLE→做采集腿` vs `NOT-OBTAINABLE→re-tier` 是一个分叉，必须留到分叉点已被 verdict 决定之后再热化）。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
grep -q 'pub enum IRAction' crates/smix-authoring-ir/src/lib.rs                 # 平台无关 IR stone 在（映射目标）
grep -q 'generate_maestro_yaml' crates/smix-recorder/src/generator_maestro_yaml.rs  # generator steel 在
grep -q 'installSwizzle' swift-bridge/SmixRunnerUITests/EventRecorder.swift     # iOS 采集腿在（参照实现）
grep -q 'record/start' swift-bridge/Sources/SmixRunnerCore/RecordRoute.swift    # iOS /record wire 在
grep -q 'inst.uiAutomation' android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt  # Android instrumentation 采集面宿主在
test -d docs/research                                                            # 调研文档目录在（C7 已建）
```

全部 exit 0 = 可开研究。任一失败 → 按 §6「何时该拒绝热化」回报上层，不硬开。

## 已经查清、不必重查的事实

- **iOS 采集机制（参照系 —— Android 要对等的就是这条）**：`EventRecorder.swift` 在 runner（XCTest 宿主）进程内 **swizzle `XCAXClient_iOS` 的 `handleAccessibilityNotification:fromElement:payload:`**（`installSwizzle`，EventRecorder.swift:95-127），劫持系统**无障碍通知（AX notification）流**。捕获的是 OS 级语义事件、不是原始触摸坐标：
  - `classifyKind`（EventRecorder.swift:479-487）把 AX notification 名映射成 kind：`hidevent`→`tap`（rawCode 1028 = `kAXHIDEventReceivedNotification`）/ `firstresponder`→`focus`（1018 = `kAXFirstResponderChangedNotification`）/ `usertesting`→`snapshot`（4002）/ `orientation`。
  - `makeEvent`（EventRecorder.swift:317-403）从 payload（binary plist）+ 元素抽 `selectorHints` / `frame` / `elementType` / tap 中心 `location`，产 `RecordedEvent`。
  - **要点**：iOS 采集的是**语义、携元素的无障碍事件流**（非 raw HID coord），这决定了「Android 对等物」应先在**同类语义事件流**里找，而非在 raw MotionEvent 里找。
- **iOS record wire + host reconcile**：`RecordRoute.swift` 提供 `POST /record/start` / `GET /record/poll` / `POST /record/stop`（RecordRoute.swift:4）；`RecordedEvent`（crates/smix-runner-wire/src/lib.rs:462 附近）只硬依赖 `raw_code` + `timestamp_ms`，其余 enrich 字段走 `extra` flatten。host 侧 `smix-sdk/src/capsule.rs:50 reconcile()` 把 focus-change 事件与 issued action 按时间窗归并；generator（`smix-recorder`）消费 `IRAction[]` 出 maestro yaml / rust。
- **IRAction 变体全表（映射目标，crates/smix-authoring-ir/src/lib.rs:33-100）**：`Tap` / `Fill` / `Clear` / `PressKey` / `Swipe`（带 `direction` + 可选 `from` anchor）/ `GoBack` / `WaitFor` / `HideKeyboard`。其中 `WaitFor` 是**生成期合成**（playback gate），任何平台都不「录」它 —— 不进「最小可移植采集集」的分子。
- **Android runner 现状（采集腿宿主，android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt）**：instrumentation 测试 `runServerForever()` 已持有 `inst.uiAutomation`，并已改 `serviceInfo.flags`（`FLAG_RETRIEVE_INTERACTIVE_WINDOWS` + `FLAG_REPORT_VIEW_IDS`，RunnerTest.kt:59-66）供 `/tree` 遍历。`android-runner/app/src/main/kotlin/dev/smix/runner/` 现有文件（`RunnerWire.kt` / `ScreenshotPacer.kt` / `SessionTable.kt` / `AppAliveCache.kt` / `SmixRunner.kt`）**无任何 `/record` 采集面**（grep `record` 仅命中截图计时 `ScreenshotPacer.record`）。
- **关键边界事实（直接关涉冷计划头号风险与 caller 点名的「app 侧权限/manifest」担忧）**：`inst.uiAutomation` 是 instrumentation **内置的特权无障碍接口**，runner 已经在用它。它**不是** app 侧声明的 `AccessibilityService`，**不需要**独立 service app / manifest `<service>` 声明 / app 侧运行时权限。故「Android 无障碍事件流」若经 `UiAutomation` 触达，**留在既有 runner-instrumentation 边界内**，不越界到被测 app。这条只是「宿主可用」的事实，**采集面能否真的拿到 tap/input 事件流仍是本调研要证的开放问题**，不得当结论。
- **§9#8 归位（不变量）**：录制采集是**感知（sense）**能力，落 android-runner core 平铺面（与 iOS runner 的 `/record/*` 同层、与 `/tree` `/tap` 同层），**不得埋进 driver**。故「拿得到」= 「作为 core 采集面拿得到」。
- **§9#1 守（不变量）**：本段全程 read-only 调研，**不跑 emulator / 不跑设备 / 不起 runner**；Android 轴是模拟器/instrumentation 采集，不引入任何真机路径。

## 本段预先定死的口径（防 scope 漂移与自欺）

- **全程 read-only**：读 iOS `EventRecorder` 作参照、读 Apple/Android 官方文档、读 android-runner 现状源码、WebSearch/WebFetch 查 `AccessibilityEvent` / `UiAutomation` / `UiWatcher` / instrumentation 触摸拦截 API 边界。**不 edit 任何实现代码、不建 record 采集类、不跑设备、不起 runner**。唯一的写是：调研文档 + 一条 §10 决策日志。
- **三条采集面轴，逐一枚举、勿预设结论**：本调研须对三个候选采集面各自独立评估「能否重建 tap/input 事件流」——
  1. **AccessibilityService 事件轴**：`AccessibilityEvent`（`TYPE_VIEW_CLICKED` / `TYPE_VIEW_TEXT_CHANGED` / `TYPE_VIEW_SCROLLED` / `TYPE_VIEW_LONG_CLICKED` / `TYPE_WINDOW_STATE_CHANGED` / …）能否经 **`UiAutomation.setOnAccessibilityEventListener`**（instrumentation 进程内、既有边界内）拿到系统级事件流。这是 iOS swizzled AX-notification 回调的**结构对等候选**。
  2. **UiAutomator 轴**：`UiWatcher` / `UiDevice` 是**主动查询/条件轮询**（`runWatchers` / `waitForIdle` 触发）还是**被动事件流**？无被动流 = 无法重建序列。
  3. **instrumentation 触摸 hook 轴**：androidTest 内 `Instrumentation` 的触摸/输入回调能否**跨进程**拦截被测 app（SUT）的 `MotionEvent` / `KeyEvent`？还是只见 instrumentation 自己进程的事件（进程边界阻断）？
- **no-ceiling-words（`.claude/rule/decomposition-discipline.md` debug/no-ceiling-words）**：任一轴判 `NOT-OBTAINABLE`，**必须附上该轴穷尽枚举的 API 面证据**（枚举出的事件类型表 / 监听 API 签名 / 官方文档明文的能力边界 + URL）作为「已穷尽」的举证，**不许**用「Android 做不到 / 平台限制 / 结构性 gap」hand-wave 收尾。负向结论的举证责任 = 把找过的采集面全列出来。
- **verdict 恰为三选一 token**（是**产物**，本热计划不据它分叉）：
  - `OBTAINABLE` —— 至少一条采集面轴能重建**最小可移植动作集**（见下）的 tap/input 序列；
  - `NOT-OBTAINABLE` —— 三条轴穷尽枚举后**无任一**能重建 tap/input 序列（附三轴全枚举证据）；
  - `PARTIAL` —— 部分动作可采（如 Tap/Fill 可、Swipe/PressKey 不可），或部分轴可、部分轴不可。
- **「最小可移植动作集」须显式判定**：调研须产一张 **IRAction 变体 × 三平台可采性** 对照表（iOS 参照列已知：Tap/Fill/Clear/Swipe/PressKey/GoBack/HideKeyboard 经 AX notification 采得；`WaitFor` 是合成、不录），逐格填 Android 该变体是否有对应可捕获事件。「三平台都该能录」的交集 = 最小可移植集；Android 采不到的记为 gap 单列（对策依冷计划：不扩 IR 迁就单平台）。
- **tier 决策无条件落一条**：无论 verdict 是哪个，都往 `docs/v2.md` 决策日志写**一条**记「verdict + 由此的 tier（Android 采集腿实现 / structurally-blocked re-tier）」的行（内容随 verdict 变、存在性不变 = 不构成分叉）。

## 步骤（线性，1 个）

### S1. 调研 Android 事件采集可得性，落成带证据的 verdict + tier 决策

**红（先写「什么证据能证伪『Android 拿得到对等采集流』」）**
- 文件：`docs/research/c1-android-capture.md`
- 先写 `## Falsification rubric` 段，**在收集证据之前**把判据钉死，每条判据的 `Evidence:` 槽先留空（证明结论非事后合理化）。至少覆盖三条采集面轴各自的证伪/证实条件：
  - **轴 1（AccessibilityService 事件）**：判 `OBTAINABLE-1` 的充分证据 = 存在一个 instrumentation 进程内、既有 `UiAutomation` 边界内可注册的**系统级 `AccessibilityEvent` 监听 API**（头号候选 `UiAutomation.setOnAccessibilityEventListener`），且其事件类型集合含**足以区分 tap（`TYPE_VIEW_CLICKED`）与文本输入（`TYPE_VIEW_TEXT_CHANGED`）**的判别项、事件携带 source 元素（可抽 selectorHints/frame 对等 iOS）；判 `NOT-OBTAINABLE-1` 的充分证据 = 枚举出的 `AccessibilityEvent` 类型表 + 监听 API 面里**无**任何能被动接收 tap/input 语义事件的路径（须列全表证明穷尽）。
  - **轴 2（UiAutomator watcher/device）**：判 `OBTAINABLE-2` 的充分证据 = `UiWatcher` / `UiDevice` 存在**被动、无需主动轮询**的事件回调流；判 `NOT-OBTAINABLE-2` 的充分证据 = 官方文档明证 `UiWatcher` 仅在 `runWatchers()`/`waitForIdle` 时按条件求值、`UiDevice` 仅主动查询（dump/find），**无被动事件流**（附 API 文档 URL）。
  - **轴 3（instrumentation 触摸 hook）**：判 `OBTAINABLE-3` 的充分证据 = androidTest 内某 `Instrumentation`/`UiAutomation` API 能**跨进程**接收 SUT 的 `MotionEvent`/`KeyEvent`；判 `NOT-OBTAINABLE-3` 的充分证据 = API 面证明 instrumentation 触摸回调只覆盖自身进程 activity、被进程边界阻断（附文档/API 签名）。
  - **可移植集判据**：产 `IRAction × {iOS, Android}` 采性对照表，标出「三平台交集（最小可移植集）」与「Android gap」。
- 跑一次断言 rubric 段存在且判据非空（红态 = 证据槽全空）：
  ```bash
  grep -q '^## Falsification rubric' docs/research/c1-android-capture.md && ! grep -q '^VERDICT:' docs/research/c1-android-capture.md
  ```
  期望：exit 0（rubric 已立、verdict 尚未下 = 证据未填，先失败一次证明不是倒着写）。

**绿（read-only 收证据 → 下 verdict → 落 tier 决策）**
- 文件：`docs/research/c1-android-capture.md` 补 `## Evidence` 段，逐条填 rubric 的 `Evidence:` 槽，每条带**可复核出处**（Android API 类名 / 方法签名 / `developer.android.com` 文档 URL / `RunnerTest.kt:line` 现状引用 / iOS `EventRecorder.swift:line` 参照）。至少产出：
  1. **轴 1 全枚举**：`AccessibilityEvent` 事件类型全表（tap/input/scroll/window 相关项标出）+ `UiAutomation.setOnAccessibilityEventListener` / `OnAccessibilityEventListener` API 签名 + 该监听是否在既有 `inst.uiAutomation`（RunnerTest.kt:59-66）边界内可注册（不需 app 侧 manifest `<service>` / 权限）。评估事件是否携 source `AccessibilityNodeInfo` 以抽 selectorHints/frame（对等 iOS `makeEvent`）。
  2. **轴 2 全枚举**：`UiWatcher` 契约（`checkForCondition` 触发时机）+ `UiDevice` 查询 API 面，明证有无被动事件流。
  3. **轴 3 全枚举**：`Instrumentation` 触摸/输入回调 API + 跨进程边界文档，明证能否拦截 SUT 事件。
  4. **可移植集对照表**：填满 `IRAction × {iOS, Android}`，交集 + Android gap 各列出。
- 文档末尾下一行 `VERDICT: <OBTAINABLE|NOT-OBTAINABLE|PARTIAL> — <一句话依据>`。
- 文件：`docs/v2.md` 决策日志（`## 决策日志（v2 cycle）` 段）末尾加**一条**（§10 格式）：`- {date} v2.10-C1 Android 事件采集可得性调研结论：{verdict}，tier→{Android 采集腿实现 / structurally-blocked re-tier} 理由：见 docs/research/c1-android-capture.md`。
- 跑一次断言 verdict 已定且证据在：
  ```bash
  grep -Eq '^VERDICT: (OBTAINABLE|NOT-OBTAINABLE|PARTIAL)\b' docs/research/c1-android-capture.md && grep -q '^## Evidence' docs/research/c1-android-capture.md
  ```
  期望：exit 0。

**重构（可选）**
- 仅整理文档标题层级 / 确认对照表可解析；不改任何 verdict 内容。

## Checkpoint C1 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
test -f docs/research/c1-android-capture.md \
  && grep -q '^## Falsification rubric' docs/research/c1-android-capture.md \
  && grep -q '^## Evidence' docs/research/c1-android-capture.md \
  && grep -Eq '^VERDICT: (OBTAINABLE|NOT-OBTAINABLE|PARTIAL)\b' docs/research/c1-android-capture.md \
  && grep -q 'c1-android-capture' docs/v2.md \
  && echo C1-PASS
```

期望：stdout 打印 `C1-PASS`，exit 0。含义 = 调研文档存在 + 有先立的证伪 rubric + 有证据段 + 有三选一 verdict + `docs/v2.md` 决策日志有一条引用该调研文档的 tier 决策行。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.10-c1-hot.md`。
2. 决策日志已在 S1 绿态写入（verdict + tier）；无需重复。
3. 调 sub-agent 热化下一段，见 CLAUDE.md §6：
   - verdict = `OBTAINABLE` / `PARTIAL` → 热化 **C2（Android 采集腿实现）**：按调研确认的采集面（大概率轴 1 `UiAutomation.setOnAccessibilityEventListener`）在 android-runner core 补 `/record/*` 对等路由 + `AccessibilityEvent → IRAction` 纯映射，PARTIAL 时仅覆盖可移植集、gap 单列。
   - verdict = `NOT-OBTAINABLE` → **re-tier**：把 Android 采集腿标 structurally-blocked，冷计划 C2/C4 相应改写（Android parity 降级或另寻采集面），由用户 / 上层 agent 拍板。
   - **本段不自作主张展开任一分支** —— 由 verdict 决定后再热化，插入时机由用户 / 上层 agent 拍板。

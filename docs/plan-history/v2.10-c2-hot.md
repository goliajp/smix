# plan-hot — v2.10 到 C2:Android 采集腿 —— UiAutomation AccessibilityEvent → IRAction + `/record/*` wire + emulator 设备 e2e

## 目标 checkpoint

C2:**Android runner 能被动采集 tap/fill/clear 三动作(经 `UiAutomation.OnAccessibilityEventListener`,C1 verdict=PARTIAL 认定的采集面),纯映射成 `IRAction`,经 android-runner 的 `/record/*` 路由吐出;真 emulator 上录一段固定脚本 → `/record/stop` 收到断言得住的 `IRAction[]` 序列 → `smix-recorder` generator 出非空 maestro YAML + Rust → 回放绿。** 通过后,「录一遍生成 flow」不再只对 iOS 成立,Android 这条采集腿从 C1 的「可得性已证」变成「真能录、真能生成、真能回放」。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— C1 verdict + 参照实现 + 采集腿宿主 ——
grep -Eq '^VERDICT: PARTIAL' docs/research/c1-android-capture.md          # C1 结论 = PARTIAL(Tap/Fill/Clear 可采)
grep -q 'pub enum IRAction' crates/smix-authoring-ir/src/lib.rs           # 平台无关 IR stone(映射目标)
grep -q 'pub fn generate_maestro_yaml' crates/smix-recorder/src/generator_maestro_yaml.rs  # generator steel(消费 IRAction[])
grep -q 'pub fn generate_rust' crates/smix-recorder/src/generator_rust.rs
grep -q 'inst.uiAutomation' android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt  # 采集腿宿主(instrumentation UiAutomation)
test -d android-runner/app/src/test/kotlin/dev/smix/runner                # JVM 单测源集在(纯映射红绿落此,无设备)
# —— Android 设备纪律(钉 serial + 让位;物理机 R5CT52DF07D 同时在场)——
adb devices | grep -q 'emulator-5554'                                     # 目标 emulator 在
```

全部 exit 0 = 可开工。任一失败 → 按 §6「何时该拒绝热化」回报,不硬开。

**Android 设备纪律(整段强制,写死不可省)**:
- **所有 adb / gradle 设备动作必须钉 `export ANDROID_SERIAL=emulator-5554`**(或 `adb -s emulator-5554 …`)。本机连着物理机 `R5CT52DF07D`(SM_S9010),`gradlew install*` / 未钉 serial 的 mutation 会装到**所有设备含物理机**(见 memory `android_gradle_installs_to_all_devices`)。`scripts/dev/adb-guard.sh` hook 会拦未钉 serial 的 mutation —— 别绕它,钉 serial。
- 任何设备操作前 `adb devices` 确认 + **让位 batch-owner**(先 `pgrep -f 'runner.ts|smix run|supervise'` 查占有者,不干扰他人活动 batch;见 memory `runner_ops_check_batch_owner_first`)。
- S1/S2 的红绿是 **JVM 单测 + Rust 单测 + route 装配,零设备**;只有 S3 上 emulator。S3 收尾 `scripts/dev/simx-sweep.sh` 一键清,不 shutdown all。

## 已经查清、不必重查的事实

- **C1 采集面结论(采集腿地基)**:`android.app.UiAutomation.setOnAccessibilityEventListener(OnAccessibilityEventListener)` 在既有 `inst.uiAutomation`(`RunnerTest.kt:61-65` 已用、已改 `serviceInfo.flags`)边界内注册,**不需 app 侧 `AccessibilityService`/manifest/权限**。`TYPE_VIEW_CLICKED`→Tap、`TYPE_VIEW_TEXT_CHANGED`→Fill/Clear(各携 source node `viewIdResourceName`)。Swipe(`TYPE_VIEW_SCROLLED` 有损)/PressKey/GoBack/HideKeyboard **不采,记 gap,不扩 IR 迁就**(C1 VERDICT + `docs/research/c1-android-capture.md` 事件表)。
- **【关键翻盘·直接决定 C2 架构】iOS 的 `RecordedEvent`/`/record/*` wire 根本不喂 generator**。读全链源码核实:
  - `crates/smix-recorder/`(session.rs + generator)消费的是 **`IRAction[]`**;喂它的是 `RecordingApp`(`session.rs:105-125`)—— host 侧 SDK wrapper,把自己发出的 `app.tap()/fill()/clear()` **直接**记成 `IRAction`,**不经任何 wire、不经 RecordedEvent**。
  - iOS 的 `/record/*` + `RecordedEvent`(swift `EventRecorder.swift` swizzle → `crates/smix-runner-wire/src/lib.rs:463`)那条被动 AX 流,**唯一消费者是 `crates/smix-sdk/src/capsule.rs::reconcile()`** —— 它只按 `raw_code==1018`(focus-change)做**时间窗归属**(判「这次 focus 变化是不是我发的动作引起的」),**从不构造 IRAction**。
  - 全 workspace grep 确认:**不存在 `RecordedEvent → IRAction` 桥函数**。CLI 的 `smix authoring record`(`authoring.rs:310 cmd_record_session`)是**树快照 assertVisible 脚手架**,也不走事件流。
  - ⇒ **「录事件 → 过 wire → 喂 generator」这条链 iOS 上并不存在**;caller 担忧的「iOS record wire 是否真注册进 runForever」答案:**真注册**(`SmixRunnerServer.swift:2225-2262` `if let recordHandlers { appendRoute("POST /record/start"|"/record/stop"|"GET /record/poll") }`,call site `SmixRunnerUITests.swift:2578` 真传 `recordHandlers: recordEnabled ? RecordHandlers(...)`),不是 selectResolveHandler 那种虚构 wire —— 但它喂的是 capsule-reconcile,不是 generator。
- **iOS record wire 真装配证据(逐字)**:`SmixRunnerServer.swift:1425 recordHandlers: RecordHandlers? = nil` 形参 → :2225 `if let recordHandlers` gate → 三条 `appendRoute`;`SmixRunnerUITests.swift:2578` 传入非 nil。与 selectResolveHandler(:1426 形参 + :1473 gate,同款「有参就装」)同构且**都真传了** —— 无「有参没传」缺口。
- **IRAction 变体 + 字段(映射目标,`smix-authoring-ir/src/lib.rs:33-100`,serde `tag="kind", rename_all="camelCase"` 内部标签)**:
  - `Tap { selector: Selector, timestampMs: f64 }` → JSON `{"kind":"tap","selector":{...},"timestampMs":N}`
  - `Fill { selector, text: String, timestampMs }` → `{"kind":"fill","selector":{...},"text":"...","timestampMs":N}`
  - `Clear { selector, timestampMs }` → `{"kind":"clear","selector":{...},"timestampMs":N}`
- **Selector JSON 形(`smix-selector/src/lib.rs:324` `#[serde(untagged)]`)**:`Selector::Id { id, ..modifiers }` 序列化为 **`{"id":"btn-xyz"}`**(untagged,无 type tag)。Android `viewIdResourceName` 剥 `:id/` 前缀(parity 既有 `substringAfter(":id/")` / `RunnerWire.shortResourceId`)→ 填 `{"id": short}`。
- **generator 消费点(steel,已在,直接复用)**:`generate_maestro_yaml(actions:&[IRAction], app_id:&str) -> Result<String,_>`(Tap→`tapOn`;Fill→`tapOn`+`inputText`;Clear→`eraseText:100`);`generate_rust(actions:&[IRAction], ...)`(Tap→`app.tap`;Fill→`app.fill`;Clear→`app.clear`)。**C2 不动 generator,只喂它 IRAction[]**。
- **Android runner 现状(采集腿宿主)**:`RunnerTest.kt runServerForever()` 持 `inst.uiAutomation` + 设 `serviceInfo.flags`(`FLAG_RETRIEVE_INTERACTIVE_WINDOWS|FLAG_REPORT_VIEW_IDS`);`SmixHttpServer.serve()`(NanoHTTPD,每连接独立线程,body 在 `serve` 顶部统一 drain 进 `drainedBody` ThreadLocal)dispatch 表**无任何 `/record` 路由**。纯逻辑 helper 走 `object RunnerWire`(main/kotlin)+ 对应 `src/test/kotlin` JVM 单测(`RunnerWireTransformTest`/`PopupWireTest`/… 10 个,junit 4.13.2 + org.json,`build.gradle` `testImplementation`)—— **纯映射器就落这套,与既有 pure-helper 模式同构,零设备可测**。
- **§9#8 三层归位(不变量,决定「映射落哪」)**:采集(注册 listener、收事件)= **sense**,落 runner core 平铺面;「`TYPE_VIEW_TEXT_CHANGED` 且结果空 = Clear、非空 = Fill;`viewIdResourceName`→`Selector::Id`;连续同源 TEXT_CHANGED 合成一个 Fill」这类分类**是 Android-runtime-specific 决策知识 → bake 进 android-runner**(§9#8 允许把 runtime-specific 决策 bake 进 driver/runner)。产物 `IRAction` 是平台中立 seam。故映射器落 runner(Kotlin),host 只反序列化 + 喂 generator(中立 steel)。

## 本段预先定死的口径(防 scope 漂移与自欺)

- **架构决策(C2 落一条 §10 决策日志):Android 的 `/record/*` 直接吐 `IRAction` JSON,不吐 `RecordedEvent`。** 依据(上「关键翻盘」):① generator 的输入契约就是 `IRAction[]`;② iOS 的 `RecordedEvent` wire 压根不喂 generator(只喂 capsule-reconcile),不存在「共享 RecordedEvent→generator」先例可对齐;③ `RecordedEvent.raw_code` 判别式是 iOS kAX int 语义,拿它塞 Android `AccessibilityEvent` 类型 int 会**语义撞车**且仍要新建一座 iOS 都没有的 host 桥。⇒ 从 runner 直接吐中立 `IRAction` 是最干净的 seam(§13 架构 clean >> 成本)。**两平台 `/record/*` body 形因此不同**(iOS=RecordedEvent 喂 reconcile;Android=IRAction 喂 generator)—— 这不是缺陷,是两条采集机制的不同用途;跨平台「同操作录出等价 IRAction」的收敛是 **cold plan C4(parity 闭合)** 的事,C2 只做 Android 这条腿。
- **只做 Tap/Fill/Clear**(C1 最小可移植集)。Swipe/PressKey/GoBack/HideKeyboard 在 Android 采集面缺一等事件 = **gap,单列注释,不采、不臆造**。不碰 web(C3)。不改 generator、不改 IR、不改 iOS 侧。
- **纯映射先红后绿(§4)**:`AccessibilityEvent` 是 Android framework 类,JVM 单测难直构 → **拆两层**:①`data class CapturedAxEvent(type, viewId, text, beforeText, eventTimeMs)`(POCO,listener 从真 `AccessibilityEvent` 填);②纯函数 `RecordMapper.map(events: List<CapturedAxEvent>): List<String /*IRAction JSON*/>`(JVM 可测)。红 = fixture `CapturedAxEvent[]` → 期望 IRAction JSON(先失败一次)。这与 iOS「`RecordedEvent` POCO 在 Core、swizzle 在 UITest target」的 sense/POCO 分离同构。
- **跨语言契约锁(防 Kotlin JSON 与 Rust IRAction 漂移)**:Kotlin 单测断言的 IRAction JSON 字符串,**同一批**在 Rust 侧(`smix-recorder` 或 `smix-authoring-ir` 的 test)`serde_json::from_str::<Vec<IRAction>>` 反序列化成功 + 落对变体 + 喂 generator 非空。两端咬同一 fixture(route-conformance 同款纪律)。
- **e2e 机器可判(§5)**:S3 gate = 录固定脚本(经 runner 既有 act 路由 `/tap-by-id`+`/input-text` 驱动确定性动作)→ `/record/stop` 拿 `{events:[IRAction...]}` → jq 断言序列(Tap{id=X} → Fill{id=Y,text=Z} → Clear{id=Y})→ 喂 generator 断言 maestro 含 `tapOn/inputText/eraseText`、rust 含 `app.tap/app.fill/app.clear` → 回放 `smix run --platform android` exit 0。全命令 + 退出码判定,无人工读图。

## 步骤(线性,3 个)

### S1. 纯映射器 `CapturedAxEvent → IRAction JSON`(Kotlin,JVM 单测;+ Rust 侧契约锁)

**红(写测试)**
- 文件:`android-runner/app/src/test/kotlin/dev/smix/runner/RecordMapperTest.kt`(JVM 单测,无设备)
- 断言(fixture `CapturedAxEvent[]` → 期望 IRAction JSON):
  - `TYPE_VIEW_CLICKED`(viewId=`com.x:id/login_btn`)→ 一条 `{"kind":"tap","selector":{"id":"login_btn"},"timestampMs":<eventTimeMs>}`(剥 `:id/`)。
  - `TYPE_VIEW_TEXT_CHANGED`(viewId=`…:id/email`,text=`"a@b.co"`,beforeText=`""`)→ `{"kind":"fill","selector":{"id":"email"},"text":"a@b.co","timestampMs":N}`。
  - **合成**:连续 3 条同源 `TYPE_VIEW_TEXT_CHANGED`(`"h"`,`"he"`,`"hel"`,同 viewId)→ **单条** `fill` `text:"hel"`(keystroke 去抖:同源相邻 TEXT_CHANGED 折叠成最终 text 一条)。
  - `TYPE_VIEW_TEXT_CHANGED`(text=`""`,beforeText=`"hel"`,同源)→ `{"kind":"clear",…}`(结果空且 before 非空 = Clear)。
  - **gap 不臆造**:`TYPE_VIEW_CLICKED` 但 `viewId==null`(无 a11y id)→ 该事件**丢弃 + 计一个 unmapped 计数**(不伪造 selector);`TYPE_VIEW_SCROLLED`/其它类型 → 丢弃(不产 IRAction)。
- 文件:`crates/smix-recorder/tests/android_iraction_contract.rs`(或 `smix-authoring-ir/tests/`)—— 把上面 Kotlin fixture 断言的**同一批 JSON 字符串**常量化,`serde_json::from_str::<Vec<IRAction>>` 成功 + 变体正确 + `generate_maestro_yaml`/`generate_rust` 输出非空。
- 跑红(须先失败一次):
  ```bash
  export ANDROID_SERIAL=emulator-5554
  ( cd android-runner && ./gradlew :app:testDebugUnitTest --tests '*RecordMapperTest*' )   # 无 RecordMapper → 编译/断言红
  cargo test -p smix-recorder android_iraction_contract                              # 无常量/文件 → 红
  ```

**绿(实现)**
- 文件:`android-runner/app/src/main/kotlin/dev/smix/runner/RecordMapper.kt`
- API:`data class CapturedAxEvent(val type: Int, val viewId: String?, val text: String?, val beforeText: String?, val eventTimeMs: Long)` + `object RecordMapper { fun map(events: List<CapturedAxEvent>): MapResult }`(`MapResult(actions: List<String /*IRAction JSON*/>, unmapped: Int)`)。
- 关键点:①`TYPE_VIEW_CLICKED`→tap;②TEXT_CHANGED:text 非空→fill(同源相邻折叠取最后 text)、text 空且 before 非空→clear;③`viewId` 剥 `:id/` → `{"id":short}`,null → 丢弃 + `unmapped++`;④timestamp 取 `eventTimeMs`。JSON 用 `org.json.JSONObject`(既有 testImpl/androidTestImpl 依赖),key 顺序无关(serde 解析)。
- Rust 侧常量文件 + test 落地。
- 跑绿:上两条命令转绿。

**重构(可选)**
- 折叠逻辑若散,收进一个 `coalesceTextChanges` 私有函数;不改行为。

### S2. `/record/*` wire 装配 + listener 采集接线(android-runner)

**红(写测试)**
- 文件:`android-runner/app/src/test/kotlin/dev/smix/runner/RecordBufferTest.kt`(JVM 单测,无设备 —— 测缓冲/生命周期纯逻辑,不测 UiAutomation)
- 断言(对一个 `object`/`class RecordBuffer`):`start()` 清空 + 置 active;喂若干 `CapturedAxEvent`(经 `RecordMapper`)→ `poll()` 返回累积 IRAction JSON **且清空**(流式读不丢);`stop()` 返回剩余 + 置 inactive;inactive 时喂事件被丢弃。线程安全(并发 append/drain 不炸)。
- 文件:`crates/smix-runner-wire/` 既有 route-conformance / android route 清单测试里追加 `/record/start`、`/record/poll`、`/record/stop` 三条 Android 路由存在性断言(与 iOS `//! - POST /record/start` 文档面 parity;body 形 = `{events:[IRAction JSON]}`)。
- 跑红:
  ```bash
  export ANDROID_SERIAL=emulator-5554
  ( cd android-runner && ./gradlew :app:testDebugUnitTest --tests '*RecordBufferTest*' )   # 红
  cargo test -p smix-runner-wire record                                             # Android record 路由未登记 → 红(若有 conformance 表)
  ```

**绿(实现)**
- 文件:`android-runner/app/src/main/kotlin/dev/smix/runner/RecordBuffer.kt` —— `start/poll/stop/append`,`synchronized` 保护 `MutableList<String>` + `active: Boolean`。
- 文件:`android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt`(sense 采集接线,device-bound → 落 androidTest 宿主):
  - 在 `runServerForever()` 里、设完 `serviceInfo` 后,`inst.uiAutomation.setOnAccessibilityEventListener { ev -> RecordBuffer.append(RecordMapper.mapOne(CapturedAxEvent.from(ev))) }`(仅 active 时入缓冲)。`CapturedAxEvent.from(AccessibilityEvent)` 抽 `eventType`/`source?.viewIdResourceName`/`text`/`beforeText`/`eventTime`。
  - `SmixHttpServer.serve()` dispatch 表加三路由:`POST /record/start`→`RecordBuffer.start()` 返 `{"ok":true}`;`GET /record/poll`→`{"events":[…]}`(drain);`POST /record/stop`→`RecordBuffer.stop()` 返 `{"events":[…]}`。body 经既有顶部 drain(`{}` 合法)。
- **接线风险(S3 设备实测处证伪,implement-discover loop,非 planning 期能定 —— 如实标)**:
  1. `setOnAccessibilityEventListener` 是单槽,可能与 UiAutomator `UiDevice.waitForIdle` 依赖的内部事件流互斥/抢占 → 设完 listener 后既有 `/tree`/`/tap-by-id` 的 `waitForIdle` 是否仍工作,S3 验。
  2. `serviceInfo.eventTypes`/`flags` 是否需追加(如 `flags |= FLAG_INCLUDE_NOT_IMPORTANT_VIEWS` 或放开 eventTypes)才能收到 `CLICKED`+`TEXT_CHANGED` 投递,S3 验。
  3. 经 a11y `performAction(ACTION_CLICK)`(`/tap-by-id` a11y 路径)发出的点击是否 fire `TYPE_VIEW_CLICKED`(预期 fire;若不 fire 则 S3 改用 touch 路径 `device.click` 驱动 fixture)。
  - 这三条是 iOS `EventRecorder` 当年 registration dance / SO_REUSEPORT 那类「源读不出、设备才现」的边界,正是 S3 e2e 存在的意义。
- 跑绿:S2 两条红命令转绿(缓冲纯逻辑 + 路由登记);`( cd android-runner && ./gradlew assembleDebugAndroidTest )` 编译进 gate(采集接线编译过)。

**重构(可选)**
- 无。

### S3. emulator 设备 e2e:录一段 → IRAction[] → generator → 回放

**（本步是 C2 定义性产出;唯一上设备的步骤。钉 serial + 让位 batch-owner + 收尾 sweep。）**
- 前置:`export ANDROID_SERIAL=emulator-5554`;`adb devices` 确认;`pgrep -f 'runner.ts|smix run|supervise'` 让位。
- fixture:**用系统 app 里带稳定 `viewIdResourceName` 的可点按钮 + `EditText` 的界面**(候选 `com.android.settings` 搜索框:经典 View、真 resource-id,`viewIdResourceName` 可靠;**不用** Compose testTag——那有 `findNodeByViewId` 已知 gap,但 recording 直接读 `event.source.viewIdResourceName`,系统 Settings 是经典 View 故无此 gap)。**具体 id 实现期由 live `/tree` dump 取真值**(parity C5 Preferences 范式),**不臆造 id**。
- 脚本(经 runner 既有 act 路由驱动确定性动作,recorder 被动观察):
  1. `smix runner up emulator-5554 --platform android`(钉 serial;`am instrument` 阻塞 server,起法同 v2.8-C6)。
  2. `curl -XPOST localhost:$PORT/record/start`。
  3. `POST /tap-by-id {id: <button>}` → 期望录到 Tap;`POST /tap-by-id {id: <editfield>}` + `POST /input-text {text:"smix"}` → 期望录到 Fill;再 `input-text` 清空或 `/press-key` 删 → 期望 Clear(实现期按 fixture 定确定性清空手段)。
  4. `curl -XPOST localhost:$PORT/record/stop` → 拿 `{events:[IRAction...]}`。
  5. host:`Vec<IRAction>` = `serde_json::from` events → `generate_maestro_yaml` + `generate_rust` 写文件。
  6. 回放:`smix run --platform android <生成的 maestro yaml>` exit 0。
- 收尾:`scripts/dev/simx-sweep.sh`(不 shutdown all);runner down。

**e2e 断言(机器可判,写进 gate 脚本)**:events JSON 经 jq 断言含 `kind=="tap"`+`selector.id==<button>`、`kind=="fill"`+`text=="smix"`、`kind=="clear"`;maestro 输出含 `tapOn`/`inputText`/`eraseText`;rust 含 `app.tap`/`app.fill`/`app.clear`;回放 exit 0。

## Checkpoint C2 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
export ANDROID_SERIAL=emulator-5554

# —— Gate A:纯映射 + 契约锁 + wire 装配(device-free,硬门)——
( cd android-runner && ./gradlew :app:testDebugUnitTest \
    --tests '*RecordMapperTest*' --tests '*RecordBufferTest*' ) \
  && cargo test -p smix-recorder android_iraction_contract \
  && ( cd android-runner && ./gradlew assembleDebugAndroidTest ) \
  && echo GATE-A-PASS

# —— Gate B:emulator 设备 e2e(录→IRAction→generator→回放)——
#   由 S3 的 e2e 脚本执行,末尾断言:
bash scripts/dev/v2.10-c2-record-e2e.sh emulator-5554   # 内部做上述 6 步 + jq/退出码断言
# 期望 stdout 末行:C2-E2E-PASS(events 序列 tap/fill/clear 断言过 + maestro/rust 非空含期望动词 + 回放 exit 0)
```

期望:`GATE-A-PASS` 打印且各命令 exit 0;`v2.10-c2-record-e2e.sh` 末行 `C2-E2E-PASS` 且 exit 0。含义 = ①`CapturedAxEvent→IRAction` 纯映射(含合成/gap)绿 + Kotlin↔Rust 契约锁咬合 + `/record/*` 缓冲纯逻辑绿 + 采集接线编译过(Gate A);②真 emulator 上被动录到 tap/fill/clear → IRAction[] 断言得住 → generator 出非空 maestro/rust → 回放绿(Gate B)。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.10-c2-hot.md`。
2. **核心架构决策(Android /record 直吐 IRAction 非 RecordedEvent)已在热化时写入 `docs/v2.md` 决策日志**(`[v2.10-C2 热化期架构决策…]` 一条),无需重复;C2 收尾若 S3 设备实测牵出与该决策相关的偏差/障碍(接线风险三条),另加一条 finding 记实测结果,不改原决策行(诚实留档)。
3. 调 sub-agent 热化 **C3(web Playwright bridge)**,见 CLAUDE.md §6。若 C2 的 S3 接线风险(listener 与 waitForIdle 互斥 / eventTypes 需放开 / a11y-action 不 fire CLICKED)在设备上翻出结构性障碍,如实记 finding + 由用户/上层拍板是否 re-tier,不隐瞒、不硬凑。

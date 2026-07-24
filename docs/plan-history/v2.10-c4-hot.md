# plan-hot — v2.10 到 C4:三平台 parity 闭合 + record→generate glue 统一(v2.10 收官)

## 目标 checkpoint

C4:**收 C2/C3 re-tier 的 deferred glue,建成平台无关的 `record → IRAction[] → generator → 文件` host 面,并机械证明三平台 parity。**通过后世界变成:

1. **glue 落地**:`smix authoring generate --input <events.json>`(device-free 核心,消费 `Vec<IRAction>` JSON → `smix-recorder` generator → 写 maestro/rust)+ `smix authoring tap-record --platform android`(live 采集:runner `/record/*` → `Vec<IRAction>` → 复用 generate 核心 → 写)。web 腿经 Node 把 `recordWebSession` 的 IRAction JSON 落文件、shell 调 `smix authoring generate` 进 Rust generator(**不在 TS 侧重造 generator**,单一 stone)。
2. **parity 机械闭合**:iOS(`RecordingApp` 直记 IRAction)/ Android(`RecordMapper`)/ web(`mapDomEvents`)三腿,对同一逻辑操作(tap 同 id / fill 同 id / clear 同 id)吐出的 IRAction JSON,经**同一 generator 出字节相同**的 maestro/rust(共享 id canonical fixture)—— device-free 硬 gate。
3. **live 兑现**:Android emulator 真录 tap/fill/clear → tap-record → maestro → `smix run --platform android` 回放绿;web headless 真录 → generate 非空。
4. **Clear 诚实定档**:web `fill('')` 可靠产 clear;Android 逐字符 DEL 尝试(C2 finding① 的 close-btn/`input ""` 不产干净 TEXT_CHANGED-to-empty),拿不到诚实标 gap。

**边界(ceiling-first,不硬塞)**:**iOS live 被动 AX 录制 → generator 的 glue 不在 C4** —— iOS `/record/*` 吐 `RecordedEvent`(raw_code=AX kAX int,唯一消费者 `capsule.rs::reconcile`),**全 workspace 无 `RecordedEvent→IRAction` 桥**,从 AX focus-change 反推 tap/fill/clear 意图是研究级问题(见 §决策日志 iOS re-tier 条)。iOS 在 parity 中经 `RecordingApp`(SDK 侧直记 IRAction、早已喂 generator)表达;iOS live 被动录制→generate 单列 v2.11+ follow-on。这不缩水 C4 的 parity/glue 达标(Android+web live 闭合 + 三腿 IRAction-level parity)。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— 平台无关地基(generator + 三腿映射产物 + 契约锁)——
grep -q 'pub fn generate_maestro_yaml' crates/smix-recorder/src/generator_maestro_yaml.rs   # generator steel(消费 &[IRAction])
grep -q 'pub fn generate_rust' crates/smix-recorder/src/generator_rust.rs
grep -q 'object RecordMapper' android-runner/app/src/main/kotlin/dev/smix/runner/RecordMapper.kt  # Android 腿(C2)
grep -q 'export function mapDomEvents' npm/smix-web-record/src/mapDomEvents.ts               # web 腿(C3)
grep -q 'pub fn record' crates/smix-recorder/src/session.rs                                  # iOS 腿 = RecordingApp(直记 IRAction)
test -f crates/smix-recorder/tests/android_iraction_contract.rs                              # Android 契约锁(C2)
test -f crates/smix-recorder/tests/web_iraction_contract.rs                                  # web 契约锁(C3)
# —— live 采集 wire(Android 直吐 IRAction JSON)——
grep -q '/record/stop' android-runner/app/src/androidTest/kotlin/dev/smix/runner/RunnerTest.kt   # Android /record 装配(C2)
grep -q 'pub async fn start_record' crates/smix-runner-client/src/lib.rs                     # runner-client 有 record 面(iOS 形,C4 补 IRAction 变体)
# —— device 纪律前置(S3)——
adb devices | grep -q 'emulator-5554'                                                        # 目标 emulator 在
node --version                                                                               # web 腿 Node 宿主
```

全部 exit 0 = 可开工。任一失败 → 按 §6「何时该拒绝热化」回报,不硬开。

**Android 设备纪律(S3 强制,写死不可省)**:
- **所有 adb/gradle 设备动作必须钉 `export ANDROID_SERIAL=emulator-5554`**(或 `adb -s emulator-5554`)。本机连着物理机 `R5CT52DF07D`(SM_S9010),未钉 serial 的 mutation 装到**所有设备含物理机**(memory `android_gradle_installs_to_all_devices`);`scripts/dev/adb-guard.sh` hook 拦未钉 serial 的 mutation,别绕。
- 设备操作前 `adb devices` + 让位 batch-owner(`pgrep -f 'runner.ts|smix run|supervise'`,memory `runner_ops_check_batch_owner_first`)。
- S1/S2 全 device-free(Rust 单测 + JVM 单测 + vitest + 契约/parity gate,零设备/零浏览器);只 S3 上 emulator + headless chromium。S3 收尾 `scripts/dev/simx-sweep.sh`(不 shutdown all)+ runner down。

## 已经查清、不必重查的事实

- **generator 契约(steel,已在,C4 不动)**:`generate_maestro_yaml(actions: &[IRAction], app_id: &str) -> Result<String, RecorderError>`(tap→`tapOn`;fill→`tapOn`+`inputText`;clear→`eraseText: 100`);`generate_rust(actions: &[IRAction], test_fn_name: &str, bundle_id: &str) -> Result<String, RecorderError>`(tap→`app.tap`;fill→`app.fill`;clear→`app.clear`;`async fn <test_fn_name>` 签名)。两者 `actions.is_empty()` → `Err(EmptySession)`。**C4 只喂它 `Vec<IRAction>`,不改 generator/IR。**
- **三腿的 IRAction 产物形(parity 对象)**:
  - **Android**(`RecordMapper.kt` / `android_iraction_contract.rs`):`{"kind":"tap","selector":{"id":"login_btn"},"timestampMs":N}` —— `viewIdResourceName` 剥 `:id/` → `{"id":short}`。
  - **web**(`mapDomEvents.ts` / `web_iraction_contract.rs`):`{"kind":"tap","selector":{"id":"login-btn"},...}` —— `data-testid` → `{"id":...}`(主路径);role→`{"role":...}`(PARTIAL);text→`{"text":...}`。
  - **iOS**(`session.rs` `RecordingApp`):`record(IRAction::Tap { selector: selector.clone(), timestamp_ms })` —— **直构 `IRAction`**,`selector` 是 SDK 侧 `Selector`(通常 `Selector::Id`)。序列化即 `{"kind":"tap","selector":{"id":...},"timestampMs":N}`(serde `tag="kind", rename_all="camelCase"`;Selector `untagged` → `{"id":X}`)。
  - ⇒ **三腿在 `Selector::Id` 上收敛到同一 JSON 形**(native id / data-testid → `{"id":X}`)。parity 的机械底座 = **共享 id canonical fixture → generator 字节相同**。
- **glue 现状(两平台皆缺,C2/C3 re-tier 定归 C4)**:`crates/smix-cli/src/authoring.rs` 无任何消费 `/record/*` 事件 / 调 generator 的路径;`AuthoringAction::Record`(main.rs:667)是**周期树快照 assertVisible 脚手架**(`cmd_record_session`),不走事件流;`tap-record` 只 doc 注释(authoring.rs:12)无实现。generator 目前唯一喂食者 = `smix-recorder::RecordingApp`(SDK 侧包 `app.tap/fill/clear`)。**「/record → generator」这条 CLI 活链此前不存在。**
- **runner-client record 面 = iOS 形(缺 IRAction 变体)**:`crates/smix-runner-client/src/lib.rs:1872-1900` 已有 `start_record() -> Result<()>`、`poll_record() -> Vec<RecordedEvent>`、`stop_record() -> Vec<RecordedEvent>`。**返回 `RecordedEvent`(iOS 形,`raw_code`/`timestamp_ms`/`extra` flatten)**;拿它读 Android 的 `{events:[IRAction JSON]}` 会把 `kind`/`selector` 吞进 `extra`、丢结构。⇒ **C4 补 `stop_record_actions() -> Vec<IRAction>`(+ `poll_record_actions`)** 反序列化同一 `{events:[...]}` 到 `Vec<IRAction>`(Android body),iOS 形不动(向后兼容,别改现有签名)。
- **`smix run` 回放入口(Android 腿唯一)**:`main.rs:389 Run { flows: Vec<PathBuf>, --device, --platform <ios|android>, --runner-port, ... }`。generator 出的 maestro yaml 直接作 `flows[0]`,`smix run --platform android <yaml>` 回放。**web 无回放**:generator 出的是 native-shape maestro/rust,无 web replay runtime;web 闭合 = record→IRAction→generate 非空(诚实边界,非缺陷)。
- **web IRAction 从哪出**:`recordWebSession(url, drive) -> Promise<string[]>`(`recordWeb.ts:59`)返 `mapDomEvents(events).actions` = IRAction JSON **字符串数组**(TS/Node 内)。进 Rust generator 的正统桥 = **Node 把该数组写 `events.json`(即 `Vec<IRAction>` JSON)→ shell `smix authoring generate --input events.json`** —— 跨语言边界靠 IRAction JSON 文件 + CLI,**不在 TS 复制 generator**(generator 是 Rust stone,单一来源)。
- **Clear 现状(C2 finding①)**:Android Settings close-btn(ImageView,`ACTION_CLICK` 不 fire CLICKED)与 `input-text ""` **都不产干净 `TYPE_VIEW_TEXT_CHANGED`-to-empty** → C2 未设备录到 Clear(mapping 逻辑已单测锁:text=""&before 非空→Clear)。web 侧 clear e2e C3 未测。**C4 S3 评估可靠 clear**:web `getByTestId('q').fill('')` 触发 `input` value=""(mapDomEvents 已处理 value=''&before 非空→clear),预期可靠;Android 候选 = 聚焦 EditText 后逐字符 `pressKey` DEL(每删一字 fire `TEXT_CHANGED`,末条 value="" before 非空,coalesce 后→Clear),S3 emulator 验;不产干净事件则诚实标 gap 记 follow-on,不臆造。
- **§9#8 三层归位(决定 glue 落哪)**:采集(listener/注入捕获)= sense,已落 runner core / web bridge(C2/C3);glue(fetch IRAction[] → 调 generator → 写文件)= **authoring lane 的 steel**,落 CLI(`authoring.rs`)+ runner-client 反序列化,**不埋进任何 driver/sense/act core**;generator 是平台中立 stone/steel。parity gate(纯 IRAction→generator 对账)= steel 单测。
- **contract-lock 范式(已在)**:`android_iraction_contract.rs`/`web_iraction_contract.rs` 各常量化本腿 IRAction JSON,`serde_json::from_str::<IRAction>` + generator 非空。**parity gate 照此建**,咬三腿**共享 id** 的同批 JSON,断言 generator 字节相同。

## 本段预先定死的口径(防 scope 漂移与自欺)

- **glue 形(两条命令一个核心)**:① **device-free 核心** `smix authoring generate --input <events.json> --format <maestro|rust> -o <out> [--app-id X] [--test-fn-name Y]` —— 读 `Vec<IRAction>` JSON → generator → 写。平台无关;web 腿即用此(Node 写 IRAction JSON + shell 调)。② **live 采集** `smix authoring tap-record --device <> --platform android --format <> -o <> [--duration-secs N] [--app-id X]` —— `start_record` → sleep → `stop_record_actions()` 拿 `Vec<IRAction>` → **复用 ① 的 generate 核心** → 写。两命令共享 generate 核心函数,不各写一遍。
- **只做 Tap/Fill/Clear**(C1 最小可移植集)。Swipe/PressKey/GoBack/HideKeyboard 三腿采集面缺一等事件 = gap(iOS `RecordingApp` 虽能记 Swipe 等,但 Android/web 采不到 → parity 不覆盖,记 gap)。不改 generator/IR/iOS 侧被动 wire。
- **iOS live 被动录制→generate = 不做(re-tier,§决策日志)**。`tap-record --platform ios` 走 `stop_record()`(RecordedEvent)喂不了 generator;C4 的 `tap-record` **只 `--platform android`** 打通。iOS 在 parity 经 `RecordingApp` 的 IRAction canonical 表达。别硬造 `RecordedEvent→IRAction` 桥(研究级,归 v2.11+)。
- **parity 机械 gate(device-free,防「等价」含糊)**:`crates/smix-recorder/tests/cross_platform_parity.rs` —— 三腿对 canonical op 集 `[tap {id:"field"}, fill {id:"field", text:"smix"}, clear {id:"field"}]` 各产 IRAction JSON(iOS RecordingApp 形 / Android RecordMapper 形 / web mapDomEvents 形,**id 统一取 `field`**:Android from `com.x:id/field`、web from `data-testid="field"`、iOS from `Selector::id("field")`)→ `serde_json::from_str::<Vec<IRAction>>` 三份 → `generate_maestro_yaml(_, "app")` 三份 **`assert_eq!` 字节相同** + `generate_rust(_, "recorded", "app")` 三份 `assert_eq!` 相同。**字节相同 = IR 真收敛**(非「看起来等价」)。selector kind 差异(role/text 路径)不进 parity canonical(只锁 id 主路径的收敛),role/text 路径的 per-leg 差异记 gap 注释。
- **e2e 机器可判(§5)**:S3 Android gate = 录固定脚本 → tap-record 出 maestro → jq 断言 IRAction 序列(tap{id} → fill{id,text} → clear{id 或标 gap})+ maestro 含 `tapOn/inputText/eraseText` + `smix run --platform android` exit 0;web gate = recordWebSession → IRAction JSON → `smix authoring generate` → maestro 非空含期望动词。全命令 + 退出码,无人工读图。
- **别再造虚构 wire**(C5 `/select/resolve` 教训):`tap-record` 调的 `/record/start`+`/record/stop` 是 **C2 设备验证过的真路由**(`RunnerTest.kt:168-170` dispatch + emulator e2e 过);runner-client 新增的 `stop_record_actions` 只是换 body 反序列化目标(IRAction vs RecordedEvent),route 不变、不新造。web 侧 `smix authoring generate` 是纯本地文件→generator,零 wire。

## 步骤(线性,3 个)

### S1. `smix authoring generate` device-free glue 核心(IRAction[] JSON → generator → 文件)

**红(写测试)**
- 文件:`crates/smix-cli/src/authoring.rs`(`#[cfg(test)]` 模块内新增,或 `crates/smix-cli/tests/authoring_generate.rs` 集成测试)
- 断言:给定 fixture `Vec<IRAction>` JSON(tap{id} + fill{id,text} + clear{id})→ `generate_actions_json(json_bytes, Format::Maestro, "com.x")` 返 String 含 `tapOn`/`inputText`/`eraseText`;`Format::Rust`("recorded","com.x")含 `async fn recorded`/`app.tap`/`app.fill`/`app.clear`;空 JSON `[]` → `Err`(透传 generator `EmptySession`,不吞)。写文件路径 → 文件存在且非空。
- 跑红(须先失败一次):
  ```bash
  cargo test -p smix-cli authoring_generate    # 无 generate 核心 → 红
  ```

**绿(实现)**
- 文件:`crates/smix-cli/src/authoring.rs`
- API:
  - `fn generate_actions_json(input: &[u8], format: GenFormat, app_id: &str, test_fn_name: &str) -> Result<String, CliError>`(纯:反序列化 `Vec<IRAction>` → `generate_maestro_yaml` / `generate_rust`;`enum GenFormat { Maestro, Rust }`)。
  - `pub async fn cmd_generate(input: PathBuf, format: GenFormat, output: PathBuf, app_id: String, test_fn_name: String) -> Result<ExitCode, CliError>`(读文件 → `generate_actions_json` → `std::fs::write` → 打印路径)。
- 文件:`crates/smix-cli/src/main.rs`
- API:`AuthoringAction::Generate { input: PathBuf, #[arg(long, value_enum, default_value_t=…)] format, #[arg(long, short)] output: PathBuf, #[arg(long, default_value="com.example")] app_id: String, #[arg(long, default_value="recorded")] test_fn_name: String }` + dispatch 到 `authoring::cmd_generate`。
- 关键点:①`serde_json::from_slice::<Vec<IRAction>>` 失败自然抛(`CliError`,不吞);②generator `EmptySession` 透传;③`smix-recorder` 加进 `smix-cli` Cargo dep(若未在)。
- 跑绿:上红命令转绿。

**重构(可选)**
- 无。

### S2. runner-client IRAction 变体 + `tap-record` live wiring + cross-platform parity gate(device-free)

**红(写测试)**
- 文件:`crates/smix-runner-client/src/lib.rs`(`#[cfg(test)]`)—— `stop_record_actions` 反序列化断言:mock `{"events":[{"kind":"tap","selector":{"id":"field"},"timestampMs":1}, ...]}` → `Vec<IRAction>` 落对变体(tap/fill/clear)。第一次红(无方法)。
- 文件:`crates/smix-recorder/tests/cross_platform_parity.rs` —— 三腿 canonical IRAction JSON(共享 id `field`,见口径)→ `generate_maestro_yaml` 三份 `assert_eq!` 字节相同 + `generate_rust` 三份 `assert_eq!` 相同 + 三份 kind 序列 == `[tap,fill,clear]`。第一次红(无文件)。
- 跑红:
  ```bash
  cargo test -p smix-runner-client stop_record_actions    # 无方法 → 红
  cargo test -p smix-recorder cross_platform_parity        # 无文件 → 红
  ```

**绿(实现)**
- 文件:`crates/smix-runner-client/src/lib.rs`
- API(新增,不改现有 `stop_record`/`poll_record` 的 iOS 形):
  - `pub async fn poll_record_actions(&self) -> Result<Vec<IRAction>, RunnerTransportError>`
  - `pub async fn stop_record_actions(&self) -> Result<Vec<IRAction>, RunnerTransportError>`
  - 两者反序列化同一 `{events:[...]}` 到 `Vec<IRAction>`(Android body);`smix-authoring-ir` 加进 runner-client Cargo dep(若未在)。
- 文件:`crates/smix-cli/src/authoring.rs`
- API:`pub async fn cmd_tap_record(port: u16, platform: RunPlatform, format: GenFormat, output: PathBuf, app_id: String, test_fn_name: String, duration_secs: u64) -> Result<ExitCode, CliError>` —— `client.start_record()` → `tokio::time::sleep` → `client.stop_record_actions()` → `serde_json::to_vec(&actions)` → **复用 `generate_actions_json`** → 写。`platform` 非 android → `CliError`(iOS live re-tier,明确报「iOS 被动录制→generate 未支持,见 v2.11 follow-on」,不静默)。
- 文件:`crates/smix-cli/src/main.rs`
- API:`AuthoringAction::TapRecord { #[arg(long, value_enum)] platform, #[arg(long, value_enum)] format, #[arg(long, short)] output, #[arg(long)] app_id, #[arg(long)] test_fn_name, #[arg(long, default_value_t=8)] duration_secs, #[arg(long, env="SMIX_RUNNER_PORT")] port, #[arg(long)] device }` + dispatch。
- 文件:`crates/smix-recorder/tests/cross_platform_parity.rs` 落地(共享 id canonical + `assert_eq!` 字节相同)。
- 关键点:parity gate 里 iOS 腿 canonical 可直接构 `IRAction::Tap{selector: Selector::id("field"), ..}` 序列化取 JSON(证 RecordingApp 形),或常量化其 JSON —— 与 Android/web 常量咬同一 id。
- 跑绿:上两条红命令转绿。

**重构(可选)**
- `cmd_generate` 与 `cmd_tap_record` 的 write 尾部若重复,收进 `write_generated(...)` 私有函数;不改行为。

### S3. 设备/浏览器 e2e:Android live 录→generate→回放 + web 录→generate + Clear 评估

**(本步是 C4 定义性产出;唯一上设备/浏览器的步骤。Android 钉 serial + 让位 batch-owner + 收尾 sweep;web headless chromium=§9#1 driver-层非真机。规划期不跑,此步执行期跑。)**

**Android emulator 腿**(`scripts/dev/v2.10-c4-android-record-e2e.sh`)
- 前置:`export ANDROID_SERIAL=emulator-5554`;`adb devices` 确认;`pgrep -f 'runner.ts|smix run|supervise'` 让位。
- fixture:系统 Settings 搜索界面(经典 View,`viewIdResourceName` 可靠;**具体 id 由 live `/tree` dump 取真值,不臆造**,同 C2/C5 范式)。
- 脚本:
  1. `smix runner up emulator-5554 --platform android`(钉 serial;`am instrument` 阻塞,起法同 C2)。
  2. `POST /record/start`;经 runner act 路由(`/tap-by-id`+`/input-text`)驱动确定性 tap/fill;**Clear 尝试 = 聚焦 EditText 后逐字符 `/press-key DEL`**(每字 fire TEXT_CHANGED,末条 value="" → coalesce→Clear;C2 finding① 的可靠 clear 评估)。
  3. `smix authoring tap-record --device emulator-5554 --platform android --format maestro -o /tmp/c4-android.yaml`(内部 stop_record_actions → generate)。
  4. jq 断言 events 序列含 `tap{id}` → `fill{id,text="smix"}` → `clear{id}`(**Clear 未产干净事件则脚本诚实打印 `CLEAR-GAP` + 该断言降级为记录,不伪装**);maestro 含 `tapOn/inputText/eraseText`。
  5. 回放:`smix run --platform android /tmp/c4-android.yaml` exit 0。
  6. 收尾:`scripts/dev/simx-sweep.sh`;runner down。
- 末行 `C4-ANDROID-E2E-PASS`(Clear 若 gap,打 `CLEAR-GAP-ANDROID` 但整体仍 PASS,gap 记 follow-on)。

**web headless 腿**(`npm/smix-web-record/e2e/record-generate-e2e.ts`,扩展 C3 的 record-e2e)
- 脚本:`recordWebSession(fixture, drive)` 里 drive = `getByTestId('go').click()` → `getByTestId('q').fill('smix')` → `getByTestId('q').fill('')`(**web clear = fill('') 触发 input value=""**)→ 返 IRAction JSON 数组 → 写 `/tmp/c4-web-events.json` → `execFileSync('smix', ['authoring','generate','--input','/tmp/c4-web-events.json','--format','maestro','-o','/tmp/c4-web.yaml','--app-id','example.com'])` → 断言 events 序列 `tap{id=go}`→`fill{id=q,text=smix}`→`clear{id=q}` + `/tmp/c4-web.yaml` 非空含 `tapOn/inputText/eraseText`。
- **无 web 回放**(generator 出 native-shape,无 web replay runtime;诚实边界)。fixture 加可清空 input(C3 `fixture.html` 已有 `q`)。
- 末行 `C4-WEB-E2E-PASS`。

**接线风险(S3 实测处证伪,implement-discover,非 planning 期能定 — 如实标)**:
1. Android 逐字符 DEL 是否 fire 干净 `TYPE_VIEW_TEXT_CHANGED`-to-empty(C2 已证 close-btn/`input ""` 不产)—— 拿不到则 `CLEAR-GAP-ANDROID`,Clear 设备生成留 v2.11 follow-on。
2. `smix authoring tap-record` 的 `duration_secs` 窗口是否够 emulator act 序列完成(sleep vs 事件到达时序)—— S3 调窗口/改 poll 循环。
3. web `getByTestId('q').fill('')` 是否 fire `input` 事件(某些框架 `fill('')` 不 dispatch input)—— 不 fire 则 fixture 加显式 clear 按钮 dispatch input(C3 fixture 可扩)。

## Checkpoint C4 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
export ANDROID_SERIAL=emulator-5554

# —— Gate A:glue 核心 + runner-client 变体 + 三平台 parity + 契约锁(device-free,硬门)——
# 注:cargo test 只吃单个 positional testname,多 filter 必须放 --test <bin> 或 -- 后(libtest OR);
# 名字咬真实测试模块(authoring_generate_tests / stop_record_actions_envelope)不是想象的名字。
cargo test -p smix-cli --bin smix authoring_generate_tests \
  && cargo test -p smix-runner-client stop_record_actions_envelope \
  && cargo test -p smix-recorder --test cross_platform_parity --test android_iraction_contract --test web_iraction_contract \
  && ( cd android-runner && ./gradlew :app:testDebugUnitTest --tests '*RecordMapperTest*' --tests '*RecordBufferTest*' ) \
  && ( cd npm/smix-web-record && bun run test ) \
  && cargo build -p smix-cli \
  && echo GATE-A-PASS

# —— Gate B:Android emulator live 录→generate→回放 ——
bash scripts/dev/v2.10-c4-android-record-e2e.sh emulator-5554
# 期望末行:C4-ANDROID-E2E-PASS(events tap/fill 断言过 + maestro 非空含期望动词 + 回放 exit 0;Clear 过或 CLEAR-GAP-ANDROID 记 follow-on)

# —— Gate C:web headless 录→generate ——
bunx playwright install chromium \
  && ( cd npm/smix-web-record && bun run e2e/record-generate-e2e.ts )
# 期望末行:C4-WEB-E2E-PASS(events tap/fill/clear 断言过 + generate 出 maestro 非空含期望动词)
```

期望:`GATE-A-PASS` 打印且各命令 exit 0;`v2.10-c4-android-record-e2e.sh` 末行 `C4-ANDROID-E2E-PASS`、`record-generate-e2e.ts` 末行 `C4-WEB-E2E-PASS` 且 exit 0。含义 =
① glue 核心(`authoring generate` 消费 IRAction[] → generator → 文件)+ runner-client IRAction 变体 + **三平台 parity 字节相同**(iOS/Android/web IRAction → 同一 generator 出相同 maestro/rust)+ 两契约锁 + 三腿映射单测 全绿(Gate A,device-free 硬门);
② Android emulator 真录 tap/fill(+Clear 尝试)→ tap-record → maestro → 回放绿(Gate B);
③ web headless 真录 tap/fill/clear → generate 出非空 maestro(Gate C)。

**不在 C4 验收内(诚实划界)**:iOS live 被动 AX 录制→generate glue(`RecordedEvent→IRAction` 桥,研究级,→ v2.11+ follow-on);web 回放(无 web replay runtime,generator 出 native-shape);Swipe/PressKey/GoBack/HideKeyboard 采集(Android/web 采集面 gap);Android 逐字符 DEL 若不产干净 clear 事件的设备生成(→ follow-on,mapping 逻辑已单测锁)。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.10-c4-hot.md`。
2. **两条架构决策(glue 形 + iOS live re-tier)已在热化时写入 `docs/v2.md` 决策日志**,无需重复;C4 收尾若 S3 设备/浏览器实测牵出偏差(Clear DEL 不 fire / tap-record 窗口 / web fill('') 不 dispatch),另加 finding 记实测,不改原决策行(诚实留档)。
3. **C4 = v2.10 收官 checkpoint**。通过后 v2.10 跨平台 recorder 阶段闭合(三腿 record→IRAction 各绿 + Android/web live record→generate + 三平台 IRAction-level parity)。调 sub-agent 热化**下一 minor 版本首 checkpoint**(v2.11,LLM-in-loop authoring,守 §9#2 走本机 `claude` CLI),见 CLAUDE.md §6;若发布顺延仍待用户授权,如实回报不自作主张 publish。若 S3 翻出结构性障碍(Clear 三平台皆不可靠 / parity 字节不收敛),如实记 finding + 由用户/上层拍板 re-tier,不隐瞒、不硬凑。

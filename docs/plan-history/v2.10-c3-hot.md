# plan-hot — v2.10 到 C3:web Playwright bridge —— 注入式 DOM 捕获 → IRAction(第三条采集腿)

## 目标 checkpoint

C3:**web 采集腿成立 —— 用 Playwright 注入式 DOM 捕获（`page.addInitScript` 捕获相 `click`/`input` 监听 + `page.exposeBinding` 回调回 Node）被动观察浏览器 DOM 交互,纯映射成 `IRAction`（与 Android `RecordMapper` 同款 `{"kind":"tap|fill|clear","selector":{...},"timestampMs":N}` JSON）;纯映射器 browser-free 单测绿 + Kotlin/TS↔Rust 同款契约锁（`web_iraction_contract.rs` 单元级证 web 吐的 IRAction JSON → `generate_maestro_yaml`/`generate_rust` 非空）+ 一段 headless chromium 真录制 e2e（对本地 file:// fixture 驱动 click/fill/clear → 断言吐出的 `IRAction[]` 序列）。** 通过后,「录一遍」不再只对 iOS/Android 成立,web 成为第三条吐同一 IR 的采集腿。**§9#1 铁律守**:web 轴是 Playwright driver-层 DOM bridge（headless chromium,CI-able）,**不是物理设备/真机**。

**交付边界（对齐 Android C2 达标形,ceiling-first 划界）**:C3 = web 采集腿（pure mapper + 契约锁 + headless e2e 证 IRAction[]）。**不做** `record → generate → replay` 的活 CLI glue —— 该 glue **两平台皆缺**（C2 finding②/re-tier 已定:iOS/Android 的 `/record` wire 都没接进 generator）,是**跨平台**问题,留 **C4** 一次建成。契约锁在单元级证明 IRAction→generator,与 Android C2 同款。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— 平台无关地基(映射目标 + generator + 契约锁范式)——
grep -q 'pub enum IRAction' crates/smix-authoring-ir/src/lib.rs              # IR stone(映射目标)
grep -q 'pub fn generate_maestro_yaml' crates/smix-recorder/src/generator_maestro_yaml.rs  # generator steel(消费 IRAction[])
grep -q 'pub fn generate_rust' crates/smix-recorder/src/generator_rust.rs
test -f crates/smix-recorder/tests/android_iraction_contract.rs             # 契约锁范式(web 腿要对照建 web_iraction_contract)
# —— 采集腿对照(Android RecordMapper = web 腿的映射对照,同款 IRAction JSON 形)——
grep -q 'object RecordMapper' android-runner/app/src/main/kotlin/dev/smix/runner/RecordMapper.kt
test -f docs/research/c1-android-capture.md                                 # 研究范式(c1/c7 = c3 研究文档模板)
# —— web 腿宿主 ——
grep -q '"npm/smix-rn"' package.json                                        # 根 bun workspace 在(新 web 包挂这里)
node --version                                                              # web 腿 Node 宿主
```

全部 exit 0 = 可开工。任一失败 → 按 §6「何时该拒绝热化」回报,不硬开。

## 已经查清、不必重查的事实

- **【关键·直接决定 C3 架构】Playwright 无被动 user-DOM-event API**(WebSearch 确认,S1 须以 Playwright 官方文档二次 verify):
  - `page.on(...)` 只投递 `console` / `request` / `response` / `dialog` / `download` / `pageerror` 等,**不含用户 `click`/`input` DOM 交互**。拿它录不到用户操作流。
  - `playwright codegen` 是**独立工具**,把交互转成 **generated script**(`page.getByRole('button').click()`),**不是稳定可消费的事件流**;去 parse 生成脚本 = 脆(生成器格式非契约、无稳定 API)。
  - ⇒ 被动捕获的**干净结构对等** = **注入式**:`page.addInitScript(script)`(文档:每次页面创建/导航后、页面自身脚本运行前注入)埋一段捕获相 `document.addEventListener('click'|'input', …, /*capture*/true)` 监听脚本 + `page.exposeBinding(name, cb)`(文档:在每帧 window 上装 `name` 函数、**跨导航存活**、调用即回 Node `cb`)把每个 DOM 事件回报 Node。文档规范示例正是 `addEventListener('click', e => window.report(e.target))`。**这是 iOS `EventRecorder` AX-swizzle / Android `setOnAccessibilityEventListener` 的 web 版**（被动观察真实交互,非重放脚本）。
- **IRAction 变体 + 字段(映射目标,`smix-authoring-ir/src/lib.rs:33`,serde `tag="kind", rename_all="camelCase"` 内部标签)** —— 与 Android 腿吐的完全同形:
  - `Tap { selector, timestampMs }` → `{"kind":"tap","selector":{...},"timestampMs":N}`
  - `Fill { selector, text, timestampMs }` → `{"kind":"fill","selector":{...},"text":"...","timestampMs":N}`
  - `Clear { selector, timestampMs }` → `{"kind":"clear","selector":{...},"timestampMs":N}`
- **Selector JSON 形(`smix-selector/src/lib.rs:324` `#[serde(untagged)]`,TS 侧 `npm/smix-rn/src/Selector.ts` 已镜像 kind=id/text/role/…)**:`Id → {"id":X}`、`Text → {"text":X}`、`Role → {"role":X}`(untagged,无 type tag)。
- **smix Role 词表是 iOS-centric(`smix-selector/src/lib.rs:80-118` `parse_role` + `ROLE_NAMES`)**:`button/link/textField/secureTextField/searchField/switch/toggle/checkBox/radio/image/staticText(heading)/tab/tabBar/navigationBar/cell/alert/dialog/slider/progressBar/picker/menu/menuItem/scrollView/segmentedControl/table/collectionView/webView/keyboard`。web ARIA role **映射对照**(去非字母数字 + 小写后比对):
  - **可映射**:`button→Button`、`link→Link`、`checkbox→CheckBox`、`radio→Radio`、`switch→Switch`、`tab→Tab`、`menu→Menu`、`menuitem→MenuItem`、`dialog→Dialog`、`alert→Alert`、`slider→Slider`、`heading→StaticText`、`img/image→Image`、`table→Table`。
  - **不可映射(gap,记录不臆造)**:`textbox`(**≠ `textField`**,normalize 后是 `textbox`∉词表)、`combobox`、`listbox`、`option`、`spinbutton`、`searchbox`(≠ `searchField`)、`grid`、`treeitem` 等。
  - ⇒ **`role → Role` 是 PARTIAL**;这正是 selector 映射须以 `data-testid → Id` 为**主路径**的原因(见口径)。
- **generator 消费点(steel,已在,C3 不动)**:`generate_maestro_yaml(&[IRAction], app_id)`(Tap→`tapOn`;Fill→`tapOn`+`inputText`;Clear→`eraseText:100`);`generate_rust(&[IRAction], name, app_id)`(Tap→`app.tap`;Fill→`app.fill`;Clear→`app.clear`)。契约锁只喂它 IRAction[]。
- **Android `RecordMapper` 折叠/gap 逻辑(web 腿对照,`RecordMapper.kt`)**:先过滤到 {CLICKED, TEXT_CHANGED}(滤掉噪音使同源连续),`TYPE_VIEW_CLICKED`→tap;同源相邻 `TYPE_VIEW_TEXT_CHANGED` **折叠取最后 text 一条**;末值空且 before 非空→clear、否则→fill;`viewId==null`→drop + `unmapped++`(不伪造 selector)。**web 腿逻辑对齐**:同源相邻 `input` 折叠成一条 fill;末值空+before 非空→clear;无稳定标识→drop+unmapped。
- **C2 re-tier 划界(已在 v2.md 决策日志两条)**:record→generate CLI glue 两平台皆缺 → C4。故 C3 与 Android C2 同款达标形 = 采集腿 + 契约锁 + 采集 e2e,**不含** glue。
- **§9#8 三层归位**:注入捕获(埋 addInitScript + 收 exposeBinding 回调)= **sense**,落 web bridge module;「`click`→tap;同源 `input` 折叠→fill、末值空→clear;`data-testid`→`Id`、ARIA role(词表内)→`Role`、可见 text→`Text`」这类分类 = **web-runtime-specific 决策知识 → bake 进 web 腿 mapper**。产物 `IRAction` 是平台中立 seam。
- **契约锁范式(`android_iraction_contract.rs`)**:常量化 mapper 吐的**同一批 JSON 字符串**,`serde_json::from_str::<IRAction>` 成功 + 变体正确 + `generate_maestro_yaml`/`generate_rust` 非空。web 腿建 `web_iraction_contract.rs` 照此。
- **本机现状**:`package.json` 根 workspace 现为 `["crates/smix-node","npm/smix-rn"]`,`npm/` 下仅 `smix-rn`;全仓无 `playwright` 生产依赖(`smix-rn` 仅 description 提及 "Playwright-shape")。`bun.lock` 在(用 bun,不混 npm/pnpm)。`docs/research/` 已有 `c1-android-capture.md`/`c7-zorder-obtainability.md`(研究文档模板)。

## 本段预先定死的口径(防 scope 漂移与自欺)

- **架构决策(C3 已在热化时写入 `docs/v2.md` 决策日志一条 `[v2.10-C3 热化期架构决策…]`,无需重复)**:web /record 腿 = 注入式 DOM 捕获直吐 **IRAction JSON**(非 `RecordedEvent`,对齐 Android);pure mapper 落**新包 `npm/smix-web-record`**;`playwright` 加该包 **devDependency**(pure mapper 零 playwright import,仅 bridge+e2e 用);**不碰** `npm/smix-rn` SDK driving 面、`crates/smix-node` napi。
- **selector 映射优先级(定死,防臆造)**:① `data-testid`(作者指定稳定测试标识;Playwright `getByTestId` 默认属性;= iOS `accessibilityIdentifier` / Android `viewIdResourceName` 的 web 诚实对等)→ `Selector::Id`;② 否则 ARIA `role` **且在 smix Role 词表内** → `Selector::Role`(PARTIAL,gap 如上);③ 否则可见 `textContent`(修剪空白,click 目标如 button/link)→ `Selector::Text`;④ 皆不满足 → **drop + `unmapped++`**(不伪造 selector,同 Android null-viewId)。**DOM `id` 属性不作主路径**(页面结构性、非测试语义;若 S1 研究判定其可作为 testid 缺席时的次选,由 S1 VERDICT 明记,不在此擅自纳入)。
- **只做 Tap/Fill/Clear**(C1 最小可移植集)。Swipe/PressKey/GoBack/HideKeyboard 在 web 采集面同为 gap = **单列注释,不采、不臆造**。不改 generator、不改 IR、不碰 iOS/Android 腿。
- **纯映射 browser-free(§4)**:selector **描述子的 DOM 抽取**(读 `data-testid` / ARIA role / `textContent` / input `value`+`beforeValue`)落 **in-page 脚本**(e2e 真 DOM 验);**描述子 → Selector 优先级 + 折叠/gap** 落 **pure mapper**(TS,vitest fixtures,零浏览器)。清晰切分 = mapper 可 device/browser-free 单测。
- **跨语言契约锁(防 TS JSON 与 Rust IRAction 漂移)**:TS 单测断言的 IRAction JSON 字符串,**同一批**在 Rust `web_iraction_contract.rs` `serde_json::from_str::<IRAction>` 成功 + 落对变体 + 喂 generator 非空。两端咬同一 fixture(与 `android_iraction_contract.rs` 同款纪律)。
- **e2e 机器可判(§5)**:headless chromium 对**本地 file:// 静态 HTML fixture**(带 `data-testid` 的 button + input + 清空手段)驱动确定性 `click`/`fill`/`clear` → bridge 收 IRAction[] → node 断言序列(`tap{id=X}` → `fill{id=Y,text=Z}` → `clear{id=Y}`)。全命令 + 退出码判定,无人工读图,**无真机**。

## 步骤(线性,3 个)

### S1. 研究 + VERDICT:web DOM 捕获可得性 + selector 映射可行性(decomposition-before-attack)

**（本步是研究,非红绿 —— 与 C1/C7 同款「研究先行」例外:产出是 falsification 文档不是代码,其「测试」= rubric 先于证据 + 机器可 grep 的 `VERDICT:` 行。理由:web 无 iOS/Android 的 a11y-id 体系,IRAction 的 `Selector` 在 web 语义是否成立、以何策略稳定映射,是须先证伪再动手的真问题;`.claude/rule/decomposition-discipline.md` `debug/decomposition-before-attack`。）**

- 文件:`docs/research/c3-web-capture.md`(照 `c1-android-capture.md` 结构:Reference（iOS/Android 采什么）→ Falsification rubric（先于证据固定）→ Evidence（逐轴 + 逐 selector）→ VERDICT）。
- **必须以 Playwright 官方文档二次 verify**(agent research 不可全信,实施前核实):
  - WebFetch `https://playwright.dev/docs/api/class-page`(确认 `page.on` 事件枚举**不含** click/input;`addInitScript` 注入时机;`exposeBinding` 跨导航存活)。
  - WebFetch `https://playwright.dev/docs/locators` 或 `class-page` 的 test-id 文档(确认 `getByTestId` 默认属性 = `data-testid`;`getByRole` 用**计算 ARIA role**)。
- **捕获轴枚举(判定「能否被动交付可重建 IRAction 的流」)**:
  - Axis-1 `page.on` 被动 user-DOM-event 流 → **NOT-OBTAINABLE**(只 console/request/dialog,无 click/input)。
  - Axis-2 parse `playwright codegen` generated script → **NOT-OBTAINABLE / fragile**(生成脚本非稳定 API,格式非契约)。
  - Axis-3 注入 `addInitScript`(捕获相 click/input 监听)+ `exposeBinding`(回 Node)→ **OBTAINABLE**(文档载明的稳定 API,跨导航存活,= AX-swizzle 的 web 对等)。
- **逐 selector 可得性(赢家 Axis-3 下)**:`data-testid → Id` OBTAINABLE;`textContent → Text` OBTAINABLE(click 目标);`role → Role` **PARTIAL**(词表内映射,`textbox`/`combobox`/… = gap,枚举列出);`DOM id 属性` = 判定是否作 testid 缺席次选(给 VERDICT 明确取/舍,不留模糊)。
- **VERDICT**:预期 `VERDICT: OBTAINABLE (core set via injected capture; selector mapping data-testid→Id primary, role→Role PARTIAL)` —— 但**以 agent 实查 Playwright 文档为准**;若文档推翻(如发现稳定被动 user-event API,或 testid 抽取不稳),诚实 re-tier / 收窄,记 VERDICT(同 c1 PARTIAL 范式),不硬凑。
- 验收(机器可判):
  ```bash
  test -f docs/research/c3-web-capture.md
  grep -Eq '^VERDICT:' docs/research/c3-web-capture.md
  grep -q 'Falsification' docs/research/c3-web-capture.md   # rubric 段在(先于证据)
  ```

### S2. 纯映射器 `CapturedDomEvent → IRAction JSON`(TS,browser-free vitest;+ Rust 契约锁)

**红(写测试)**
- 文件:`npm/smix-web-record/src/__tests__/RecordMapper.test.ts`(vitest,`environment: 'node'`,无浏览器)。
- 断言(fixture `CapturedDomEvent[]` → 期望 IRAction JSON,咬 S1 定的映射优先级):
  - `{kind:'click', testId:'login_btn'}` → 一条 `{"kind":"tap","selector":{"id":"login_btn"},"timestampMs":<ts>}`。
  - `{kind:'click', testId:undefined, role:'button', text:'Sign In'}` → `role` 在词表 → `{"kind":"tap","selector":{"role":"button"},...}`(testid 缺 → role 主路径次选)。
  - `{kind:'click', testId:undefined, role:undefined, text:'Continue'}` → `{"kind":"tap","selector":{"text":"Continue"},...}`。
  - `{kind:'input', testId:'email', value:'a@b.co', beforeValue:''}` → `{"kind":"fill","selector":{"id":"email"},"text":"a@b.co","timestampMs":N}`。
  - **折叠**:同 `testId:'email'` 连续 3 条 `input`(`'h'`,`'he'`,`'hel'`)→ **单条** `fill` `text:"hel"`。
  - **clear**:同源 `input`(`value:''`,`beforeValue:'hel'`)→ `{"kind":"clear",...}`(末值空且 before 非空)。
  - **gap 不臆造**:`{kind:'click', testId:undefined, role:'combobox', text:''}`(role 不在词表 + 无 text)→ **drop + unmapped++**;`{kind:'input', testId:undefined, role:'textbox', ...}`(textbox∉词表,input 无 text)→ drop + unmapped++。
- 文件:`crates/smix-recorder/tests/web_iraction_contract.rs` —— 把上面**同一批** JSON 字符串常量化,`serde_json::from_str::<IRAction>` 成功 + 变体正确 + `generate_maestro_yaml`/`generate_rust` 非空(镜像 `android_iraction_contract.rs`)。
- 跑红(须先失败一次):
  ```bash
  ( cd npm/smix-web-record && bun run test )                    # 无 RecordMapper → 红
  cargo test -p smix-recorder web_iraction_contract             # 无文件/常量 → 红
  ```

**绿(实现)**
- 文件:`npm/smix-web-record/package.json`(`name:@goliapkg/smix-web-record`,`private:true`,`type:module`,`scripts.test:"vitest run"`,`devDependencies`: `playwright` + `vitest` + `typescript` + `@types/node`)、`tsconfig.json`、`vitest.config.ts`(`environment:'node'`,`include:['src/__tests__/**/*.test.ts']`)。
- 文件:根 `package.json` workspaces 追加 `"npm/smix-web-record"`。
- 文件:`npm/smix-web-record/src/CapturedDomEvent.ts` —— `interface CapturedDomEvent { kind: 'click'|'input'; testId?: string; role?: string; text?: string; value?: string; beforeValue?: string; timestampMs: number }`。
- 文件:`npm/smix-web-record/src/RecordMapper.ts` —— `mapDomEvents(events: CapturedDomEvent[]): { actions: string[]; unmapped: number }`。关键点:① `click`→tap(selector 走优先级 `selectorFor(ev)`:testId→`{id}`、role(词表内)→`{role}`、text→`{text}`、否则 null);② `input`:同源(同 selector)相邻折叠取末值 → 末值空+before 非空→clear、否则→fill;③ selector 为 null → drop + `unmapped++`;④ role 词表判定用与 `parse_role` 同集(button/link/checkbox/radio/switch/tab/menu/menuitem/dialog/alert/slider/heading→staticText/image/table),不在集内不产 `{role}`;⑤ JSON key 用 `kind,selector,text?,timestampMs`(serde 解析,key 顺序无关)。
- 文件:`crates/smix-recorder/tests/web_iraction_contract.rs` 落地。
- 跑绿:上两条红命令转绿。

**重构(可选)**
- `selectorFor` 若与折叠逻辑纠缠,抽独立纯函数;不改行为。

### S3. Playwright bridge + headless chromium 录制 e2e:录一段 → IRAction[]

**（本步是 C3 定义性产出;唯一起浏览器的步骤 —— headless chromium,§9#1 = driver-层 DOM bridge 非真机,CI-able。规划期不起浏览器,此步在执行期跑。）**

**红(写测试)**
- 文件:`npm/smix-web-record/e2e/record-e2e.mjs`(node 脚本)—— 期望:import bridge → 启 headless chromium → 打开 file:// fixture → `startRecord` → 驱动 click/fill/clear → `stopRecord` 拿 `IRAction[]` → 断言序列。bridge 未实现 → import 失败/断言红。
- 跑红:
  ```bash
  node npm/smix-web-record/e2e/record-e2e.mjs   # 无 bridge → 红
  ```

**绿(实现)**
- 文件:`npm/smix-web-record/src/recorder-inpage.ts` —— 导出注入脚本字符串(或函数序列化):捕获相 `document.addEventListener('click'|'input', …, true)`,对 `event.target` 抽 `data-testid`(`getAttribute('data-testid')`)、ARIA role(`getAttribute('role')` 或 `tagName` 默认 role 映射,S3 实测定精度)、`textContent`(修剪)、input `value` + 缓存的 `beforeValue`,组 `CapturedDomEvent` 调 `window.__smixRecordReport(ev)`。
- 文件:`npm/smix-web-record/src/bridge.ts` —— `class WebRecorder`:构造持 Playwright `page`;`start()`:`page.exposeBinding('__smixRecordReport', (_src, ev)=>buffer.push(ev))` + `page.addInitScript(recorderInPage)`(active gate);`poll()`:drain buffer 经 `mapDomEvents` 返 IRAction JSON(流式清空);`stop()`:返剩余 + inactive;`RecordBuffer` 语义(active flag,inactive 丢弃)对齐 Android `RecordBuffer`。
- 文件:`npm/smix-web-record/e2e/record-fixture.html` —— 静态页:`<button data-testid="login_btn">`、`<input data-testid="email">`、一个可清空 input 的手段(如 `<button data-testid="clear_btn">` 触发清空并 dispatch input,或 e2e 里 `fill('')`)。**不用**依赖网络的资源。
- 文件:`record-e2e.mjs` 实现:`chromium.launch({headless:true})` → `page.goto('file://…/record-fixture.html')` → `WebRecorder.start()` → `page.getByTestId('login_btn').click()` → `page.getByTestId('email').fill('smix')` → 清空(`fill('')` 或点 clear_btn)→ `stop()` → jq/断言。
- **接线风险(S3 起浏览器实测处证伪,implement-discover loop,非 planning 期能定 —— 如实标)**:
  1. `addInitScript` 注入时机 vs SPA:fixture 是静态页无导航,预期 init 脚本一次注入即生效;真 SPA 的 re-render 是否漏事件 = 超 C3 fixture 范围,记 follow-on。
  2. `input` 事件 `beforeValue` 缓存:in-page 需按 target 缓存上次 value;capture 相监听是否覆盖所有输入方式(粘贴/IME)= S3 验,fixture 用直接键入。
  3. `page.getByTestId('email').fill('smix')` 是否 fire **单个** `input` 还是逐字符多个 → 折叠逻辑正是为此(Android 同款),S3 验折叠成一条。
  4. ARIA role 抽取精度:显式 `role` 属性 vs `tagName` 隐式计算 role;fixture 用 `data-testid` 走主路径规避,role 路径的隐式计算精度记为 role-gap 之外的次要 follow-on。
  - 这几条是 iOS `EventRecorder` registration dance / Android `TEXT_SELECTION_CHANGED` 噪音那类「源读不出、跑起来才现」的边界,正是 S3 e2e 存在的意义。
- 跑绿:`node npm/smix-web-record/e2e/record-e2e.mjs` 转绿(末行 `C3-E2E-PASS`)。

**重构(可选)**
- 无。

## Checkpoint C3 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

# —— S1 研究 gate ——
test -f docs/research/c3-web-capture.md \
  && grep -Eq '^VERDICT:' docs/research/c3-web-capture.md \
  && grep -q 'Falsification' docs/research/c3-web-capture.md \
  && echo S1-VERDICT-PRESENT

# —— Gate A:纯映射 + 契约锁(browser-free,硬门)——
bun install \
  && ( cd npm/smix-web-record && bun run test ) \
  && cargo test -p smix-recorder web_iraction_contract \
  && echo GATE-A-PASS

# —— Gate B:headless chromium 录制 e2e(CI-able,无真机)——
bunx playwright install chromium \
  && node npm/smix-web-record/e2e/record-e2e.mjs
# 期望 stdout 末行:C3-E2E-PASS(录到的 IRAction[] 序列断言过:tap{id=login_btn} → fill{id=email,text=smix} → clear{id=email})
```

期望:`S1-VERDICT-PRESENT` + `GATE-A-PASS` 打印且各命令 exit 0;`record-e2e.mjs` 末行 `C3-E2E-PASS` 且 exit 0。含义 =
① S1 研究文档 + VERDICT + falsification rubric 在(decomposition-before-attack 履约);
② `CapturedDomEvent → IRAction JSON` 纯映射(含 selector 优先级 / 折叠 / gap)browser-free 绿 + TS↔Rust 契约锁咬合(单元级证 web IRAction JSON → generator 非空)(Gate A);
③ headless chromium 对 file:// fixture 真录到 click/fill/clear → 断言得住的 `IRAction[]` 序列(Gate B)。

**不在 C3 验收内(诚实划界,归 C4)**:`record → generate → replay` 活 CLI glue —— 两平台皆缺,C4 跨平台一次建成。C3 的契约锁已在单元级证明 web 吐的 IRAction 喂 generator 非空(与 Android C2 同款达标)。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.10-c3-hot.md`。
2. **核心架构决策(web /record 腿注入式 DOM 捕获直吐 IRAction、selector 映射优先级、playwright devDep、glue 留 C4)已在热化时写入 `docs/v2.md` 决策日志**(`[v2.10-C3 热化期架构决策…]` 一条),无需重复;C3 收尾若 S1 VERDICT 或 S3 浏览器实测牵出与该决策相关的偏差(如 role 隐式计算精度、SPA 注入时机、testid 抽取不稳),另加一条 finding 记实测结果,不改原决策行(诚实留档)。
3. 调 sub-agent 热化 **C4(三平台 parity 闭合 + 统一 record→generate glue)**,见 CLAUDE.md §6。C4 一次建成 iOS/Android/web 三腿的 `/record` → generator glue(C2/C3 均已 re-tier 归此),跑跨平台「同操作录出等价 IRAction」parity gate,并收 Android C2 finding①(Clear 设备生成)。若 S3 接线风险(SPA 注入 / role 计算精度)翻出结构性障碍,如实记 finding + 由用户/上层拍板,不隐瞒、不硬凑。

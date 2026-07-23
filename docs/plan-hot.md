# plan-hot — v2.9 到 C3：退 `App.ts`/`Smix.ts` 的 napi 桩 → 经 smix-node 真调用 + 恢复 TS live sense

## 目标 checkpoint

C3：**`npm/smix-rn` 的 TS SDK 不再对可经 smix-node 兑现的驱动/感知面抛 `SmixNotImplementedError('napi', …)`。`App` 经一个 `NodeDriver`/`NodeSession` seam（其形正是 `crates/smix-node/index.d.ts` 的真实 napi 面，生产传入真 `SmixNodeDriver` 实例、测试传入 mock）真正驱动：`App.tap` = `snapshotTree`→resolver(selector→id)→`tapById`；`App.fill` = resolve→`tapById`(focus)→`inputText`；`pressKey`/`swipe`/`tapAtCoord`/`terminate`/`relaunch` 直映；`App.snapshotTree`/`systemPopups` 接回真树（TS live sense 复活，`Locator` 轮询重新有真树可查）；`Smix.launchApp` 退桩为 `openSession`→`launchApp`→构造已接线 `App`。三个 host 侧/无 wire 缺口（`screenshot`/`openUrl`/`launchFresh`）保留显式 not-implemented 但改标真实 blocker（不再谎称 napi）。退桩后 `route-conformance` rc=0 守住、`npm/smix-rn` vitest 套件绿、smix-node C2 五套件不回归。** 全程纯逻辑 + mock seam，不需真设备、不碰跨 triple prebuild（=C4）、不碰设备 e2e（=C5）。

## 前置条件

```bash
test -f docs/plan-history/v2.9-c2-hot.md                                   # C2 热计划已归档
grep -q 'tapById' crates/smix-node/index.d.ts                             # smix-node 的 napi 面已在（退桩要调的真身）
grep -q 'launchApp' crates/smix-node/index.d.ts                           # SmixNodeSession 生命周期面已在
grep -q "SmixNotImplementedError('napi'" npm/smix-rn/src/App.ts           # App.ts 12 桩仍在（本段要退的对象）
grep -q "SmixNotImplementedError('napi', 'Smix.launchApp')" npm/smix-rn/src/Smix.ts  # 第 13 桩（入口）仍在
python3 scripts/dev/route-conformance.py                                  # 基线 rc=0（终端直读退出码，非管道）
```

## 已经查清、不必重查的事实

- **「13 桩」的准确分布（本机 grep 核实）**：`App.ts` 有 **12** 个 `SmixNotImplementedError('napi', …)`（tap/fill/pressKey/swipe/screenshot/tapAtCoord/terminate/relaunch/launchFresh/snapshotTree/systemPopups/openUrl），第 **13** 个在 `Smix.ts`（`Smix.launchApp`）。冷计划表述「App.ts 的 13 个」把入口那一桩也算进了 —— 数目对、位置分两文件。要真正驱动，`App` 需要一个 driver+session，唯一注入点是 `Smix.launchApp`，故第 13 桩必须一并退，否则退了的 App 方法拿不到 driver、成死码。

- **screenshot / openUrl / launchFresh wire 缺口决策 = (c)（本机核实成本后拍板，已写 `docs/v2.md` 决策日志 2026-07-24）**：本段**退其余 9 桩、这 3 桩保留显式 not-implemented**，单列后续 checkpoint。依据（读源码核实）：
  - `grep -rnE 'screenshot|/screenshot|open.url|/open-url|open_url' crates/smix-runner-client/src/lib.rs crates/smix-runner-wire/src/lib.rs` 命中的全是**注释/无关词**（observation surface 描述、OCR 截图文本匹配），**没有 `/screenshot` / `/open-url` 的 `json_post`/`json_get` 路由**。smix-node `index.d.ts` 也无 screenshot/openUrl 方法（C2 已如实记为缺口）。补 (a) 要新增 `smix-runner-wire` 类型 + `smix-runner-client` 方法 + **iOS(Swift XCUITest runner) 与 Android(Kotlin runner) 双端**新路由 + route-conformance 登记 + 四 SDK parity —— 双端 runner 路由是大面，正是「不让 C3 膨胀」要避开的。走 (b) host 侧（SDK 直 shell `xcrun simctl io screenshot`）会把设备权威从 runner 挪进 SDK、破 §12 三层架构（感知/操作应落 core/runner 而非 SDK 直连 host），且 SDK 与 sim 未必同主机 —— 拒。故 **(c)**：截图/openUrl 应是**runner wire 能力**（正统、§13 质量/架构 clean >> 成本），作独立 checkpoint 补双端路由，不在 C3 hack。
  - `launchFresh(clearState/clearKeychain/appPath)` 的**清态是 host 侧 simctl**（无 runner wire；C2 表已记「清态是 host 侧，不过此边界」），`appPath` 安装亦 host 侧 —— 同属无 wire 缺口。Swift `App.swift` 亦**无** launchFresh/screenshot/openUrl（parity 参照里它们本就不存在）。故 launchFresh 与 screenshot/openUrl 同处理：保留 not-implemented。
  - **诚实标注修正**：这 3 桩保留时，把 `SmixNotImplementedError` 的 `stage` 参数从 `'napi'` 改为真实 blocker —— screenshot/openUrl → `'wire'`（缺 runner 路由，post-C3 单列），launchFresh → `'host'`（清态/安装是 host 侧）。C3 后 napi 已落地，再让消息说「lands napi」是假信息（`honesty/no-false-verified` 同源）。

- **parity 参照 = `swift-bridge/Sources/SmixSDK/App.swift`（读源码核实，逐方法照搬语义）**：
  - `App` 持 `driver`(tree/snapshot) + `session`(act/lifecycle) 两个 handle。TS 同构，但 **act/sense 动词归 `NodeDriver`、仅 launch/terminate/relaunch 归 `NodeSession`** —— 这是 C1/C2 已定的 smix-node 布局（与 UniFFI `driving.rs` 把无状态动词也挂 session 的差异是**组织层非能力层**，C2 已记）。故 TS `App` 里：tap/fill/pressKey/swipe/tapAtCoord/snapshotTree/systemPopups 走 `driver`；terminate/relaunch 走 `session`。
  - `tap` = `resolveFirstOrThrow`(driver.tree → resolveSelector → 第一个匹配，零匹配抛 `ELEMENT_NOT_FOUND` 且填 `visibleElements`=树前 20 节点) → `session.tapById(id)`。
  - `fill` = `resolveFirstOrThrow` → `tapById(id)`(focus) → `inputText(text)`。
  - `pressKey` = `session.pressKey(key.wireName)`，**`enter`→`return`**（TS `KeyName` 有 `enter` 别名，smix-node/runner 只认 `return`；不映射会被 runner 拒）。
  - `swipe` = `session.swipeOnce(direction)`；`tapAtCoord` = **先校验 `nx,ny∈[0,1]`，越界抛 `ASSERTION_FAILED`** → `session.tapAtNormCoord`；`terminate`/`relaunch` 直映。
  - `tree()`/`snapshotTree()` = `decodeTree(driver.tree())`；`systemPopups()` = decode `session.systemPopups()`。TS 侧对应 `JSON.parse(await driver.snapshotTree()) as A11yNode`、`JSON.parse(await driver.systemPopups()) as A11yNode[]`。

- **13 桩逐一去向（三列）**：

  | # | App.ts / Smix.ts 桩 | 去向 | 经 smix-node 面 |
  |---|---|---|---|
  | 1 | `App.tap` | 真调用 | `driver.snapshotTree` + resolver(TS) + `driver.tapById` |
  | 2 | `App.fill` | 真调用 | `driver.snapshotTree`+resolver + `driver.tapById`(focus) + `driver.inputText` |
  | 3 | `App.pressKey` | 真调用 | `driver.pressKey`(enter→return) |
  | 4 | `App.swipe` | 真调用 | `driver.swipe` |
  | 5 | `App.tapAtCoord` | 真调用 | 范围校验 + `driver.tapAtCoord` |
  | 6 | `App.terminate` | 真调用 | `session.terminateApp` |
  | 7 | `App.relaunch` | 真调用 | `session.relaunchApp` |
  | 8 | `App.snapshotTree` | 真调用（sense 复活） | `driver.snapshotTree` → `JSON.parse` |
  | 9 | `App.systemPopups` | 真调用 | `driver.systemPopups` → `JSON.parse` |
  | 10 | `App.screenshot` | **保留 not-implemented** `'wire'` | 无（缺 runner 路由，(c) 单列后续） |
  | 11 | `App.openUrl` | **保留 not-implemented** `'wire'` | 无（缺 runner 路由，(c) 单列后续） |
  | 12 | `App.launchFresh` | **保留 not-implemented** `'host'` | 清态/安装 host 侧，无 wire |
  | 13 | `Smix.launchApp` | 真调用（入口） | `driver.openSession`→`session.launchApp`→构造 `App` |

- **seam 设计（首选正统，镜像仓库既有注入范式）**：新建 `npm/smix-rn/src/NodeDriver.ts`，定 `interface NodeDriver`（`tapById(id):Promise<boolean>` / `inputText(text):Promise<void>` / `pressKey(key):Promise<void>` / `swipe(dir):Promise<void>` / `tapAtCoord(nx,ny):Promise<string>` / `snapshotTree():Promise<string>` / `systemPopups():Promise<string>` / `openSession(bundleId):Promise<NodeSession>`）+ `interface NodeSession`（`launchApp` / `terminateApp` / `relaunchApp`，皆 `Promise<void>`）—— **形与 `crates/smix-node/index.d.ts` 逐字对齐**，故真 `SmixNodeDriver` 结构上满足此 seam，生产直接传真实例。同文件给 `MockNodeDriver`/`MockNodeSession`（记录调用、返回预置 tree/popups JSON、`tapById`→true），镜像已有 `MockSelectorResolver`（`SelectorResolver.ts`）。这与 codebase 一贯的 seam 注入（resolver / `HttpFetch` 皆注入）同源，非分叉。
  - **为什么 seam+mock 而非在 TS 侧起 loopback http**：`App` 的退桩逻辑（snapshot→resolve→tapById 组装、范围校验、enter→return、tree/popups 的 `JSON.parse`）是**seam 之上的纯 TS 逻辑**；seam 边界正是 `SmixNodeDriver` 面，而该面已在 C2 对 `node:http` loopback 单测覆盖（`crates/smix-node/__test__/{act,sense,session}.test.mjs`）。TS 侧用 mock seam 测 `App` 组装逻辑 = 分层测各测其层，纯逻辑、确定、无原生构建、无设备，也不重复 C2 的 loopback 覆盖。真 `SmixNodeDriver`（C2 已 loopback 验）在生产结构式落进 seam。

- **`App` 构造 + `Smix.launchApp` 签名变更（注入 driver，同 resolver 已注入之范式）**：
  - `App` 构造改为 `(bundleId, driver: NodeDriver, session: NodeSession, resolver, labelsResolver?)`（driver/session 紧随 bundleId，对齐 Swift `init(bundleId:driver:session:)`；resolver/labelsResolver 是 TS 特有 seam，殿后）。
  - `Smix.launchApp` 改为 `(target, driver: NodeDriver, resolver, labelsResolver?)`：取 `bundleId`（`target.kind==='bundleId'` → `value`）→ `const session = await driver.openSession(bid)` → `await session.launchApp()` → `return new App(bid, driver, session, resolver, labelsResolver)`。
  - **`appPath` target = 已知缺口（host 侧安装，非本段）**：`launchApp` 对 `appPath` target 抛清晰错误（安装是 host 侧，同 launchFresh 主题），不静默。bundleId 路径是黄金路径，本段兑现。

- **两个 "Session" 概念并存 = 已识别 finding，非本段统一**：现有 `Session`(Session.ts) 是 resolver/HTTP 侧的 `/session/*` 会话（`Session-Id` header + `X-Sim-Health`），走 TS `fetch`→runner；新退桩后 `App` 的 lifecycle 走 `NodeSession`(napi→`smix-runner-client`→runner)。二者是打同一 runner 的**两条独立 HTTP 客户端**（一 Rust 经 napi、一 TS 经 fetch），`Session-Id` 未跨二者协调。C3（mock 测组装 + route-conformance）不受影响；真设备上二客户端的会话协调属 **C5 e2e** 范畴。如实记，不在 C3 塞入统一改造。

- **生产接真 `.node` = C4 边界（非本段隐缺口）**：本段交付 seam + 退桩 + mock 覆盖。`npm/smix-rn` 对 `@goliapkg/smix-node` 的**硬依赖 + 真 `.node` 工厂 + 跨 triple prebuild 分发**属冷计划 **C4**（`.node` 预构建 matrix + npm `npm/` 范式分发）。现 smix-node 只有 darwin-arm64 prebuild、未发布，若此刻在 smix-rn 加硬依赖会让非 darwin-arm64 平台 `install` 破 —— 故真 import 留 C4，与冷计划 C3/C4 切分一致。seam 已让「经 smix-node 真调用」成立（生产传真 `SmixNodeDriver` 实例即真 napi 调用）。

- **退桩不引新 runner 路由字符串 → route-conformance 天然守 rc=0**：退桩后 `App.ts` 调 `driver.tapById(...)` 等 seam 方法、**不含任何新路由字面量**（真路由在 napi→`smix-runner-client`，route-conformance 已经其 Rust 客户端看到、C2 已覆盖）。故本段 TS 侧不新增待服务路由，基线 rc=0 无损。checkpoint 仍**跑 `route-conformance.py` 作 gate**（终端直读退出码，**不** `| tail; echo $?` —— 那读的是 tail 的码不是脚本的）。

- **vitest 工具链（`npm/smix-rn/package.json` 核实）**：`"test": "vitest run"`、`"typecheck": "tsc --noEmit -p tsconfig.json && tsc -p tsconfig.test.json"`，`vitest ^2` 在 devDeps，`vitest.config.ts` 在。跑法 = `cd npm/smix-rn && bun run test` / `bun run typecheck`。TS strict + `noUncheckedIndexedAccess` + `exactOptionalPropertyTypes` 全开（§8.3）—— 构造签名变更后 typecheck 必须一并绿。既有 `__tests__/MvpApiShape.test.ts` 的「pending-napi surface throws」块断言 tap/fill/snapshotTree/systemPopups/`Smix.launchApp` 抛 napi，**退桩后这些不再抛** → 该块必须随退桩改写（改为经 mock driver 真驱动 / 或断言保留的 3 桩改标后仍 throw）。

- **⚠️ 补充核实（实现前发现，本段热化 agent 漏了 —— 必须一并处理，否则 checkpoint 红/README 谎报）**：`App` 构造签名从 2 参（`bundleId, resolver`）改 5 参，`grep -rn 'new App(' npm/smix-rn/src` 命中**三处** caller，热化只提了 MvpApiShape 两处，**漏了第三处 `ReadmeSnippets.test.ts:105`**（`new App('com.example.app', runtime.resolver)`，`HttpSimRuntime` 造 resolver）。更关键：`ReadmeSnippets.test.ts` 的契约是 **读 `README.md` → 正则提取所有 ``App.<m>`` / ``Smix.<m>`` 名 → 断言每个被点名的 App 方法都 `rejects.toBeInstanceOf(SmixNotImplementedError)`**（守「README 说会抛就真会抛」）。而 `README.md:9` 明写「**the driving methods (`Smix.launchApp`, `App.tap`, `App.fill`, …) throw** [SmixNotImplementedError]」。C3 让这 9 个真工作后：
  1. **`README.md` 必须改**：line 9 的「driving methods … throw」对退掉的 9 个不再成立 —— 改为「driving works through the napi addon (lands wire distribution in a later release); `App.screenshot`/`openUrl`/`launchFresh` still throw pending wire/host」之类，**只保留对 screenshot/openUrl/launchFresh 的 throw 声明**。这是 npm 落地页的中心警示，措辞要准（`honesty/no-false-verified` 同源：README 不能对已工作的方法说「会抛」）。
  2. **`ReadmeSnippets.test.ts` 必须改**：`new App(...)` 补 driver+session(mock)；「每个 README-named App 方法都 throw napi」的断言改为「只有 README 仍点名为 throw 的（screenshot/openUrl/launchFresh）throw、其余经 mock driver 真驱动」。
  3. **`SmixNotImplementedError` 消息措辞**：其 message 模板是 ``${api} not implemented yet (lands ${stage})``（`Locator.ts:186`）。stage 改 `'wire'`/`'host'` 后消息成「lands wire」/「lands host」—— 语义可接受（「等 wire 路由 / host 侧落地」），但确认 README 与该措辞一致。
  → **S3 的范围要含 README.md 改写 + ReadmeSnippets.test.ts 改写**（原 S3 只写了 MvpApiShape）。README 是面向 npm 的公开文档：改的是仓库源文件（非 `npm publish`，发布仍顺延用户授权），但措辞要当公开声明对待、准确无谎。

## 步骤（线性，3 个，按面分组）

### S1. seam + 恢复 live sense + 退入口桩（`Smix.launchApp` + `App` 构造）

**红（写测试，先失败一次）**
- 文件：`npm/smix-rn/src/__tests__/AppDriving.test.ts`（新建，vitest）。
- 断言（用 `MockNodeDriver`/`MockNodeSession` + `MockSelectorResolver`）：
  1. `MockNodeDriver.snapshotTree` 预置返回 `{"rawType":"application","identifier":"root"}`；`new App('bid', driver, session, resolver)` 的 `await app.snapshotTree()` 返回对象 `.identifier === 'root'`（且 `app.tree()` 同）。
  2. `MockNodeDriver.systemPopups` 预置 `[{"rawType":"alert","identifier":"p1"}]`；`await app.systemPopups()` 长度 1、`[0].identifier === 'p1'`。
  3. `Smix.launchApp(bundleId('com.acme.app'), driver, resolver)` resolve 出一个 `App`，且 `driver.openSession` 收到 `'com.acme.app'`、`session.launchApp` 被调一次（mock 记录）。
- 跑：`cd npm/smix-rn && bun run test src/__tests__/AppDriving.test.ts` → 期望**红**（`App` 构造签名旧、`snapshotTree`/`systemPopups`/`Smix.launchApp` 仍抛 napi）。

**绿（实现，最少代码转绿）**
- 新文件 `npm/smix-rn/src/NodeDriver.ts`：`interface NodeDriver` + `interface NodeSession`（形对齐 `crates/smix-node/index.d.ts`）+ `MockNodeDriver`/`MockNodeSession`（预置 tree/popups JSON、记录 `calls`、`openSession` 返回 mock session）。
- `npm/smix-rn/src/App.ts`：
  - 构造改 `(bundleId, driver: NodeDriver, session: NodeSession, resolver, labelsResolver?)`；`import type { NodeDriver, NodeSession } from './NodeDriver.js'`。
  - `snapshotTree()` = `JSON.parse(await this.driver.snapshotTree()) as A11yNode`；`systemPopups()` = `JSON.parse(await this.driver.systemPopups()) as A11yNode[]`。（`tree()` 不改，仍委托 `snapshotTree()`。）
- `npm/smix-rn/src/Smix.ts`：`launchApp(target, driver: NodeDriver, resolver, labelsResolver?)` 退桩 = 取 bundleId（`appPath` target 抛清晰 host-侧-安装错误）→ `openSession`→`launchApp`→`new App(...)`。
- `npm/smix-rn/src/index.ts`：导出 `NodeDriver`/`NodeSession` 类型 + `MockNodeDriver`/`MockNodeSession`。
- 关键点：① sense 经 driver 真回树（`Locator` 轮询即刻有真树）；② seam 注入同 resolver 范式，无原生依赖；③ 构造签名变更牵动 typecheck，配合 S3 全量修测试。
- 跑：`cd npm/smix-rn && bun run test src/__tests__/AppDriving.test.ts` → 期望**绿**。

**重构（可选）**
- 若 `bundleId` 抽取在 launchApp 内重复，抽 `targetBundleId(target)` 小函数；不改行为。

### S2. 退 act 桩：`tap`/`fill`/`pressKey`/`swipe`/`tapAtCoord`/`terminate`/`relaunch`

**红（写测试，先失败一次）**
- 文件：`npm/smix-rn/src/__tests__/AppDriving.test.ts`（续加 act 断言）。
- 断言（`MockNodeDriver` 记录 `tapById`/`inputText`/`pressKey`/`swipe`/`tapAtCoord` 调用序，`MockSelectorResolver.registerHit`）：
  1. `snapshotTree` 预置树；resolver 对 `Selector.id('btn-ok')` 的 selectorJson 注册 `['btn-ok']`；`await app.tap(Selector.id('btn-ok'))` 后 `driver.tapById` 收 `'btn-ok'`（snapshot→resolve→tapById 组装）。
  2. resolver 返回 `[]`（无匹配）：`app.tap(...)` 抛 `ExpectationFailure`，`code==='ELEMENT_NOT_FOUND'`，`visibleElements` 非空（来自 parse 的树）。
  3. `await app.fill(Selector.id('inp'), 'hello')` → 调用序 = `tapById('inp')` 后 `inputText('hello')`。
  4. `await app.pressKey('enter')` → `driver.pressKey` 收 `'return'`（enter→return 映射）；`await app.pressKey('escape')` 收 `'escape'`。
  5. `await app.swipe('up')` → `driver.swipe` 收 `'up'`。
  6. `app.tapAtCoord(1.5, 0.5)` 抛 `ExpectationFailure` `code==='ASSERTION_FAILED'`（越界）；`await app.tapAtCoord(0.5, 0.5)` 调 `driver.tapAtCoord(0.5,0.5)`。
  7. `await app.terminate()` 调 `session.terminateApp`；`await app.relaunch()` 调 `session.relaunchApp`。
- 跑：`cd npm/smix-rn && bun run test src/__tests__/AppDriving.test.ts` → 期望**红**（七 act 方法仍抛 napi）。

**绿（实现）**
- `npm/smix-rn/src/App.ts` 退七桩（照搬 Swift `App.swift` 语义）：
  - 私有 `resolveFirstOrThrow(selector)`：`const treeJson = await this.driver.snapshotTree(); const tree = JSON.parse(treeJson) as A11yNode; const ids = await this.resolver(treeJson, encodeSelectorJson(selector))`（resolver 已吃 JSON 串，直喂不 re-encode）；`ids[0]` 空 → 抛 `ExpectationFailure{code:'ELEMENT_NOT_FOUND', visibleElements: flatten(tree).slice(0,20), suggestions:[…]}`；返回 `ids[0]`。
  - `tap` = `await this.driver.tapById(await this.resolveFirstOrThrow(selector))`。
  - `fill` = `const id = await this.resolveFirstOrThrow(sel); await this.driver.tapById(id); await this.driver.inputText(text)`。
  - `pressKey` = `await this.driver.pressKey(key === 'enter' ? 'return' : key)`。
  - `swipe` = `await this.driver.swipe(direction)`。
  - `tapAtCoord` = 校验 `nx,ny∈[0,1]`（越界抛 `ExpectationFailure{code:'ASSERTION_FAILED'}`）→ `await this.driver.tapAtCoord(nx, ny)`（丢弃返回的 hit-chain 串）。
  - `terminate` = `await this.session.terminateApp()`；`relaunch` = `await this.session.relaunchApp()`。
  - 补 `import { encodeSelectorJson } from './Selector.js'` + `import { flatten } from './A11yNode.js'`。
- 关键点：① selector 不过 napi 边界（resolve 在 TS 侧，喂 `driver.tapById(id)`，与 C2/`driving.rs` 决策一致）；② 失败码贴 Swift（`ELEMENT_NOT_FOUND`/`ASSERTION_FAILED`）；③ `resolveFirstOrThrow` 只在 miss 路径需要 `flatten` 树填 `visibleElements`。
- 跑：`cd npm/smix-rn && bun run test src/__tests__/AppDriving.test.ts` → 期望**绿**。

**重构（可选）**
- 无。

### S3. host/wire 缺口改标 + 全量套件与 gate 收口

**红（写测试，先失败一次）**
- 文件：`npm/smix-rn/src/__tests__/MvpApiShape.test.ts`（改写「pending-napi surface throws」块）。
- 断言：
  1. 保留的 3 桩仍抛 `SmixNotImplementedError`，但 `stage` 改标：`app.screenshot()`/`app.openUrl('x')` → `stage==='wire'`；`app.launchFresh()` → `stage==='host'`（不再是 `'napi'`）。
  2. 退桩后的面**不再**抛 `SmixNotImplementedError`：经 mock driver，`app.tap`/`app.snapshotTree`/`app.systemPopups`/`Smix.launchApp` 真驱动（复用 S1/S2 的 mock 断言迁入或引用），删去旧的「throws napi」断言。
  3. `Locator` 断言（`app.find(...).toBeVisible`）在 mock driver 回可见节点树时 resolve、不再抛 napi。
- 跑：`cd npm/smix-rn && bun run test` → 期望**红**（旧 MvpApiShape 仍断言退桩面抛 napi，与新行为冲突而失败）。

**绿（实现）**
- `npm/smix-rn/src/App.ts`：`screenshot`/`openUrl` 保留 `throw new SmixNotImplementedError('wire', 'App.screenshot'|'App.openUrl')`（附一行注释：runner wire 路由缺，(c) 决策单列后续 checkpoint）；`launchFresh` 保留 `throw new SmixNotImplementedError('host', 'App.launchFresh')`（注释：清态/appPath 安装 host 侧）。
- 改写 `MvpApiShape.test.ts` 使全量绿：更新导入（`MockNodeDriver`/`MockNodeSession`）、把退桩面断言改为真驱动、3 桩断言改标。
- 关键点：诚实标注 —— napi 已落地，保留桩的 blocker 是 wire/host 而非 napi（`honesty/no-false-verified` 同源）。
- 跑：`cd npm/smix-rn && bun run typecheck && bun run test` → 期望**绿**（typecheck 覆盖构造签名变更；全 vitest 套件绿）。

**重构（可选）**
- 若 S1/S2 与 MvpApiShape 的 mock 装配重复，抽一个测试内 `makeApp()` helper；不改断言。

## Checkpoint C3 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix \
  && python3 scripts/dev/route-conformance.py \
  && ( cd npm/smix-rn && bun install && bun run typecheck && bun run test ) \
  && ( cd crates/smix-node && bun install && bun run build \
        && node --test __test__/load.test.mjs __test__/tap.test.mjs \
             __test__/act.test.mjs __test__/sense.test.mjs __test__/session.test.mjs ) \
  && echo C3-PASS
```

期望：stdout 末尾打印 `C3-PASS`，exit 0。含义（`&&` 链让任一环非零即中止、无 `C3-PASS`）：
1. **`route-conformance.py` 退出码由终端直读**（是链的第一环，其真实 rc 直接 gate；**不经 `| tail`**）—— rc=0：退桩后 TS 侧无新增待服务路由，parity 基线守住。
2. `npm/smix-rn` **typecheck 绿**（TS strict 全开；`App`/`Smix.launchApp` 构造签名变更 + seam 类型无洞）+ **vitest `bun run test` 全绿**（退桩后 `App` 的 snapshot→resolve→tapById 组装、fill focus+input、enter→return、tapAtCoord 范围校验、terminate/relaunch、live sense、`Smix.launchApp` 装配，全经 mock seam 覆盖；保留 3 桩改标后仍 throw）。
3. `crates/smix-node` **C2 五套件不回归**（`.node` 重建 + load/tap/act/sense/session 全过）—— 退 TS 桩未触碰 napi crate。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.9-c3-hot.md`。
2. screenshot/openUrl/launchFresh 缺口决策 (c) 已在本段执行前写入 `docs/v2.md` 决策日志（2026-07-24 一行）；无需重复。
3. 调 sub-agent 热化 **C4**（跨 triple prebuild matrix —— darwin-arm64/x64 + linux-x64 的 `.node` 预构建 + CI + npm `npm/` 范式分发；此时把 `npm/smix-rn` 对 `@goliapkg/smix-node` 的真 import/硬依赖接线，因预构建到位、非 darwin-arm64 平台不再破 install），见 CLAUDE.md §6。C4 前须核实：本段留的 seam 形与 smix-node `index.d.ts` 仍逐字对齐、真 `SmixNodeDriver` 结构式满足 `NodeDriver`。

- **⚠️ 补充核实 #2（实现 S1 seam 时发现）：`systemPopups` 返回类型语义不一致**。`App.systemPopups()` 声明 `Promise<A11yNode[]>`（`App.ts:96` 旧桩），但 smix-node 的 `systemPopups()` 返回的是 wire `Vec<SystemPopup>`（`smix-runner-wire`：字段 `id`/`type`/`source`，**非 A11yNode 形**，`SystemPopup.id` ≠ `A11yNode.identifier`）。plan 原写 `JSON.parse(...) as A11yNode[]` 是**类型谎报**（把 SystemPopup 数据当 A11yNode 返回）。这是**真 API 设计决策**，非机械退桩：要么 (i) `App.systemPopups` 返回类型改 `SystemPopup[]`（TS 侧新增 `SystemPopup` 类型，与 wire 对齐；Swift/Kotlin parity 看它们怎么表达 popups）、要么 (ii) smix-node 侧把 popups 转成 A11yNode 形（但那扭曲 wire 语义）。**倾向 (i)**（正统：类型贴 wire 真相）。实现 C3 前先定此决策，记决策日志。—— 与 README 契约、ReadmeSnippets 一样，属 C3 在 TS SDK（新区域）暴露的设计级盲点，需审慎定夺非改一行。

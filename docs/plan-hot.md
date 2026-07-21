# plan-hot — v2.3 到 C5:最后三条需设备的缺陷全闭,ledger 的 `present` 归零

## 目标 checkpoint

C5:`docs/audit-ledger.md` 里**剩下的 3 条 `present`(⑤a / ⑨b / ⑩)全部变成 `fixed`**,
每条各带一条**不需要设备就能复跑**的断言;`audit-ledger-scan.py` 末行从
`16 rows (12 fixed / 3 present / 1 moot)` 变成 `16 rows (15 fixed / 0 present / 1 moot)`。

做完的样子:

- **⑤a** —— `/tap` `/double-tap` `/long-press` 三条 runner 路由都能解 `text` / `id` / `label`
  三种纯字面量选择器(今天只解 `text`)。`docs/ai-guide/04-actions.md:49-51` 那个
  `tapOn: {id: "btn-login", dispatch: daemonProxy}` 的文档示例**真能跑**,而不是在上 wire 前
  被 guard 拦下。role / regex / spatial-index modifier 三类**仍然**在上 wire 前被挡,错误信息
  据实说明为什么(判据见下面「范围裁定 A」)。
- **⑩** —— `describe()` 的 `front_app` 与 `captured_at` 填真值;`summary` 的承诺**收窄**;
  `crates/smix-cli/src/act.rs` 的 rustdoc 不再承诺全仓没有实现的 "title / status bar"。
- **⑨b** —— `smix authoring suggest 'id: qa-*'` 在活 runner 上真跑过一次,跑出来的树被落成
  **提交进仓库的 fixture**,答案由此变成一条不再需要设备的断言;clap 帮助补上它今天缺的那半句
  (无候选时退出码 1),那正是"帮助示例与实现不符"剩下的部分。
- 三条 ledger 行的状态 / 判据 / 层 / 核验日全部改真。

---

## 范围裁定(执行期不再议 —— 两条都在这里定死)

### A. 三条全做,且 ⑤a 的三条路由一起动

**判据(不是成本判据,是记账判据)**:

> ledger 的**行集合由 `docs/v2.md:642` 那条待办冻结**,不能新增行(C1 S1 立的规则,闸门的覆盖
> 检查以那一行为锚)。⑤a 这一行的文字只点了 `/tap`,但 `docs/v2.md:642` 的上下文与
> `crates/smix-driver/src/lib.rs:1017-1022` 的 rustdoc 点的是**三条路由**。
> 只修 `/tap` 会让这一行的状态词变成 `fixed`,而它命名的同一个缺陷仍活在两条兄弟路由里,
> **且没有任何一行能记录它** —— 这正是这张表前身烂掉的形态(true when written, false later)。
> 所以:要么三条一起动,要么一条都不动。

**实证核对(不照抄 v2.md,已逐文件读过)**:三条路由确实同病,且病得一模一样 ——

| 路由 | decode 位置 | 只读的键 | 谓词 |
|---|---|---|---|
| `/tap` | `TapRoute.swift:79-80` | `selector["text"]`,必须是 `String` | `SmixRunnerUITests.swift:1317` `label == %@ OR identifier == %@` |
| `/double-tap` | `DoubleTapRoute.swift:41-44` | 同上 | `SmixRunnerUITests.swift:2430` 同一条谓词 |
| `/long-press` | `LongPressRoute.swift:40-43` | 同上(外加 `durationMs`) | `SmixRunnerUITests.swift:2451` 同一条谓词 |

Rust 侧三个调用点也是同一个 guard:`crates/smix-driver/src/lib.rs:477 / 512 / 547` 都调
`require_plain_text_selector`。

**为什么不再往后推**:C5 是本冷计划**最后一个** checkpoint,再推没有下一段接住。C3 的归档
已经写明这三条**不需要用户拍板去留,只需要安排哪一段起设备** —— 推出去等于把"排期问题"
退化回"无主项",而无主项正是这张表的病根。

### A′. ⑤a 修到哪为止 —— 边界的判据

> 一种选择器形态进 runner 侧路由 ⟺ 它能被 runner **已有的**匹配面直接表达
> (`NSPredicate` 对 `label` / `identifier` 的严格相等),**不需要在 XCUITest 里再造一份
> 主机侧已经有的解析器**。

按这条判:

| 形态 | 进? | 依据 |
|---|---|---|
| `text: "Sign In"`(纯字面量)| 已在 | 现状 |
| `id: "btn-login"` | **进** | 谓词右半 `identifier == %@` 今天就在跑,缺的只是 wire 说不出"这是 id" |
| `label: "Sign In"` | **进** | 谓词左半 `label == %@` 同上 |
| `text: /^Sign/`(regex)| 不进 | 要在 Swift 里重写 `smix_selector::match_text` 的大小写 / flag 语义 = 一份契约两个实现 |
| `role: button` | 不进 | 要在 Swift 里重写 `rawType → Role` 映射表(28 个 role) |
| spatial / index modifiers | 不进 | 要在 XCUITest 里重写整棵树的空间与序号解析 —— 那是主机侧 resolver 的职责 |
| Focused / Anchor / OcrText / LocalizedText / AnchorRelative / Point / Fallback | 不进 | 同上,且各自另有已实现的分发路径 |

**关键**:今天的谓词 `label == %@ OR identifier == %@` 对 `id` 和 `label` 是**混淆**的 ——
一个 `id:` 选择器会误命中同名 label。所以这次不是"放宽 guard 让 id 混进 text",而是
**让 wire 说得出形态、让谓词分得开** —— 否则就是 §13 拒绝的那种"看起来修了"。

### B. ⑩ 走「补实现」,只有 `summary` 一个字段走「收窄承诺」

**判据(先说判据,再套字段)**:

> 一个恒空字段进「补实现」⟺ 它有**唯一确定的诚实来源**。有 → 填它,并把字段文档改成那个
> 来源实际能保证的说法,**不多说一个字**。没有唯一来源 → 「收窄」:把承诺删掉,不留一个
> 空着的坑给消费方。

**两条路都摆在这里,选定的是「补实现」**,因为探测推翻了 C3 记的前提:

> **C3 记的「树的 wire 上没有 bundle 字段」是错的。**
> `swift-bridge/SmixRunnerUITests/SmixRunnerUITests.swift:1534` 的
> `convertSnapshot(snap, rootIdentifierOverride: bundleId, …)` 把 bundle id 写进了
> **树根节点的 `identifier`**;see-through 路径
> (`SmixRunnerUITests.swift:3256-3258` `identifier: bundleId`)同样。
> 注释自陈动机:"host-side smoke gates can assert on `.identifier == "com.apple.Preferences"`"。
> 所以 `front_app` 的诚实来源**已经在 wire 上**,不需要新路由、不需要新字段。

逐字段:

| 字段 | 唯一诚实来源 | 裁定 |
|---|---|---|
| `captured_at` | 抓树那一刻的宿主墙钟 | **补实现**(零 runner 改动) |
| `front_app` | `/tree` 根节点 `identifier`(runner 已 emit) | **补实现**(host 读 + runner 一行改真,见下) |
| `summary` | 无 —— 字段文档自己写着 "(caller-populated)",一句话摘要的形状由消费方定,`describe()` 不是它的 owner | **收窄承诺** |

**runner 那一行为什么还得改**:`rootIdentifierOverride: bundleId` 里的 `bundleId` 是
**runner 启动时**由 `TargetBundleResolver.resolve` 定的常量,而同一个 handler 里的 `app` 是
**每请求重绑**的 `await resolveApp()`(`SmixRunnerUITests.swift:1485`,读
`App-Bundle-Id` 头 / `Session-Id`)。客户端换 bundle 时,快照来自 A 而根 identifier 写的是 B。
不改这一行,`front_app` 就是"大概率对" —— 这次的题眼恰好是不接受"大概率对"。

**同时校正字段文档的措辞**:`front_app` 今天写 "Bundle id of the frontmost app at capture
time"。runner 能保证的是"这次快照取自哪个 app",不是"谁在最前面"(两者在快照成功时几乎总是
一致,但"几乎"不是文档该写的词)。改成据实的说法 —— 这不是收窄承诺的替代品,是「补实现」
判据里"不多说一个字"的那一半。

---

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix

git status --short                          # 期望:只有 `?? docs/plan-hot.md`(本文件)
python3 scripts/dev/audit-ledger-scan.py    # 期望末行:
#   audit-ledger-scan: clean — 16 rows (12 fixed / 3 present / 1 moot), 16 citations re-evaluated
uptime                                      # 记进 S1 记账段,与热化时的数对照
pgrep -fl 'cargo|xcodebuild|gradle|emulator|smix run|runner.ts' | head -20
```

末行那串是本段的**起点快照**,验收里的那串是终点。

**热化时已完成的本机探测(下面按它写,不按 ledger / 冷计划的文字写)**:

- **机器负载**:`uptime` load **7.89 / 8.61 / 9.55**。别的会话在跑三份 cargo
  (`stables/mailrs` perf_gate、`goliajp/kevy` 两个)、一个 Gradle daemon(空转)、一个远端
  `mini` 上的 torajs。**没有** iOS 模拟器在跑(`simctl list devices -j` 逐台查,全部 `Shutdown`),
  也没有 runner batch / `smix run` 在跑 —— 本段起设备不会踩别人的活动 batch。
- **模拟器**:全部 `Shutdown`。本段用 `sim-smix-02` = `5D087114-ECB3-443C-8DDB-40EEF9CFB90C`。
  **任何 simctl 命令一律显式 UDID**;`booted` / `all` / 设备名会被 `scripts/dev/sim-guard.sh` 拦下
  (热化期间已实测被拦一次)。
- **抢核对策**:`xcodebuild` 是本段最重的一步,而机器上有三份别人的 cargo。所有 `xcodebuild`
  一律 `nice -n 10 … -jobs 2`。这是确定性的让核方式,**不等别人跑完**(等 = 分叉)。
- **`/double-tap` `/long-press` 与 `/tap` 确实同病**(逐文件读过,见「范围裁定 A」的表)。
  `v2.md:362` 的说法**成立**,不是转述。
- **谓词今天已经读 `identifier`**:`label == %@ OR identifier == %@`。所以 `tapOn: {text: "btn-x"}`
  配 `dispatch: daemonProxy` 今天**能**命中一个 testID —— 但那是**混淆**命中,不是 id 语义。
  真正拦住 `tapOn: {id: …}` 的是 Rust 侧 `require_plain_text_selector`
  (`crates/smix-driver/src/lib.rs:1023`),它在上 wire 前就返回具名 `DriverError`。
- **文档层已经在承诺一件做不到的事**:`docs/ai-guide/04-actions.md:47-51` 的 daemonProxy 示例写的
  正是 `id: "btn-login"`;`docs/ai-guide/wire-format.md:32` 把 body 写成
  `{ selector: { text: string }, … }`。两处都要跟着 S1 改。
- **runner 源要重新打包**:`crates/smix-runner-sources/src/lib.rs:21` 用 `include_bytes!` 把
  `data/swift-runner-sources.tar.gz` 编进 CLI,`smix runner up` 从它解出运行时源。**改完
  `swift-bridge/` 必须跑 `bash scripts/release/build-runner-tarball.sh`**,否则设备上跑的是旧
  Swift —— 这正是 v1.0.4-v1.0.9 六版一循环的元根因(MEMORY: `v1010_source_sync_systemic_fix`)。
- **`front_app` 的消费方只有两个构造点**:`crates/smix-driver/src/lib.rs:137` 与
  `crates/smix-sdk/src/lib.rs:1497`,全仓没有读它的代码。改字段类型的爆炸半径已量过,是最小的。
- **⑨b 的纯逻辑那半已经绿着**:`crates/smix-cli/src/authoring.rs:111-116` 的
  `matches_partial` 认前缀通配 `qa-*`,`suggest_id_wildcard` 单测(同文件 378 行)已经在跑。
  ⑨b 剩下的**只有端到端那一跳**:`cmd_suggest`(231 行)→ `act::fetch_tree_json` → 活 runner
  `/tree` → `serde_json::from_value::<A11yNode>` → `suggest_selectors` → 打印。这一跳从没被跑过。
- **⑨b 帮助里今天缺的那半句**:`cmd_suggest` 无候选时 `eprintln!` + **退出码 1**
  (`authoring.rs:238-241`),而 clap 帮助(`crates/smix-cli/src/main.rs:479-482`)只给了两个
  例子、没说退出码。这就是"帮助示例与实现不符"里**不需要设备就能判定**的那一半。
- **既有的红绿机制**:Swift 侧 `swift test` 覆盖 `SmixRunnerCore`(`Tests/SmixRunnerCoreTests/`
  已有 `TapRouteTests` / `DoubleTapRouteTests` / `LongPressRouteTests`),`ship.sh:53` 起是
  non-bypassable gate;`crates/smix-runner-wire/tests/tap_route_shape.rs` 用 `include_str!` 读
  Swift 源断言键字面量 —— 这两条是本段"设备无关的绿"的承重墙。

---

## 步骤(线性,无分叉)

### S1. ⑤a —— 选择器形态上 wire,三条路由一起动

**红(写测试)**

1. 文件:`swift-bridge/Tests/SmixRunnerCoreTests/RouteSelectorTests.swift`(新建)。
   测试类名固定为 `RouteSelectorTests`(验收命令按类名抓)。6 个断言:
   - `testDecodesTextForm` —— `{"text":"Sign In"}` → `.text("Sign In")`
   - `testDecodesIdForm` —— `{"id":"btn-login"}` → `.id("btn-login")`
   - `testDecodesLabelForm` —— `{"label":"Sign In"}` → `.label("Sign In")`
   - `testRejectsRegexObjectForm` —— `{"text":{"regex":"^Sign"}}` → 抛 `wrongType`
   - `testRejectsRoleForm` —— `{"role":"button"}` → 抛 `unsupportedSelectorForm`
   - `testRejectsEmptySelectorObject` —— `{}` → 抛 `unsupportedSelectorForm`
2. 三个既有路由测试各加一条 id 形态断言(`TapRouteTests` / `DoubleTapRouteTests` /
   `LongPressRouteTests`),并把今天断言 `.missingText` 的用例改成断言
   `.unsupportedSelectorForm` —— `missingText` 这个 case 在新形状下是死的,它的存在本身就是
   "只有 text 才算数"的残留。
3. 文件:`crates/smix-runner-wire/tests/tap_selector_forms.rs`(新建)。沿用
   `tap_route_shape.rs` 的 `include_str!` 手法读 Swift 源,3 个断言:
   - `tap_route_decodes_three_selector_keys` —— `RouteSelector.swift` 源里同时出现
     `"text"` / `"id"` / `"label"` 三个键字面量
   - `all_three_tap_routes_share_one_selector_decoder` —— `TapRoute.swift` /
     `DoubleTapRoute.swift` / `LongPressRoute.swift` 三份源里都出现 `RouteSelector.decode`
   - `selector_wire_keys_match_rust_selector_enum` —— 三个键与
     `smix_selector::Selector` 序列化出来的键一致(`Selector::Id{..}` → `{"id":…}`)
4. 文件:`crates/smix-driver/src/lib.rs` 的 `#[cfg(test)] mod tests`。6 个断言,函数名统一以
   `runner_resolvable_` 开头(验收按前缀数数):
   - `runner_resolvable_accepts_plain_text` / `_accepts_id` / `_accepts_label`
   - `runner_resolvable_rejects_regex_text` / `_rejects_role` / `_rejects_index_modifier`

期望红:
- `( cd swift-bridge && swift test )` —— `RouteSelectorTests` 编译失败(`RouteSelector` 不存在)
- `cargo test -p smix-runner-wire tap_selector_forms` —— 断言失败(源里没有那些字面量)
- `cargo test -p smix-driver runner_resolvable` —— 编译失败(新函数名不存在)

**把三处红的真实输出记进 S1 记账段**,不是"应该会红"。

**绿(实现)**

- 新文件:`swift-bridge/Sources/SmixRunnerCore/RouteSelector.swift`
  ```swift
  public enum RouteSelector: Equatable, Sendable {
    case text(String)
    case id(String)
    case label(String)

    public enum Failure: Error, Equatable {
      case unsupportedSelectorForm
      case wrongType(String)
    }

    public var raw: String       // 三种形态的字面量
    public var wireKey: String   // "text" | "id" | "label"

    public static func decode(from obj: [String: Any]) throws -> RouteSelector
  }
  ```
  关键点:
  1. 键的**存在**就是判别式 —— 与 `smix_selector::Selector` 的 `#[serde(untagged)]`
     (`crates/smix-selector/src/lib.rs:322-348`)同源。顺序固定 `text → id → label`,
     三个都没有 → `unsupportedSelectorForm`;有但不是 `String` → `wrongType`。
  2. **不加** role / regex / modifier 的解析(判据见「范围裁定 A′」)。
- 改 `TapRoute.swift` / `DoubleTapRoute.swift` / `LongPressRoute.swift`:
  - `TapRequest.Selector` 与 `selectorText: String` 统一换成 `RouteSelector`
  - `notFound(...)` 的 body 按 `wireKey` 输出真实的键(`{"selector":{"id":"…"}}`),
    不再一律写 `"text"`。**这是安全的**:Rust 侧 404 走
    `crates/smix-runner-client/src/lib.rs:218-222` 的 `TapNotFoundError`,body 是不透明字符串,
    没有结构解析(已核)。
  - 各自的 `DecodeError` 去掉死掉的 `missingText`,保留 `unsupportedSelectorForm` / `wrongType`
- 改 `swift-bridge/SmixRunnerUITests/SmixRunnerUITests.swift` 四处:
  - `tapHandler`(1294 起)、`doubleTapHandler`(2426 起)、`longPressHandler`(2447 起)的
    `NSPredicate` 按形态分流:`.text` → `label == %@ OR identifier == %@`(**保持今天的行为
    逐字节不变**);`.id` → `identifier == %@`;`.label` → `label == %@`
  - `firstSeeThroughMatch(app:text:)`(341 起)签名换成 `RouteSelector`,内部 `matches(_:)`
    按同一张分流表判
- 改 `crates/smix-driver/src/lib.rs`:
  - `require_plain_text_selector` 更名 `require_runner_resolvable_selector`(名字今天就在撒谎:
    它挡下的不止"非纯文本")。接受 `Selector::Text{Pattern::Text, 默认 modifiers}` /
    `Selector::Id{默认 modifiers}` / `Selector::Label{默认 modifiers}`,其余照旧拒
  - 错误信息重写:点名**还剩哪三类**被挡(regex / role / modifier)、为什么(主机侧默认
    tap 解全部 11 种形态,runner 侧路由不重造解析器)、替代路径是什么
- 改 `docs/ai-guide/wire-format.md:32` 的 body 形状为
  `{ selector: { text | id | label: string }, mode?: … }`
- 改 `docs/ai-guide/04-actions.md` 的 `dispatch: daemonProxy` 段:补一句它现在吃哪三种选择器、
  不吃哪三类
- **跑 `bash scripts/release/build-runner-tarball.sh` 重打 runner 源 tarball**(见前置条件末条)

**重构**

- `crates/smix-driver/src/lib.rs:1017-1022` 那段 rustdoc 说的是旧世界("the Swift side has no
  resolver for id / label / role / regex forms")。改完代码就得改断言 ——
  注释是断言,代码才是事实(MEMORY: `comments_are_claims_code_is_truth`)。

**ledger 行改动(闸门强制,漏改则验收第 5 条红)**

- ⑤a 状态 `present` → `fixed`
- 判据从缺陷代码改钉**修复代码**:
  `at swift-bridge/Sources/SmixRunnerCore/RouteSelector.swift:<行> "case id(String)"`
  —— 行号以改完后实测为准;**必须钉在代码行,不能钉注释行**(闸门检查 6)
- 「层」栏改 `—`(`fixed` 行必须是 `—`,闸门强制)
- 「可达性 / 理由」改写为:三条路由(`/tap` `/double-tap` `/long-press`)共用
  `RouteSelector`,解 text / id / label 三种纯字面量形态,谓词按形态分流不再混淆 id 与 label;
  regex / role / spatial-index modifier 仍在上 wire 前被
  `require_runner_resolvable_selector` 挡下并给出具名错误 + 替代路径,理由是让 runner 解它们
  等于在 XCUITest 里再造一份主机侧解析器
- 核验日改当天
- **栏内不许出现裸 `|`**(闸门按 7 格切行,一个管道会把整行从检查里抹掉)

---

### S2. ⑩ —— `front_app` / `captured_at` 填真,`summary` 收窄

**红(写测试)**

- 文件:`crates/smix-driver/src/lib.rs` 的 `#[cfg(test)] mod tests`。4 个断言,函数名统一以
  `describe_meta_` 开头:
  - `describe_meta_front_app_reads_tree_root_identifier` ——
    根 `identifier = "com.apple.Preferences"` 的树 → `Some("com.apple.Preferences")`
  - `describe_meta_front_app_is_none_without_root_identifier` ——
    根无 identifier → `None`(**不是空串**;空串等于"不知道"被伪装成"知道",正是本冷计划要收的病)
  - `describe_meta_captured_at_is_unix_millis` —— 返回值 > 2026-01-01 的毫秒数
  - `describe_meta_summary_is_not_produced_here` —— 断言 `ScreenDescription::default().summary`
    仍是空串,把"`describe()` 不 own 这个字段"钉成契约而不是遗漏
- 第 5 条断言,单独一个文件:`crates/smix-runner-wire/tests/tree_root_identity.rs`(新建),
  沿用 `tap_route_shape.rs` 的 `include_str!` 手法读 `SmixRunnerUITests.swift`,
  `tree_root_identifier_tracks_per_request_bundle` —— 断言 `rootIdentifierOverride:` 的实参
  含 `currentContext.bundleId`,**不是**裸的启动期常量。
  这条承重:runner 那一行改在 `SmixRunnerUITests`(XCUITest target),`swift test` 覆盖不到它,
  只有 `xcodebuild build-for-testing` 能编、只有设备能跑 —— 源级断言是它唯一的设备无关看守。
- 期望红:`cargo test -p smix-driver describe_meta` 编译失败(两个新函数不存在);
  `cargo test -p smix-runner-wire tree_root_identity` 断言失败(源里还是启动期常量)。

**绿(实现)**

- 文件:`crates/smix-screen/src/lib.rs`
  - `pub front_app: String` → `pub front_app: Option<String>`,加
    `#[serde(default, skip_serializing_if = "Option::is_none")]`
  - 字段文档改成据实的说法:这是**这份描述取自哪个 app** 的 bundle id(来源 = a11y 树根节点的
    identifier),`None` = 树根没带 identifier。**不写 "frontmost"** —— runner 能保证的不是那个
  - `summary` 字段文档改成:调用方自行填写;`describe()` 不产生它
  - 结构体头部那段 "`frontApp` / `summary` / `captured_at` are caller-populated metadata"
    跟着改真(现在只有 `summary` 是 caller-populated)
- 文件:`crates/smix-driver/src/lib.rs`
  ```rust
  pub fn front_app_of(tree: &A11yNode) -> Option<String>;   // 根 identifier,空串归 None
  fn captured_at_unix_millis() -> f64;
  ```
  `describe()`(132 行)改成填 `front_app: front_app_of(&tree)` 与
  `captured_at: captured_at_unix_millis()`;`summary` 保持空串(按契约)
- 文件:`crates/smix-sdk/src/lib.rs:1490-1500` 的 `App::describe` 同步改(同一份聚合,两个构造点)
- 文件:`crates/smix-cli/src/act.rs:236-238` 的 rustdoc —— 删掉 "title / status bar"
  两个全仓没有实现的承诺,改成它真打印的东西
- 文件:`swift-bridge/SmixRunnerUITests/SmixRunnerUITests.swift:1534` ——
  `rootIdentifierOverride` 从启动期常量 `bundleId` 换成**本次请求真正解析到的 bundle**
  (`SmixRunnerServer.currentContext.bundleId ?? bundleId`)。see-through 路径
  (3256 行 `identifier: bundleId`)同改。
  关键点:`resolveApp()` 的 session 分支(1096-1108)拿的是 session 表里的 app;
  取 bundle 时走同一条优先级链,**不要**另起一条 —— 两条链会漂。
- 改完 Swift 后**再跑一次** `bash scripts/release/build-runner-tarball.sh`

**重构**

- `crates/smix-screen/src/lib.rs:79` 那句结构体级注释与三个字段文档要互相对得上;
  一个说 "caller-populated" 另一个说 "at capture time" 的状态就是 ⑩ 的成因。

**ledger 行改动**

- ⑩ 状态 `present` → `fixed`
- 判据:`at crates/smix-driver/src/lib.rs:<行> "front_app: front_app_of(&tree)"`
- 「层」栏改 `—`
- 「可达性 / 理由」改写为:`front_app` 的诚实来源就在 `/tree` 根节点 identifier 上
  (runner 的 `rootIdentifierOverride`,C3 记的"树的 wire 上没有 bundle 字段"是错的);
  本段把 override 从启动期常量换成每请求解析到的 bundle,host 侧读它填 `front_app: Option`;
  `captured_at` 用抓树时刻墙钟;`summary` 无唯一来源,按 caller-populated 契约收窄承诺,
  `act.rs` 的 "title / status bar" 两条空承诺删掉
- 核验日改当天

---

### S3. ⑨b —— 起设备:一次会话里验 S1 / S2 的真行为,并把 ⑨b 的答案落成不需要设备的断言

**这一步是本段唯一起设备的一步。** 三条缺陷共用这一次设备会话:S1 / S2 的绿已经由设备无关的
断言盯住,设备在这里的职责是**证明它们在真机上确实是那个行为**,并**产出 ⑨b 的固定件**。

**红(写测试)**

- 文件:`crates/smix-cli/tests/authoring_live_tree.rs`(新建)。3 个断言,引用一份**尚不存在**的
  fixture `crates/smix-cli/tests/fixtures/live-tree-preferences-2026-07-22.json`:
  - `live_tree_deserializes_into_a11y_node` —— 实测树 JSON 能被 `serde_json::from_value::<A11yNode>`
    解出来(这是 `cmd_suggest` 端到端那一跳今天唯一没被证明的环节)
  - `suggest_id_wildcard_runs_on_live_tree` —— 帮助里那条 `id: qa-*` 原样喂给
    `suggest_selectors`,不 panic、返回一个列表(空与非空都算通过 —— 断言的是**这条 spec 在真实
    树上走得通**,不是 Preferences 里恰好有 `qa-` 前缀的 id)
  - `suggest_label_prefix_yields_candidates_on_live_tree` —— 用 fixture 里**真实存在**的一个
    label 前缀,断言 ≥ 1 个候选。这条防的是上一条空转通过(vacuous green)
- 期望红:`cargo test -p smix-cli authoring_live_tree` 失败,报 fixture 文件不存在。
  **这是真红,不是编的** —— 文件确实要由设备会话产出。

**绿(实现)—— 设备会话,按序执行**

0. **进设备前重跑一次占有者检查**(MEMORY: `runner_ops_check_batch_owner_first`):
   ```bash
   pgrep -fl 'runner.ts|smix run|supervise|xcodebuild|gradle' | head -20
   ```
1. 装当前树的 CLI(tarball 必须已由 S1 / S2 重打过):
   ```bash
   nice -n 10 cargo install --path crates/smix-cli --force
   smix runner install --force
   ```
2. 起模拟器与 runner,**显式 UDID**:
   ```bash
   xcrun simctl boot 5D087114-ECB3-443C-8DDB-40EEF9CFB90C
   smix runner up 5D087114-ECB3-443C-8DDB-40EEF9CFB90C --bundle com.apple.Preferences
   ```
3. **⑨b 的固定件**:
   ```bash
   smix authoring capture-tree crates/smix-cli/tests/fixtures/live-tree-preferences-2026-07-22.json
   smix authoring suggest 'id: qa-*' ; echo "exit=$?"
   smix authoring suggest 'General'  ; echo "exit=$?"
   ```
   两条 `suggest` 的**完整输出与退出码原样记进 S3 记账段**。它们是 ⑨b 那句
   "需 live runner 才能验示例是否原样跑通" 的兑现。
4. **S1 的真行为** —— 写一份三步 yaml 到 `/tmp/smix-c5-dispatch.yaml` 用 `smix run` 跑。
   yaml 是 `dispatch:` 的**真表面**(`smix tap` 没有 `--dispatch` 旗标,已核 `smix tap --help`),
   所以证据取在这一层:
   - `tapOn: {id: <A>, dispatch: daemonProxy}` → **命中**
   - `tapOn: {label: <B>, dispatch: daemonProxy}` → **命中**
   - `tapOn: {id: <B>, dispatch: daemonProxy, optional: true}` → **不命中**
   第三条是判别性的那条:它证明 `id` 不再退化成 label 匹配(今天的谓词
   `label == %@ OR identifier == %@` 会让它误命中)。`optional: true` 让这一步的未命中不终止 flow,
   由 `--format json` 的 run-summary 判读。
   `<A>` / `<B>` 从第 3 步落盘的 fixture 里**按规则取**(不是分叉):
   `<A>` = 第一个 `identifier` 非空的节点的 identifier;
   `<B>` = 第一个 `label` 非空、且该串不出现在任何节点 `identifier` 上的节点的 label。
   ```bash
   smix run /tmp/smix-c5-dispatch.yaml --device 5D087114-ECB3-443C-8DDB-40EEF9CFB90C \
     --bundle-id com.apple.Preferences --format json
   ```
   完整 run-summary 记进 S3 记账段。
5. **S2 的真行为** —— `front_app` 跟着 runner 绑定走:
   ```bash
   smix describe --json | head -40                       # 期望 frontApp = com.apple.Preferences
   smix runner down
   smix runner up 5D087114-ECB3-443C-8DDB-40EEF9CFB90C --bundle com.apple.mobilesafari
   smix describe --json | head -40                       # 期望 frontApp = com.apple.mobilesafari
   ```
   两次 `capturedAt` 都 > 0 且第二次更大。两段输出原样记进 S3 记账段。
   (`smix describe` 没有 `--bundle-id` 旗标,已核 —— 所以换绑走 `runner up --bundle`,
   不编一个不存在的旗标。每请求 `App-Bundle-Id` 头那一半由 S2 的源级断言盯住,见 S2 红的第 5 条。)
6. **收尾回收**(不留 booted 模拟器,MEMORY: `simx_sim_recycle_after_use`):
   ```bash
   smix runner down
   xcrun simctl shutdown 5D087114-ECB3-443C-8DDB-40EEF9CFB90C
   ```
   仓库里没有 `simx-sweep.sh`(已查,`scripts/dev/` 下不存在)—— 回收就是上面这两条。

**绿(代码)—— 与设备结果无关的那半,无条件做**

- `crates/smix-cli/src/main.rs:479-482` 的 `Suggest` clap 帮助补一句:无候选时退出码 1。
  这一句今天就缺,是"帮助示例与实现不符"里不需要设备判定的那一半
  (`authoring.rs:238-241` 是它的出处)。**无条件加**,不看设备跑出什么。

**重构**

- 无。fixture 是数据,不重构。

**ledger 行改动**

- ⑨b 状态 `present` → `fixed`
- 判据:`at crates/smix-cli/tests/authoring_live_tree.rs:<行> "live-tree-preferences-2026-07-22.json"`
- 「层」栏改 `—`
- 「可达性 / 理由」改写为:端到端那一跳(`cmd_suggest` → `fetch_tree_json` → `A11yNode` →
  `suggest_selectors`)已在 `sim-smix-02` 上对 `com.apple.Preferences` 实跑,树落成仓库内
  fixture,3 条断言从此不需要设备复跑;帮助补上无候选退出码 1 这句
- 核验日改当天

---

## Checkpoint C5 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

# 1. S1 的绿 —— Swift 侧(不起设备)
( cd swift-bridge && swift test ) 2>&1 | tee /tmp/smix-c5-swift.log | tail -3
grep -q "Test Suite 'RouteSelectorTests' passed" /tmp/smix-c5-swift.log

# 2. S1 的绿 —— Rust 侧(数数,不看退出码:过滤器匹配 0 个测试也是 exit 0)
cargo test -p smix-runner-wire tap_selector_forms 2>&1 | grep -q 'test result: ok. 3 passed'
cargo test -p smix-driver runner_resolvable 2>&1 | grep -q 'test result: ok. 6 passed'

# 3. S1 的 UITest 真能编(让核,不抢别人的 cargo)
( cd swift-bridge && nice -n 10 xcodebuild build-for-testing -scheme SmixRunner \
    -destination 'generic/platform=iOS Simulator' -jobs 2 ) > /tmp/smix-c5-uitest.log 2>&1

# 4. S2 / S3 的绿
cargo test -p smix-driver describe_meta 2>&1 | grep -q 'test result: ok. 4 passed'
cargo test -p smix-runner-wire tree_root_identity 2>&1 | grep -q 'test result: ok. 1 passed'
cargo test -p smix-cli authoring_live_tree 2>&1 | grep -q 'test result: ok. 3 passed'

# 5. 记账翻转到位,且 16 条判据全部重新求值通过
python3 scripts/dev/audit-ledger-scan.py

# 6. 三处改真 + 文档跟上代码的机器判据
grep -q 'RouteSelector' docs/audit-ledger.md
grep -q 'rootIdentifierOverride' docs/audit-ledger.md
grep -q 'live-tree-preferences' docs/audit-ledger.md
grep -q 'require_runner_resolvable_selector' docs/audit-ledger.md
grep -q 'text | id | label' docs/ai-guide/wire-format.md
test -f crates/smix-cli/tests/fixtures/live-tree-preferences-2026-07-22.json

# 7. runner 源 tarball 不是陈的(改了 swift-bridge 就必须重打)
bash scripts/release/build-runner-tarball.sh
git diff --quiet -- crates/smix-runner-sources/data

# 8. 既有闸门没被本段破坏
python3 scripts/dev/hygiene-scan.py
python3 scripts/dev/workflow-scan.py
python3 scripts/dev/route-conformance.py
python3 scripts/dev/fact-scan.py
python3 scripts/dev/scope-promise-scan.py
bash scripts/dev/preflight.sh

# 9. 设备回收干净(不留 booted 机器)
xcrun simctl list devices -j | python3 -c "import json,sys; d=json.load(sys.stdin)['devices']; \
  bad=[v['udid'] for vs in d.values() for v in vs if v['state']!='Shutdown']; \
  print('non-shutdown:', bad); sys.exit(1 if bad else 0)"
```

期望:

- 第 1 条:`grep -q` exit 0(新测试类跑过且全绿)。
- 第 2 条:两行 `grep -q` 都 exit 0。**先看到红再看到绿** —— S1 三处红相的真实输出必须已记进
  S1 记账段,验收只复跑绿。
- 第 3 条:exit 0(UITest 编译通过;**不起模拟器**,`generic/platform` destination)。
- 第 4 条:三行 `grep -q` 都 exit 0。
- 第 5 条:exit 0,末行**逐字符等于**:
  `audit-ledger-scan: clean — 16 rows (15 fixed / 0 present / 1 moot), 16 citations re-evaluated`
  (起点是 `12 fixed / 3 present`;这一串变化本身就是"改了哪三行"的判据)
- 第 6 条:6 条全 exit 0。闸门**看不出"状态词与判据语义是否相符"**(它的 docstring 明写),
  所以这几条是人给出的、可机器复查的补充。
- 第 7 条:`git diff --quiet` exit 0 —— 重打出来的 tarball 与树里那份一致,说明 Swift 改动确实
  进了会发给用户的那份源。
- 第 8 条:六个闸门全 exit 0,preflight 末行 `preflight: clean`。
- 第 9 条:exit 0,`non-shutdown: []`。

**第 3 / 5 / 6 / 7 条是本段"设备段也要给得出半年后确定结论"的实现方式**:设备只在 S3 出现一次,
它的产物(fixture、记账段里的实测输出)留在仓库里;所有验收命令**没有一条需要模拟器**。

---

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.3-c5-hot.md`
2. 在 `docs/v2.md` §10 决策日志末尾追加(不动 07-19 那条历史原文):
   - ⑤a 修到哪为止 + 那条边界判据(为什么 regex / role / modifier 不进 runner 侧路由)
   - **C3 把 ⑩ 的可达性记错了** —— "树的 wire 上没有 bundle 字段"是错的,bundle id 一直在树根
     `identifier` 上(`SmixRunnerUITests.swift:1534` 的 `rootIdentifierOverride`)。这条必须写:
     C1 记错两条、C3 记错一条,**同一张表在四段里失真了三次**,这个频次本身是结论
   - ⑩ 的字段级裁定(两个补实现、一个收窄)与它的判据
   - ⑨b 的设备实测结论 + fixture 位置
   - S2 顺带发现的 `rootIdentifierOverride` 用启动期常量的问题(已修,记在这里留出处)
   - **追加时注意**:正文若写到会被 sim-guard / adb-guard 拦的命令形状,heredoc 正文会被 guard
     当命令读(07-21 已发生过一次)—— 改措辞或改用编辑工具写入,**不改 guard**
3. 在 `docs/plan-cold/v2.3-release-truth.md` 的「出口验收」补一句:ledger 的 `present` 计数
   归零是本冷计划的终局判据,并标注 C5 已闭。
4. 按 §7 收尾 task 状态(S1 / S2 / S3 三个 task 全 `completed`)。
5. **不自行热化下一段**(§6)。本冷计划到此结束,报给用户:三条缺陷的修法与实测、C3 那处记错的
   更正、ledger 归零后的下一步由用户拍板(v2.3 收口 / 起新冷计划 / 进发布)。

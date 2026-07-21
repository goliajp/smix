# plan-hot — v2.3 到 C3:两条主机侧缺陷修掉,三条记错的账改真

## 目标 checkpoint

C3:`docs/audit-ledger.md` 里 **5 条 `present` 中的 2 条变成 `fixed`,且各自带一条会红的断言**;
剩下 3 条里有 **2 条的「可达性」栏是 C1 写错的**,本段把它们改成属实的说法。

做完的样子:

- ② `webview_eval` 的 iOS bridge 端口不再写死 —— `SMIX_WEBVIEW_BRIDGE_PORT` / builder 可覆盖,
  默认仍 28080;4 个单元测试钉住解析与 URL 组装。ledger ② → `fixed`,判据钉在读 env 的那行代码上。
- ⑧ `smix run` 的端口优先级链(flag/env → 注册表 `runnerPort` → 22087)成为一个可测的纯函数,
  4 个单元测试钉住它,含"有 flag 时不去读注册表"这一条惰性。ledger ⑧ → `fixed`。
- ⑤a / ⑩ 两行的「可达性」栏改成实测属实的说法(状态、判据、层都不动),核验日改当天。
- `python3 scripts/dev/audit-ledger-scan.py` 末行从
  `16 rows (10 fixed / 5 present / 1 moot)` 变成 `16 rows (12 fixed / 3 present / 1 moot)`。

**为什么是这两条,不是五条 —— 判据只有一条,不在执行期再议**:

> 一条缺陷进 C3 ⟺ **它的「绿」能由一条不依赖模拟器 / emulator 的命令判定**。
> 机器可查的代理判据:ledger 的「层」栏不含 `swift-runner`,且「可达性」栏没写"需 live runner"。

按这条判:

| # | 层 | 进 C3? | 依据 |
|---|---|---|---|
| ② | `rust-client` | **进** | 端口解析 + URL 组装是纯函数,`cargo test` 判定 |
| ⑧ | `cli` | **进** | 端口优先级是纯函数,`cargo test` 判定 |
| ⑤a | `swift-runner` | 留下一段 | 真绿在 XCUITest 的 `tapHandler` 里,只有设备上跑得出来 |
| ⑩ | `driver`+`swift-runner` | 留下一段 | `front_app` 的唯一诚实来源在 runner 侧,主机侧填等于再造一次"看起来修了" |
| ⑨b | `cli`+`docs` | 留下一段 | 判据栏自己写着"需 live runner 才能验示例" |

**这条判据为什么承重**:§5 要求 checkpoint「半年后重跑能给出确定结论」。一个绿依赖"当时那台模拟器上
XCUITest 跑通了"的 checkpoint 半年后重跑给不出确定结论。所以设备段是**另一个 checkpoint**,不是本段
少做的部分 —— 留下的三条**不是范围问题,是排期问题**:它们是缺陷,不需要用户拍板去留,只需要用户
安排哪一段起设备(见「完成后动作」第 2 条)。

---

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix

git status --short                        # 期望:只有 `?? docs/plan-hot.md`(本文件)
pgrep -fl 'emulator|xcodebuild|gradle'    # 期望:空。本段不起设备,但 cargo 要跟别人抢核
python3 scripts/dev/audit-ledger-scan.py  # 期望末行:
#   audit-ledger-scan: clean — 16 rows (10 fixed / 5 present / 1 moot), 16 citations re-evaluated
```

末行那串是本段的**起点快照**;验收里的那串是终点。两串一起把"改了哪几行"变成机器可判的。

**热化时已完成的本机探测(下面按它写,不按 ledger 的文字写)**:

- **机器负载**:`uptime` load 6.22 / 8.60 / 9.03;三个别的会话在跑 cargo(`stables/mailrs`、
  `goliajp/kevy`、远端 mini 上的 `torajs`)。**没有** emulator / 模拟器在跑,也没有 runner batch。
  本段只编译 `smix-runner-client` 与 `smix-cli` 两个 crate,可接受;**进 S1 前重跑一次 pgrep**。
- **⑧ 的 ledger 文字是错的**:`smix run` 自 2026-07-19 的 `978ff7624` 起就走
  `flag/env → lookup_registered → 22087`(`crates/smix-cli/src/main.rs:1510-1516`),`--runner-port`
  带 `env = "SMIX_RUNNER_PORT"`(`main.rs:299-301`)。C1 只读到 `act.rs:34` 的 `runner_port_from_env`
  就下了结论,没走到 `Cmd::Run` 分支。**那条链今天还没有任何测试盯着** —— 这正是本段要补的。
  `docs/ai-guide/05-cli.md:304` 的 "Environment-variable precedence"(flag → env → registry → default)
  写的就是这条链。
- **⑤a 的 ledger 文字也是错的**:`tapOn: {id}` 根本不 POST `/tap`。默认路径是
  `SimctlDriver::tap`(`crates/smix-driver/src/lib.rs:364`)主机侧解树 + `/tap-at-norm-coord`;
  Swift SDK `App.tap`(`swift-bridge/Sources/SmixSDK/App.swift:42`)走 `/tap-by-id`。唯一 POST `/tap`
  的是 `tap_with_mode(DaemonProxySynthesize)`,而它上 wire 前被
  `require_plain_text_selector`(`crates/smix-driver/src/lib.rs:1023`)挡下并给出明确 DriverError。
  所以 ⑤a 不是"400 烧 5s",是**能力缺口**:`dispatch: daemonProxy` 只吃纯文本选择器,RN Pressable
  常见的 `testID`(id 选择器)用不上这条路径。
- **⑤a 体量(冷计划「已知风险」第二条要的估计)**:要动 4 处 ——
  ① `TapRoute.TapRequest.Selector` 从 `text: String` 变成能表达 id/label/role 的形状,
  连带 `TapRoute.notFound(selector:)` 的 JSON 输出与 `crates/smix-runner-wire/tests/tap_route_shape.rs`
  钉的 wire 契约;② `SmixRunnerUITests.swift:1294` 的 `tapHandler` 谓词
  (`label == %@ OR identifier == %@`)要按选择器种类分流;③ 同文件的 `firstSeeThroughMatch(app:text:)`
  同样是 text 签名;④ Rust 侧 `require_plain_text_selector` 放宽。
  外加 **v2.md:362 的原文说的是 `/tap` `/double-tap` `/long-press` 三条路由同病**
  (`FindRoute.decode` 也是同一形状),所以真做要么三条一起动、要么明确只动 `/tap` 并写清为什么。
  **加上它的绿只在设备上跑 XCUITest 才成立** —— 超出一个无设备 checkpoint。
- **⑩ 的承诺点不在 clap help**:`main.rs:203-204` 的 help 只说 "the visible interactive elements
  aggregated from the current a11y tree",属实。承诺在
  `crates/smix-screen/src/lib.rs:89-94` 的字段文档("Bundle id of the frontmost app at capture time" /
  "Wall-clock capture timestamp")与 `crates/smix-cli/src/act.rs:236-238` 的 rustdoc
  ("title / interactive elements / status bar");两个构造点 `crates/smix-driver/src/lib.rs:137`
  与 `crates/smix-sdk/src/lib.rs:1497` 都填空。**树的 wire 上没有 bundle 字段** —— `front_app`
  的诚实来源只能来自 runner 侧。
- **② 的收窄结论(C1 留的"待收窄"到此为止)**:iOS 直连**是设计**,不是缺陷 ——
  `docs/v2.md:365`(07-19 决策日志)原文已写明"iOS 直连 app 内 bridge,模拟器共享宿主 loopback",
  Android 半改走 `/webview-eval` 代理(`crates/smix-driver/src/android.rs:605-611`)。
  真缺陷只剩**端口不可配**一半:`crates/smix-runner-client/src/lib.rs:1330` 的字面量,
  连注释都自陈 "Bridge port is fixed at 28080 today"。全仓无任何 doc 让用户改这个端口。
  **本段修这一半,不需要用户拍板** —— 它不改任何范围承诺,只是把写死的常量变成可覆盖的默认值。

---

## 步骤(线性,无分叉)

### S1. ② —— webview bridge 端口从字面量变成可覆盖的默认值

**红(写测试)**

- 文件:`crates/smix-runner-client/tests/wire.rs`(追加,不新建文件 —— bridge URL 就是 wire 契约)
- 4 个断言,函数名固定(验收命令按名字数数):
  - `webview_bridge_port_defaults_to_28080` —— `webview_bridge_port_from(None) == 28080`
  - `webview_bridge_port_reads_override` —— `webview_bridge_port_from(Some("29999")) == 29999`
  - `webview_bridge_port_ignores_unparseable` —— `webview_bridge_port_from(Some("nonsense")) == 28080`
  - `webview_bridge_url_uses_given_port` —— `webview_bridge_url(29999) == "http://127.0.0.1:29999/eval"`
- 期望红:`cargo test -p smix-runner-client webview_bridge` **编译失败**(两个函数都不存在)。
  这是新 API 的标准红相,把它当红收下,并把失败输出记进 S1 记账段。
- **测试吃 `Option<&str>`,不 mutate 进程 env** —— v1.0.26 的 env-mutation flake 就是这么来的
  (记账见 MEMORY 的 v1.0.26 条)。env 只在调用点读一次,解析是纯函数。

**绿(实现)**

- 文件:`crates/smix-runner-client/src/lib.rs`
- API:
  ```rust
  pub const DEFAULT_WEBVIEW_BRIDGE_PORT: u16 = 28080;
  pub fn webview_bridge_port_from(raw: Option<&str>) -> u16;
  pub fn webview_bridge_url(port: u16) -> String;
  pub fn with_webview_bridge_port(mut self, port: u16) -> Self;   // HttpRunnerClient builder
  ```
- 关键点:
  1. `HttpRunnerClient` 加字段 `webview_bridge_port: u16`,在 `new()` / `with_base()` 里用
     `webview_bridge_port_from(std::env::var("SMIX_WEBVIEW_BRIDGE_PORT").ok().as_deref())` 求值;
     `webview_eval` 用 `webview_bridge_url(self.webview_bridge_port)` 换掉 1330 行的字面量。
  2. **host 不动**。`127.0.0.1` 是"模拟器共享宿主 loopback"的设计(v2.md:365),不是写死的疏漏;
     把它一并参数化会把一条已裁决的设计改成可配置面,超出本行范围(§8.1)。
  3. 无法解析时回落默认值 + 与 `act.rs` 的 `runner_port_from_env` 同形 —— 这是**沿用树里既有约定**,
     不是新造兜底;第三个断言把这个行为钉成契约而不是意外。
- 文件:`docs/ai-guide/05-cli.md` 第 26 行那张 "Environment variables consumed by `smix run`" 表
  加一行 `SMIX_WEBVIEW_BRIDGE_PORT` / `28080` / iOS app 内 `SmixWebViewBridge` 的端口。
  它与 `SMIX_RUNNER_PORT` 同族(基础设施端口),**不是**那四个被 yaml switch 取代的行为开关,
  所以**不加** `warn_if_env` 提示。

**重构**

- `webview_eval` 的 rustdoc 里 "Bridge port is fixed at 28080 today" 改成属实的说法。
  注释是断言,代码改了断言就得改(`comments_are_claims_code_is_truth`)。

**ledger 行改动(闸门强制,漏改则 C3 验收第 2 条红)**

- ② 状态 `present` → `fixed`
- 判据从缺陷代码改钉**修复代码**:`at crates/smix-runner-client/src/lib.rs:<行> "SMIX_WEBVIEW_BRIDGE_PORT"`
  —— 行号以改完后实测为准(闸门失配时会打印真实行号);**必须钉在读 env 的那行代码上,不能钉注释行**
  (闸门检查 6)
- 「层」栏改 `—`(`fixed` 行必须是 `—`,闸门强制)
- 「可达性 / 理由」改写为:Android 半走 `/webview-eval` 代理;iOS 直连是设计(v2.md 07-19 原文);
  本段修的是端口写死那一半,默认 28080 可由 `SMIX_WEBVIEW_BRIDGE_PORT` / builder 覆盖
- 核验日改当天
- **栏内不许出现裸 `|`**(闸门按 7 格切行,一个管道会把整行从检查里抹掉)

---

### S2. ⑧ —— `smix run` 的端口优先级链变成可测的纯函数,并把 C1 记反的那行改真

**这一条不是 C3 修的,是 C1 记错的。** 代码 07-19 就对了,错的是 ledger。本段做两件事:
把那条链变成会红的断言(它至今无人盯着),把行改真。

**红(写测试)**

- 文件:`crates/smix-cli/src/main.rs` 的 `#[cfg(test)] mod tests`(2710 行起)
- 4 个断言,函数名固定:
  - `run_port_flag_wins` —— `run_port(Some(22099), || Some(23000)) == 22099`
  - `run_port_falls_back_to_registry` —— `run_port(None, || Some(22099)) == 22099`
  - `run_port_defaults_to_22087` —— `run_port(None, || None) == 22087`
  - `run_port_skips_registry_lookup_when_flag_present` —— 闭包用 `Cell<bool>` 记是否被调用,
    有 flag 时必须**没被调用**
- 期望红:`cargo test -p smix-cli run_port` 编译失败(`run_port` 不存在)。
- 第四条承重:现在的写法是 `flag.or_else(...)`,惰性是**行为**的一部分(有 flag 时不该去读 sims.json)。
  没有它,一次"顺手改成 eager"的重构会悄悄多读一次磁盘且无人知道。

**绿(实现)**

- 文件:`crates/smix-cli/src/main.rs`
- API:
  ```rust
  fn run_port(flag: Option<u16>, registered: impl FnOnce() -> Option<u16>) -> u16 {
      flag.or_else(registered).unwrap_or(22087)
  }
  ```
- 调用点(现 1510-1516 行)改成
  `run_port(runner_port, || device.as_deref().and_then(lookup_registered).and_then(|s| s.runner_port))`
- 关键点:**行为逐字节不变**,只是把它挪进一个能被断言的位置。现有那段注释说的是真的,保留。

**重构**

- 不动 `crates/smix-cli/src/act.rs` 的 `runner_port_from_env` —— 见下面的「本段新发现」。

**本段新发现(不进 ledger,去处交用户)**

`smix tap` / `find` / `wait-for` / `fill` / `describe` / `system-popups` / `authoring …` 这些 shell-out
子命令走 `act::runner_port_from_env`(`main.rs:1296-1378`、`1703`),**只认 `--port` 与 env,不查注册表**,
而且它们没有 `--device` 可查 —— 与 `smix run` 的链、以及 `docs/ai-guide/05-cli.md:304` 写的
"flag → env → registry → default" 都不一致。**本段不改**:它不在 v2.md:362 那条待办的 14 条里,
ledger 的行集合由那条待办定义,擅自加行会让闸门的覆盖检查失去锚(C1 S1 的同一条规则)。
记进 v2.md 决策日志,标明未修、未进 ledger、去处待定。

**ledger 行改动**

- ⑧ 状态 `present` → `fixed`
- 判据:`at crates/smix-cli/src/main.rs:<行> "fn run_port"`
- 「层」栏改 `—`
- 「可达性 / 理由」改写为:`smix run` 自 2026-07-19 的 `978ff7624` 起就是
  flag/env → 注册表 `runnerPort` → 22087;C1 只读了 `act.rs` 的 `runner_port_from_env`,没走到
  `Cmd::Run` 分支,记反了;本段补上会红的单元测试。**commit 号写进栏里** —— 让"不是本段修的"
  这个事实钉在表里,而不是只活在 plan-history。
- 核验日改当天

---

### S3. ⑤a / ⑩ 两行的可达性改真,加 v2.md 决策日志

**这一步不写代码,也不编红相。** 理由与 C1 的 S1 相同:纯核实的产出是事实,给它编一个假的红相
只会让"红"这个信号贬值。它的验证是机器可判的(见验收第 3 条)。

**⑤a 行**(状态 `present` 不动、判据 `at swift-bridge/Sources/SmixRunnerCore/TapRoute.swift:80
"selector.text not string"` 不动、层 `swift-runner` 不动):

「可达性」栏按前置条件里的实测改写,必须点到这三跳:
默认 `tapOn` 走 `SimctlDriver::tap` 主机侧解树 + `/tap-at-norm-coord`,不 POST `/tap`;
Swift SDK `App.tap` 走 `/tap-by-id`;唯一 POST `/tap` 的 `tap_with_mode(DaemonProxySynthesize)`
被 `require_plain_text_selector` 在上 wire 前挡下。**所以缺口是能力**:`dispatch: daemonProxy`
只吃纯文本选择器,`testID` 那类 id 选择器用不上这条路径。核验日改当天。

**⑩ 行**(状态 / 判据 / 层都不动):

「可达性」栏改写为:承诺不在 clap help(help 说的是属实的),在
`crates/smix-screen/src/lib.rs:89-94` 的字段文档与 `crates/smix-cli/src/act.rs:236` 的 rustdoc;
两个构造点 `crates/smix-driver/src/lib.rs:137` 与 `crates/smix-sdk/src/lib.rs:1497` 都填空;
`smix describe --json` 把这三个空字段直接发给消费方;`front_app` 的诚实来源只在 runner 侧
(树的 wire 上没有 bundle 字段),所以修它必须动 `swift-runner`。核验日改当天。

**v2.md §10 决策日志追加**(不动 07-19 那条历史原文,只在日志末尾追加):

1. C3 修了 ② 与 ⑧ 的哪一半,各自的断言在哪;
2. **C1 把 ⑧ 和 ⑤a 的可达性写错了** —— 错在哪、现在的说法是什么。这条必须写,理由是本段的题眼:
   一张自称准确的表,第一次失真是三天,第二次是一天;
3. S2 的「本段新发现」(act 子命令不读注册表),标明未修、未进 ledger;
4. ⑤a / ⑩ / ⑨b 三条留给需设备的一段,附 ⑤a 的体量估计。

**追加时注意**:正文若写到会被 sim-guard / adb-guard 拦的命令形状,heredoc 正文会被 guard 当命令读
(07-21 已发生过一次)—— 改措辞或改用编辑工具写入,**不改 guard**。

---

## Checkpoint C3 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

# 1. 两条修复各自有会红的断言,现在绿(数数,不看退出码 —— 过滤器匹配 0 个测试也是 exit 0)
cargo test -p smix-runner-client webview_bridge 2>&1 | grep -q 'test result: ok. 4 passed'
cargo test -p smix-cli run_port 2>&1 | grep -q 'test result: ok. 4 passed'

# 2. 记账翻转到位,且 16 条判据全部重新求值通过
python3 scripts/dev/audit-ledger-scan.py

# 3. 三处改真的机器判据
grep -q 'require_plain_text_selector' docs/audit-ledger.md
grep -q 'smix-screen' docs/audit-ledger.md
grep -q '978ff7624' docs/audit-ledger.md
grep -q 'SMIX_WEBVIEW_BRIDGE_PORT' docs/ai-guide/05-cli.md

# 4. 既有闸门没被本段破坏
python3 scripts/dev/hygiene-scan.py
python3 scripts/dev/workflow-scan.py
bash scripts/dev/preflight.sh
```

期望:

- 第 1 条:两行 `grep -q` 都 exit 0(各 4 个测试全绿)。**先看到红再看到绿** —— S1 / S2 的红相
  输出必须已记进各自的记账段,验收只复跑绿。
- 第 2 条 exit 0,末行**逐字符等于**:
  `audit-ledger-scan: clean — 16 rows (12 fixed / 3 present / 1 moot), 16 citations re-evaluated`
  (起点是 `10 fixed / 5 present`;这一串变化本身就是"改了哪两行"的判据)
- 第 3 条:4 条 `grep -q` 全 exit 0。它们分别钉住 ⑤a 的真拦截点、⑩ 的真承诺点、⑧ 的真修复出处、
  ② 的用户可见开关。**闸门看不出"状态词与判据语义是否相符"**(它的 docstring 明写),
  所以这四条是人给出的、可机器复查的补充。
- 第 4 条:`hygiene-scan` / `workflow-scan` exit 0,preflight 末行 `preflight: clean`。

**本段所有验收命令都不依赖设备** —— 这是「目标 checkpoint」那条判据的实证形态。

---

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.3-c3-hot.md`
2. 在 `docs/plan-cold/v2.3-release-truth.md` 的「Checkpoint 概要列表」补一行:
   **C5:需设备段 —— ⑤a(`/tap` 非 text 选择器,连带 `/double-tap` `/long-press`)、
   ⑩(`describe()` 三个空字段)、⑨b(`authoring suggest` 帮助示例复验)**,并把 C3 那行的措辞
   从"逐个修"改成属实的说法(C3 修的是主机侧可机判的两条)。
   **同时在 v2.md 决策日志写明为什么拆** —— 不允许只改冷计划不留理由。
   拆的是排期不是范围:这三条是缺陷,不需要用户拍板去留,只需要用户安排哪一段起设备。
3. 按 §7 收尾 task 状态(S1 / S2 / S3 三个 task 全 `completed`)。
4. **不自行热化 C4**(§6)。把三件事报给用户:C3 的两条修复、C1 两处记错的更正、
   S2 的新发现与 C5 的排期请求。由用户说"开始 C4"或"先做 C5"。

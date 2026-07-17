# plan-hot — v2 到 C6：让 wire 与 VERB_TABLE 说真话

## 目标 checkpoint

C6：break #2（wire schema-version 协商）与 break #6（VERB_TABLE freeze v2）落地；三个已发布 SDK 打的是 runner 真的服务的路由；`smix migrate` 的输出**保证能被 parser 收**，并给出 summary 而非一句 "done"。

通过后世界：**三张互相矛盾的表（SDK 的路由表 / VERB_TABLE / parser 的 dispatch）合成一张，且每一张的偏离都由 gate 机械挡住** —— 而不是靠谁手工枚举一遍（手工枚举已在本 cycle 失败五次）。

其余四项破坏性变更（#1 #3 #4 #5）+ `SimctlError` 改名 = C7。

## 前置条件

```bash
git branch --show-current                                 # 期望 feature/v2.0
git log --oneline -1                                      # 期望 C5 已归档（f5f9fecca 或其后）
pgrep -fl "runner.ts|smix run|supervise"                  # 期望空（in-house batch 不活动）
pgrep -fl "gradle|mobilegate|emulator"                    # 期望空（S3 要动 gradle SDK）
python3 scripts/dev/hygiene-scan.py --noise-only          # 期望 exit 0
bash scripts/dev/fence-check.sh                           # 期望 exit 0
cargo test --workspace 2>&1 | grep -c "^test result: ok"  # 期望 129（本段基线，实测）
cargo clippy --workspace --all-targets 2>&1 | grep -cE "^(error|warning): "  # 期望 0
```

## 已确证的起点（本次热化实测，非转述）

**六项破坏性变更的权威表述**：`docs/v2.md:29-38` 的表 + `.claude/design/v2.0/architecture.html` §03。按其原序，不重排：#1 sessions 强制 · #2 wire schema-version 协商 · #3 `SMIX_*` 折进 config · #4 `Modifier(s)` + 双 `open_url` 合并 · #5 `smix-recorder-ir` → `smix-authoring-ir` · #6 VERB_TABLE freeze v2。

**① 打假 wire 的 SDK 是三个，不是两个。** `docs/v2.md:99-104` 记的是 npm + Maven 两个。实测（路由字符串 vs 两个 runner 注册表的并集 41 = iOS 37 ∪ Android 18）：

| SDK | 引用路由 | 未被服务 | 载体 |
|---|---|---|---|
| `@goliapkg/smix`（npm）| 16 | **13** | `npm/smix-rn/src/HttpRunner.ts` |
| `jp.golia.smix:smix-sdk`（Maven）| 18 | **13** | `android-runner/sdk/src/main/kotlin/dev/smix/sdk/HttpSmixSimRuntime.kt` |
| **SmixSDK（Swift Package，`git tag swift-vX.Y.Z`）** | 18 | **13** | `swift-bridge/Sources/SmixSDK/HttpSmixSimRuntime.swift` |
| `smix-runner-client`（Rust）| 36 | **0** | 干净，是 wire 的参考实现 |

三个 SDK 的 13 个坏路由**逐字相同**（`/a11y/snapshot` `/input/tap` `/input/swipe` `/input/press-key` `/input/send-string` `/input/tap-normalized` `/sim/launch` `/sim/launch-fresh` `/sim/launch-from-path` `/sim/open-url` `/sim/screenshot` `/sim/system-popups` `/sim/terminate`）—— 互为 mirror 移植，继承同一套虚构。Swift SDK 是 `Package.swift:17` 的 `.library(name: "SmixSDK")`，**是发布物**。
**遗漏本身就是 C6 要修的那个病**：两个 SDK 这个数字来自手工枚举，而手工枚举漏了第三个。本 cycle 第五次同型（benches/ → SmixRunnerUITests/ → Tests/ → npm/ → 现在 Swift SDK）。

**② codemod 现在就在产出 parser 收不了的 yaml —— 而且坏的是它的主业。** 实测（`smix migrate` → `smix run --check`，**带 migrate 前的对照**，只认「输入 rc=0 而输出 rc≠0」）：

```
doubleTapOn: Item   → doubleTap: Item    输入 rc=0 → 输出 rc=2   ← codemod 把有效 flow 改坏
longPressOn: Item   → longPress: Item    输入 rc=0 → 输出 rc=2   ← 同上
back:               → pressKey:          键 `back` 被丢掉（应为 `pressKey: back`，见 verb-parity.md:67）
```

`doubleTapOn` / `longPressOn` 是 maestro 的正典拼写 —— **codemod 存在的全部理由就是翻译它们**。

**③ 那 11 行不同质，「逐个 wire 或 drop」对其中一行是错的。** VERB_TABLE 服务**两个**消费者：parser（smix yaml 收什么）与 codemod（`default_rules()` 直接由 `maestro_name → smix_name` 生成，`smix-migrate/src/lib.rs:623-632`）。按 `maestro_name` vs `smix_name` 分：

- **`back`（maestro=back, smix=pressKey）是合法的 rename 行**。parser 拒 `back:` 是**对的**（codemod 会把它改写掉）。它的缺陷只在 arg transform 丢了键，不在这一行该不该存在。
- **其余 10 行是 identity 行（maestro_name == smix_name）**，表在承诺一个 smix 收不了的名字。

**④ identity 行不是「缺一个 verb」，是在遮蔽已经能用的 alias。** `parser.rs:2276-2291` 的 `normalize_verb_name`：先 `find_by_maestro` 命中即原样返回。于是 identity 行 `doubleTap` 让 `doubleTap:` 走 fast path 返回 `doubleTap` → dispatch 无此臂 → 失败；**删掉这一行**，`find_by_maestro` miss → `find_by_smix("doubleTap")` 命中 `("doubleTapOn","doubleTap")` → 返回 `doubleTapOn` → dispatch 成功。**删行 = 修好 verb**，同时修好 ②。

**⑤ `toggleAirplaneMode` 是纯幽灵，且不可能达成 parity。** 全仓（`.rs`/`.kt`/`.swift`/`.ts`，大小写不敏感）只出现在：`smix-verbs/src/lib.rs:339-340`（表）、`smix-migrate/src/lib.rs:660`（一段 dead code）、`verb_table_gate.rs:101`（allowlist）。无 parser、无 Step、无 device call。Android 原语实测可用（`v2.md:134`），但 **iOS 侧无等价原语**（`simctl status_bar` 只改状态栏外观），故它无法达到「iOS + Android 全 parity」这条发布门槛。`verb-parity.md:107` 已记 ❌/❌。

**⑥ `smix-migrate` 藏着 VERB_TABLE 的第二份真源。** `smix-migrate/src/lib.rs:637-669` 有 `#[allow(dead_code)] fn is_known_verb`，一份硬编码 verb 清单，与 `smix_verbs::is_known_verb`（真源，`lib.rs:514` 用的是它）同名并存且**已漂移**（含 `assertWithAI` / `readClipboard` / `assertFalse` / `screenshot` / `swipeOnce` 等）。break #6 的标题是「单一真源」，这一份必须删。

**⑦ 计划前提两处已不成立**（见文末「与冷计划不符之处」）。

**⑧ 其余 break 的落点已核**：`SimctlError` 定义在 `smix-simctl/src/lib.rs:43`，全仓 168 处 / 3 个 crate（smix-simctl 4 文件 · smix-sdk 4 · smix-cli 2）。`Modifier` / `Modifiers` 的重复**只在 Kotlin 与 Swift SDK**（`Modifier.kt` 9-case sealed interface + `Modifiers.kt` data class；Swift 同构）—— Rust 只有 `Modifiers`（`smix-selector/src/lib.rs:220`），TS 无此文件。`/health` 现在的 body 是 `{"ok":true,"runnerVersion":…}`（`HealthRoute.swift`），**无 `wireSchema` 字段**。

## 步骤（线性，无分叉）

> S3（四个改名 break + `SimctlError` 改名）已移出本段，成为 C7 —— 见 v2.md 决策日志 2026-07-17「拍板·拆 C6」。

### S1. 让 wire 说真话：路由一致性 gate + 三个 SDK 接回真路由 + wire v2 协商（break #2）

**红（写测试）**

- 文件：`scripts/dev/route-conformance.py`（新）
- 断言：被 git 跟踪的源码里每一个「路由形状的字符串字面量」，都必须属于两个 runner 注册表的并集。当前应**红**，报 39 处（npm 13 + Kotlin 13 + Swift 13）。
- **形状照抄 `hygiene-scan.py`，不照抄它的白名单**：文件集取自 `git ls-files`（默认全扫），跳过必须在 `EXCLUSIONS` 里写理由，**且每条 exclusion 的剩余命中数每次运行都打印**。理由见起点 ①：白名单会把这个缺陷再定义掉一次，而这正是它躲过五轮的原因。
- 两个 runner 的注册表由脚本**读源码算出**，不手写：iOS 取 `swift-bridge/Sources/SmixRunnerCore/SmixRunnerServer.swift` 的 `appendRoute("<VERB> /path")`；Android 取 `android-runner/app/src/` 的路由字面量。
- **两个 runner 服务不同的路由集是设计，不是缺陷**：iOS runner 侧解析 selector（`/find` `/tap` `/fill`），Android 宿主侧解析、按坐标动作（`/tap-at-norm-coord` `/input-text`）。gate 判的是**并集包含**，不是两表相等。
- 文件：`crates/smix-runner-wire/tests/schema_negotiation.rs`（新）+ `swift-bridge/Tests/SmixRunnerCoreTests/HealthWireSchemaTests.swift`（新）
- 断言：`/health` body 含 `wireSchema: { negotiated, supports }`；无公共版本时客户端得到**响亮的错误 + upgrade hint**，不是 decode error。当前红（字段不存在）。

**绿（实现）**

- 文件：`npm/smix-rn/src/HttpRunner.ts` · `android-runner/sdk/src/main/kotlin/dev/smix/sdk/HttpSmixSimRuntime.kt` · `swift-bridge/Sources/SmixSDK/HttpSmixSimRuntime.swift`
- API：每个方法接回**真实路由**，逐一对照 `crates/smix-runner-client/src/lib.rs` —— **Rust client 是 wire 的参考实现（36/36 被服务），不是再发明一套**。`snapshot()` → `GET /tree`；`launch()` → `POST /session/launch-app`；`terminate()` → `POST /session/terminate-app`；`swipe()` → `POST /swipe-once`；`tapNormalized()` → `POST /tap-at-norm-coord`。
- `screenshot()`：**删掉这个方法**，三个 SDK 都删。没有 `/sim/screenshot`，也不为它造一个 —— 截图走带外 simctl / `smix-screen`（`architecture.html` §04 明记）。这是 break，入 deprecation 表。
- 文件：`crates/smix-runner-wire/src/lib.rs` + `swift-bridge/Sources/SmixRunnerCore/HealthRoute.swift` + `crates/smix-runner-client/src/lib.rs`
- 协商算法照 `architecture.html` §04：client 首次调用送 supported 列表 → runner 取最高公共版本 → runner 在 `GET /health` 回 `negotiated` → 双方对该 schema 定型 → 无公共版本则响亮失败 + `runner too old — smix runner install --force`。
- 关键点：`HealthRoute.body()` 的 legacy 字节序列 `{"ok":true}` 是**故意稳定**的（有工具 jq 死解它），扩展字段只能追加；wireSchema 加在 `bodyDetail` 一侧。
- 关键点：三个 SDK 的测试全部注入 mock（`MockHttpRunnerClient` / mock runtime），**它们验证的是 SDK 跟自己说话**。所以真正的证据是 gate，不是这些测试转绿。

**重构**

- 删掉 `HttpRunner.ts:4-12` 的 KNOWN DEFECT 注释块 —— 缺陷修完，注释就该走，否则它就是下一条陈旧注释（本 cycle 已因此撤回 4 次）。

### S2. 让 VERB_TABLE 说真话：freeze v2（break #6）+ codemod 往返 gate + summary 契约

**红（写测试）**

- 文件：`crates/smix-adapter-maestro/tests/verb_table_gate.rs`
- 断言：把反向 gate**改问对的问题**。它现在问「parser 是否 dispatch `maestro_name`」，而契约是「codemod 的产物能不能被收」，即「parser 是否收 `smix_name`（经 normalize）」。改后 `back`（smix=`pressKey`）**自动转绿并离开 allowlist**，10 个 identity 行仍红。
- 文件：`crates/smix-migrate/tests/codemod_roundtrip.rs`（新）
- 断言：对 VERB_TABLE 的**每一个** `maestro_name` 造最小 flow → `Migrator::migrate` → `smix_adapter_maestro` parse，必须收。当前红（实测 `doubleTapOn: Item` / `longPressOn: Item` 由 rc=0 变 rc=2）。**这条 gate 是 C1 那条的正确形态** —— C1 验成员关系，它验产物可用。
- 断言：`MigrateReport` 产出 summary（rewritten / warned / manual-review 计数），且**任何无法改写的 verb 都变成具名 warning + 建议替代**，不静默丢弃。

**绿（实现）**

- 文件：`crates/smix-verbs/src/lib.rs`
- 动作：按下表处置 10 个 identity 行。**判据单一**：一行的存在理由是「它的 `smix_name` 是 parser 收的 verb」；identity 行不满足，且其能力另有可达形式 → 删行。**删行不删能力**，逐行记进 `docs/v2.md` 决策日志 + `docs/ai-guide/verb-parity.md`。

| 行 | category | 处置 | 依据（实测） |
|---|---|---|---|
| `doubleTap` | SmixNative | **删行** → `doubleTap:` 随即可用 | 删后 `find_by_smix` 路由到 `doubleTapOn`；同时修好 codemod |
| `longPress` | SmixNative | **删行** → `longPress:` 随即可用 | 同上 |
| `ocrText` | SmixNative | **删行** —— 是 selector 字段不是 verb | `tapOn: {ocrText: "Login"}` rc=0；`parser.rs:280` |
| `anchorRelative` | SmixNative | **删行** —— 是 selector 字段（`anchored` 的 alias）| `parser.rs:264,728` |
| `findTextByOcr` | SmixNative | **删行** —— 是路由不是 yaml verb，能力经 `ocrText` 选择器可达 | `/find-text-by-ocr` |
| `tapAtCoord` | SmixNative | **删行** —— yaml 形式是 `tapOn: {point}` | `tapOn: {point: "50%,50%"}` rc=0 |
| `tapByCoord` | Tap | **删行** —— 同上 | 同上 |
| `swipeAtCoord` | SmixNative | **删行** —— yaml 形式是 `swipe` 的 point 形 | §9#3 授权的是 SDK escape hatch，不是 yaml verb |
| `tapById` | SmixNative | **删行** —— yaml 面用 `tapOn: {id}`；fast path 留作 SDK 方法 | `tapOn: {id: "btn"}` rc=0。同一件事留两个 yaml 拼写，正是 break #4 在别处要合并的那种 dupe |
| `toggleAirplaneMode` | Device | **删行 + 记 re-tier** | iOS 无等价原语 → 达不到 ✅/✅ 发布门槛；`verb-parity.md:107` 已 ❌/❌ |

- 文件：`crates/smix-migrate/src/lib.rs`
- 动作：(a) `back` 的 arg transform 补上丢掉的键 —— 产出 `pressKey: back`（`verb-parity.md:67`）；(b) **删掉 dead `is_known_verb` + `is_ignored_key`**（`lib.rs:637-676`）—— 它们是 VERB_TABLE 的第二份真源且已漂移，留着与 break #6 的标题直接矛盾；(c) `MigrateReport` 补 summary 输出，CLI 打印，**永不只打一句 "done"**。
- 关键点：删 identity 行**不是**能力倒退 —— 那些行从未在 yaml 里工作过（实测 `doubleTap: Item` / `tapAtCoord:` / `toggleAirplaneMode:` 全 rc=2）。删掉之后其中两行反而开始工作。

**重构**

- `TABLE_ROWS_THE_PARSER_LACKS` 清空并连同 `the_known_gap_list_does_not_outlive_the_gaps` 一起删 —— 债还完，清单本身就该走。反向 gate 改问 `smix_name` 之后，这个 allowlist 没有存在理由。

## Checkpoint C6 验收

```bash
# 1. 路由一致性：每个 SDK 引用的路由 ⊆ 两个 runner 注册表的并集
python3 scripts/dev/route-conformance.py ; echo "rc=$?"
# 2. codemod 往返：VERB_TABLE 每个 maestro_name 经 migrate 后 parser 必须收
cargo test -p smix-migrate --test codemod_roundtrip 2>&1 | grep "^test result:"
# 3. VERB_TABLE 双向 gate（allowlist 已删）
cargo test -p smix-adapter-maestro --test verb_table_gate 2>&1 | grep "^test result:"
grep -c "TABLE_ROWS_THE_PARSER_LACKS" crates/smix-adapter-maestro/tests/verb_table_gate.rs
# 4. wire v2 协商
cargo test -p smix-runner-wire 2>&1 | grep "^test result:"
# 5. 三个 SDK 真的接回了真路由（量代码，不量排版）
grep -rn "sim/screenshot" npm/smix-rn/src android-runner/sdk/src/main swift-bridge/Sources/SmixSDK | wc -l
# 6. 无回归
cargo test --workspace 2>&1 | grep -c "^test result: ok"
cargo clippy --workspace --all-targets 2>&1 | grep -cE "^(error|warning): "
python3 scripts/dev/hygiene-scan.py --noise-only ; echo "rc=$?"
# swift：读 XCTest 的 "Executed N tests" 行。不要读 "Test run with N tests ... passed" ——
# 那是 swift-testing harness 的行，实测报的是 `0 tests`，而真正的 360 个在另一行。
( cd swift-bridge && swift test 2>&1 | grep "Executed .* tests" | tail -1 )
# android：BUILD SUCCESSFUL 不是证据 —— 实测 `./gradlew test` 打了 BUILD SUCCESSFUL +
# "54 up-to-date" 却一个测试都没跑。数 XML 报告里的真数字。
( cd android-runner && ./gradlew test --console=plain >/dev/null 2>&1
  find . -name "TEST-*.xml" | xargs grep -ho 'tests="[0-9]*"'    | grep -o '[0-9]*' | paste -sd+ - | bc
  find . -name "TEST-*.xml" | xargs grep -ho 'failures="[0-9]*"' | grep -o '[0-9]*' | paste -sd+ - | bc )
```

期望，逐条：

1. `rc=0`，且脚本打印每条 exclusion 的剩余命中数（0 violation）。
2. `test result: ok`，`0 failed`。
3. `test result: ok`；`grep -c` 得 **0**（allowlist 连同它的守卫测试一起删）。
4. `test result: ok`。
5. `sim/screenshot` 计数 **0**（三个 SDK 都不再打这条不存在的路由）。
6. `test result: ok` 计数 **≥129**（本段基线实测 129，不回退）；clippy 计数 **0**；hygiene `rc=0`；swift 那行读作 `Executed 360 tests, with 2 tests skipped and 0 failures`（**≥360 且 0 failures**，本段实测 360）；android 两个数字为 **≥134** 与 **0**（本段实测 134 / 0）。

**仪器纪律**（本 cycle 反复吃亏；下列每条都是本次热化**亲手复现**过的，不是转述）：
- 测退出码**不接管道** —— `cmd | head; echo $?` 量的是 `head`（`v2.md:137` 的原案）。
- `--include='*.rs'` 必须**带引号** —— 不带引号 zsh 直接报 `no matches found`，整条 grep 不执行（本次热化第一次查 `SimctlError` 就踩了）。
- **`swift test` 会同时给出两个"通过"**：`✔ Test run with 0 tests in 0 suites passed`（swift-testing harness，真的是 0 个）与 `Executed 360 tests … 0 failures`（XCTest，真身）。grep 错行就是拿 0 个测试的绿当 360 个测试的绿。
- **`./gradlew test` 的 `BUILD SUCCESSFUL` 可以在零测试执行的情况下打印** —— 实测紧跟着的是 `54 actionable tasks: 54 up-to-date`。所以数 XML，不数横幅。
- 第 5 组量的是**代码里的字符串计数**，不是文档排版（`v2.md:78` 的教训：别把命令写成量自己的排版）。

**未被本 checkpoint 覆盖的**：三个 SDK 接回真路由后**没有任何真设备证据** —— 它们的测试全注入 mock，gate 只证明「路由字符串对得上注册表」，不证明「请求真的被正确应答」。真 sim / emulator 的四 SDK smoke 属 C8 的 ship gate（`v2.md:70` 已记 C8 要补 `xcodebuild build-for-testing`）。**按 C3/C4/C5 的同一条教训写在明处：mock 与 schema 都证明不了真设备上的事。**

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2-c6-hot.md`
2. 调 sub-agent 生成新 `plan-hot.md`（到 C7：四个改名 / 合并 break #1/#3/#4/#5 + `SimctlError` 改名），见 CLAUDE.md §6
3. C8（docs）必须承接本段记账的三条：roadmap.md:85 的 `SMIX_DEV_LOCK` 是幻影需删；roadmap.md:86 的 `Modifier`/`Modifiers` 描述需注明只在 Kotlin/Swift；`docs/v2.md` 记的「两个 SDK」**已更正为三个**（2026-07-17）。

## 与冷计划不符之处（必须先读，不要隐瞒）

1. **冷计划 C6 的风险条「wire v2 协商破坏旧 runner」预设 SDK 现在说的是一套能用的 wire。它们不是** —— 四个 SDK 里三个的 13 个路由从来没被任何 runner 服务过。在 404 的路由上协商 schema 版本是在为不存在的东西谈判。故 S1 把**路由一致性排在协商之前**，顺序与冷计划的表述相反。
2. **dossier 对 break #6 的表述已被 C1 做完了**。`architecture.html` §03 写 break #6 = 「注册 `clearUserDefaults` / `resetAppData` / `clearAppData` + 加 parser ⊆ table 测试」—— 这**正是 C1 已落地的内容**（`v2.md:149`）。C6 剩下的是**反方向**（表承诺而 parser 不认的 10 行）+ codemod 产物可用性，与 dossier 的字面描述不同。
3. **roadmap.md:85 点名要删的 `SMIX_DEV_LOCK` 在全仓不存在** —— 只出现在 roadmap 那一行本身。break #3 的 3-var 表述是错的：真实是 39 个 `SMIX_*`，其中只有 4 个属「opt-out 开关」这一类。
4. **C6 原本是本版本最大的一段** —— 8 项工作压进 3 step。**已拆**：四个改名 break 移出为 C7，原 C7/C8 顺延为 C8/C9。理由见 v2.md 决策日志（风险性质不同：本段修的是已经坏掉的东西，改名改的是能用的东西）。
# plan-hot — v2.4 到 C1:指南里印出来的每条示例,都要被喂给它真正会走的那段代码

## 目标 checkpoint

**C1**:仓库里存在一道闸门,它把 `docs/ai-guide/` 里的示例喂给**真实的解析器、真实的
`Adapter` 分发、真实的 driver 准入规则、真实的 clap 命令树**,判定每条示例会走哪条代码路径、
那条路径是否接受它;并存在一张清单 `docs/guide-executability.md`,每行的状态由闸门每次运行重新求值。

闸门建成时**当场双向对上**:

- **抓出三条已知未修的**(负对照)——`N1` 单点 verb 的端口优先级链 / `N2` `launchApp` 的
  `.MainActivity` 约定 / `N3` `04-actions.md` §Default tap 的分发断言
- **放行两条今天刚修好的**(正对照)——`P1` `tapOn: {id, dispatch: daemonProxy}` /
  `P2` `authoring suggest` 的裸字符串形态

C1 **不修任何一条缺陷**。C1 只交付「判定能力」与「这一族的名册」。

## 前置条件

```bash
git status --short
# 期望:空

grep -c 'Selector::Id { modifiers, .. } | Selector::Label { modifiers, .. }' crates/smix-driver/src/lib.rs
# 期望:1 —— P1 的修复在位(driver 准入规则接受 Id)

grep -c 'let candidates: \[Option<&str>; 5\]' crates/smix-cli/src/authoring.rs
# 期望:1 —— P2 的修复在位(裸形态搜所有可读串)

grep -c '/.MainActivity' android-runner/app/src/main/kotlin/dev/smix/runner/RunnerWire.kt
# 期望:≥1 —— N2 的缺陷仍在
```

已在本机核实(2026-07-22,只读):工作树干净;七道 python source gate 全绿
(`audit-ledger-scan: 16 rows (15 fixed / 0 present / 1 moot)`、
`scope-promise-scan: 9 promises`);`docs/plan-hot.md` 此前不存在。

---

## 本段预先定死的四个口径(执行期不得再议)

### 口径 1 — 闸门是 Rust 测试,不是 python;它住在 `crates/smix-cli/src/guide_gate.rs`

**跨语言问题的选路与理由。** 三条路里选**第三条**(闸门写成 Rust 测试),另两条各有致命处:

- **python 侧重新实现约束** —— 一份契约两个实现。冷计划的整个立论是「判定『这条示例能跑』
  必须给出**它会走的那条代码路径**」;若 python 自己写一份「哪些选择器能上 wire」,闸门判的
  就是它自己的影子,而不是 `require_runner_resolvable_selector`。**排除**。
- **Rust 出一个可被 python 调的检查入口** —— 需要一份 JSON 进出协议,而**那份协议本身无人守**;
  且它把测试脚手架塞进已发布二进制的表面。比第三条严格更差。**排除**。
- **Rust 测试** —— 决定示例能不能跑的东西全部是 Rust 纯函数/纯数据:
  `parse_flow_yaml`、`Adapter::run_step` 的 `dispatch` 分派、`require_runner_resolvable_selector`、
  `authoring::{parse_partial, matches_partial, suggest_selectors}`、`act::runner_port_from_env`、
  `main::run_port`、clap 的 `Cli::command()` 树。直接调它们 = 零重实现。
  **并且这是本仓既有惯例**:两道现存文档闸门(`documented_flags_exist.rs`、`guide_yaml_parses.rs`)
  正是 `include_str!` 指南 + Rust 测试,理由同一条。**选定**。

**为什么住在 `crates/smix-cli/src/` 而不是 `tests/`。** 五条判据里有三条的裁决函数是
**smix-cli 这个 bin crate 的私有项**(`act::runner_port_from_env`、`main::run_port`、
`authoring::suggest_selectors`);smix-cli 没有 lib target,`tests/` 下的集成测试永远看不见它们。
把它们提 pub 需要给 bin crate 加 lib target = 往 crates.io 上多发一个库,只为让测试够得着 ——
按 §13 这是把研发便利放在架构 clean 前面,拒。反过来,bin crate 内的 `#[cfg(test)] mod`
**同时**看得见:crate 私有项、全部依赖的公开 API(`smix-adapter-maestro` 的 `Adapter` / `AppLike`、
`smix-driver`、`smix-selector`)、以及 clap 的 `Cli::command()` 树本身(比从 `--help` 文本里
反解更直接)。`main.rs:2725` 已有 `mod tests`,`authoring.rs:482` 已有用实测树 fixture 的单测 ——
同一惯例。

**唯一一处生产可见性改动**:`crates/smix-driver/src/lib.rs:1039` 的
`require_runner_resolvable_selector` 提为 `#[doc(hidden)] pub`。它不是修复,是把**已经存在的契约**
变得可观察。该 crate 已有两处同形先例(`lib.rs:1178` / `lib.rs:1184` 的 `_*Reexport`,
注释写明就是为跨 crate 触达)。此改动按 §10 进 `docs/v2.md` 决策日志。

### 口径 2 — 覆盖边界:哪些静态可判,哪些必须人/设备

**闸门判得到**(全部无需设备):

| 层 | 判的是什么 | 靠哪段真代码 |
|---|---|---|
| 解析 | 示例是否解析得过 | `parse_flow_yaml`(与 `guide_yaml_parses` 同源,不重复它的断言) |
| 运行时分发 | 示例会落到哪个 SDK 调用 | 真 `Adapter` + 记录型 `MockApp: AppLike` 的调用轨迹 |
| driver 准入 | 落到 runner-side 路由的 tap,选择器是否被接受 | `require_runner_resolvable_selector` |
| CLI 表面 | 某个 verb 的端口优先级链有几级可索引 | `Cli::command()` 子命令树 + `act::runner_port_from_env` / `main::run_port` |
| 建议器 | 裸字符串形态在**实测树**上是否可命中 | `authoring::suggest_selectors` + `tests/fixtures/live-tree-preferences-2026-07-22.json` |

**闸门判不到**(必须逐条写进 gate docstring 的 `WHAT THIS CANNOT SEE`):

1. **设备层的一切**。元素是否真在屏上、IOHID 是否真触发 `onTap`、被测应用的 launcher activity
   是否恰好叫 `.MainActivity`、runner 是否在跑。闸门判的是「这条示例会走哪条路径、那条路径接不接受它」,
   **不是**「在某台机器上跑通了」。
2. **`MockApp` 之下的实现**。轨迹记到 SDK 调用为止;wire 编解码、Swift / Kotlin 侧行为不在其内 ——
   唯一一处真正下探到 driver 的是准入规则那一臂。
3. **没人为它写 probe 的散文断言**。派生臂自动判「跑不跑得动」;「本页说它会走 X 路由」这类断言
   必须手写 probe。probe 名册是手维护的,这是本闸门相对 `audit-ledger-scan`(行集由 v2.md 导出)
   **更弱**的一处,如实写明。
4. **Kotlin 侧只判到文本级**。Rust 进程调不动 Kotlin;`N2` 的判据是「字面量在 Kotlin 源里存在
   且该约束在全部指南页缺席」,不是执行级。
5. **指南之外的文档**。README、各 crate 的 rustdoc、`crates/smix-mcp/README.md` 不在语料内。
6. **不判「这条示例是不是好建议」**,只判「它会不会跑」。
7. **`P2` 判的是形态不是字面量**。帮助里印的 `'Sign In'` 不可能出现在 Settings 树里;probe 判的是
   「裸字符串这一形态在真树上可命中」,用该树里确实存在的串(`General`)。

判不到的部分按冷计划归 C3(需设备)与 C4(逐条闭合)。

### 口径 3 — 清单叫 `docs/guide-executability.md`,与既有两张表锚点互不相同

- `docs/audit-ledger.md` 的行集锚在 `docs/v2.md`「待办(按严重度,未修)」那一行的圈号 ——
  它是 **07-19 那次审计的 14 项**,已 `15 fixed / 0 present / 1 moot` 全闭。
- `docs/scope-evidence.md` 的行集锚在 `v2.md` in-scope 的编号项 —— 它是**承诺 vs 实现**。
- 新表的行集锚在**指南示例本身**:派生臂扫出的每个 broken 块必须有行,加上手写 probe 的行。

不合并的硬理由,`v2.md` 决策日志 2026-07-22 [v2.3-C3] 末条已经写死:`N1` 这条
「本段新发现,未修、未进 ledger …… **行集合由 07-19 那条待办的圈号定义,擅自加行会让闸门的
覆盖检查失去锚**。去处待定」。本表就是那个「去处」。

**不造成第四张孤表的接线**:新表带 `ledger` 列。取值为 `—`,或一个在 `docs/audit-ledger.md`
的 `#` 列里真实出现过的圈号;闸门校验这一点。于是 `P1 → ⑤a`、`P2 → ⑨b` 两行天然与旧表咬合,
而 `N1/N2/N3` 的 `—` 就是「这三条不属于那次审计」的机器可查证据。

**表结构(9 格,闸门强制;单元格内的 `|` 必须转义成 `\|`,与两道 python 扫描同样理由)**:

```
| id | 出处 | 声明 | status | probe | 依据(它会走的代码路径) | 层 | ledger | 复核 |
```

- `status` ∈ `{runs, broken, unjudged}`,开放词汇会让这一列漂回散文
- `probe`:`runs` / `broken` 行必须给出闸门里真实存在的 probe 名(双向对账:probe 无行 = 红,
  行指向不存在的 probe = 红);`unjudged` 行必须写 `—`
- `依据`:`unjudged` 行必须写明「要人做什么 / 哪个 checkpoint 结账」
- `层`:沿用 `audit-ledger-scan` 的固定词汇
  `{parser, rust-client, driver, sdk, mcp, cli, swift-runner, kotlin-runner, docs}`;
  `runs` 行必须是 `—`(没有要修的东西)
- `复核`:ISO 日期,不得晚于今天
- **不对称**:`broken` 行钉**缺陷代码**(修掉 → 判据失配 → 红 → 逼你改状态);
  `runs` 行钉**修复代码**(revert → 同样红)。与 `audit-ledger-scan` 同一设计

### 口径 4 — C1 不修任何东西

`N1` / `N2` / `N3` 在 C1 结束时仍然是坏的,并且**被记在案、被闸门盯着**。改实现还是改文档,
按冷计划分别落 C2 / C3。C1 内出现任何「顺手把它改对」= 违反本段。

---

## 步骤(线性,3 个)

### S1. 让闸门看见「示例会走哪条路径,以及那条路径接不接受它」

**红(写测试)**

- 文件:`crates/smix-cli/src/guide_gate.rs`(新)+ `crates/smix-cli/src/main.rs` 加
  `#[cfg(test)] mod guide_gate;`
- 先写一条**红向注入**断言:一个指南**可能**印、而 driver 准入规则**必然拒**的形态

  ```yaml
  - tapOn:
      text: "/Sign.*/"
      dispatch: daemonProxy
  ```

  断言闸门判它 `broken`。
- 此时只实现派生臂 1(运行时分发):真 `Adapter` 把它交给 `MockApp::tap_with_mode` 并成功返回,
  闸门判 `runs` → **断言失败**。
- 跑:`cargo test -p smix-cli guide_gate`,应看到红。

**绿(实现)**

- 文件:`crates/smix-driver/src/lib.rs`
- 改动:`fn require_runner_resolvable_selector` → `#[doc(hidden)] pub fn ...`(唯一的可见性改动,
  函数体一字不动)
- 文件:`crates/smix-cli/src/guide_gate.rs`
- **派生臂 1 —— `every_yaml_example_reaches_a_route`**
  - 语料:`include_str!` 八页 —— `02-yaml-reference` / `03-selectors` / `04-actions` /
    `05-cli` / `06-fixtures` / `07-errors` / `08-cookbook` / `10-ai-assertions`
    (本机实测:```yaml 块共 **71** 个)
  - 抽块与补 flow header 的规则沿用 `guide_yaml_parses.rs` 的 `yaml_blocks` / `as_flow`;
    `10-ai-assertions` 同样需要 `set_ai_assertions_override(Some(true))`
  - 每块:`parse_flow_yaml` → 真 `Adapter::run`,`AppLike` 由本文件内的记录型 `MockApp` 实现
    (每个方法返回 `Ok` 并把调用压进 `Vec<MockCall>`)
  - 判定:某块**在任何设备调用发生之前**被 runtime 自己的准入逻辑拒掉 → `broken`;
    正常留下轨迹 → `runs`
  - 两条豁免,预先定死,不在执行期再议:
    - 非 flow 块(`sims.json` / `config.yaml` 形态,既无 `- ` 条目也无 `appId:`)跳过,
      与 `guide_yaml_parses` 同规则
    - `runFlow:` 指向仓库里不存在的占位路径而抛 `RunError::Io` 的步骤,不算 `broken`;
      该块按其余步骤判定
    - 某块因 runtime 内固定 sleep 超过 2s → 记 `unjudged`,原因写进清单
  - **反空转下界**:判定块数 `>= 45`,否则断言失败并说明「抽取已失配,此检查会因一无所知而通过」
    (`guide_yaml_parses` 在 5 页上取 40;本臂多读 3 页,45 是不会因少数块被跳过而误红的下界)
- **派生臂 2 —— `every_documented_tap_is_admissible_at_the_driver_boundary`**
  - 对派生臂 1 轨迹里每一个落到 runner-side 路由的 tap(`TapWithMode` → `POST /tap`;
    `TapXcui` → `POST /tap-by-id`),把记录下来的 selector 交给
    `smix_driver::require_runner_resolvable_selector` 重判
  - 被拒 → 该块 `broken`
- 跑:`cargo test -p smix-cli guide_gate`,红向注入那条转 `broken`,断言通过

**重构**

- 无。`MockApp` 与 `crates/smix-adapter-maestro/tests/runtime_mock.rs` 里的那个形态相近但**不共享** ——
  跨 crate 的 `tests/` 目录不可导入,这是重复一个 mock,不是重复一份契约;在文件头注明。

### S2. 把这一族列成清单,先对着「全部 runs」变红

**红(写测试)**

- 文件:`docs/guide-executability.md`(新)—— 按口径 3 的 9 格结构,先写五行,**status 一律写 `runs`**
- 文件:`crates/smix-cli/src/guide_gate.rs`
- 断言:
  - 表格解析(格数不是 9 → 报错,不跳过;跳过等于让这一行从此不被任何检查看见)
  - 行 ↔ probe 双向对账
  - 派生臂 1/2 报出的每个 `broken` 块必须在清单里有行
  - `层` / `status` / `ledger` / `复核` 的词汇与格式校验
  - `ledger` 非 `—` 时,该圈号必须在 `docs/audit-ledger.md` 的 `#` 列里存在
- **派生臂 3 —— 五条 probe**(每条与清单一行 1:1):
  - `port_ladder_registry_rung_is_reachable`(N1)——
    `Cli::command()` 里每个会拨 runner 的子命令,必须存在一个能索引注册表的参数(device / alias);
    没有则「flag → env → **registry** → default」这一级在该命令上不可达
  - `launch_app_activity_convention_is_documented`(N2)——
    `RunnerWire.kt` 里 `.MainActivity` 字面量存在,则该约束必须在 `docs/ai-guide/` 某页出现
  - `default_tap_route_matches_the_page`(N3)——
    把 `04-actions.md` §Default tap 自己那个块跑过派生臂 1,轨迹必须是页面声明的路由
  - `daemon_proxy_id_example_is_admissible`(P1)——
    `04-actions.md` §Tap with explicit dispatch 的 `dispatch: daemonProxy` 块,轨迹必须是
    `TapWithMode`,且其 selector 通过 driver 准入规则
  - `authoring_bare_string_example_matches_a_real_tree`(P2)——
    帮助印的裸字符串形态,对 `tests/fixtures/live-tree-preferences-2026-07-22.json` 用该树里
    确实存在的串(`General`),`authoring::suggest_selectors` 必须返回 ≥ 1 候选;
    同一串走 `text` 分支(只查 text/value/title)必须返回 0 —— 后半句是红向注入,证明 probe 在测真行为
- 跑:`cargo test -p smix-cli guide_gate`,应看到红,且失败文本**至少**点名 `N1` / `N2` / `N3`
  三行(probe 判 broken,行写 runs),**不**点 `P1` / `P2`

**绿(实现)**

- 文件:`docs/guide-executability.md`
- 把 `N1` / `N2` / `N3` 三行 status 改 `broken`,补「依据」与「层」:

  | id | 依据(它会走的代码路径) | 层 |
  |---|---|---|
  | N1 | `crates/smix-cli/src/act.rs:34 runner_port_from_env` 只读 `SMIX_RUNNER_PORT` + 常量 22087;`Cmd::Tap/Find/WaitFor/Fill/PressKey/Scroll/Tree/Describe/SystemPopups` 与 `AuthoringAction::*` 的 clap 定义均无 device / alias 参数,注册表这一级无从索引。对照 `main.rs:2670 run_port`(`smix run` 三级齐全) | `cli+docs` |
  | N2 | `android-runner/app/src/main/kotlin/dev/smix/runner/RunnerWire.kt:157 foregroundCommand` 把启动 Activity 钉死在 `<pkg>/.MainActivity`;`/session/launch-app` 与 `/session/relaunch-app` 复用同一约定(同文件 159-162 行注释自陈) | `kotlin-runner+docs` |
  | N3 | 轨迹为 `Tap(Id)`:`IosDriver::tap`(`crates/smix-driver/src/lib.rs:368`)主机侧解树 + `/tap-at-norm-coord`;`AndroidDriver::tap`(`android.rs:311`)同形。`/tap-by-id` 只有两条入口 —— `dispatch: xcui` 与 `runtime.rs:62/89` 的 `v2-modal-*` / `v2-tab-*` 白名单 | `docs` |

- 若派生臂 1/2 扫出五条之外的 `broken` 块,按同一格式补行(冷计划已预期「清单只会变长」)
- 跑:全绿;摘要行打印 `guide-executability: N claims (2 runs / 3 broken / 0 unjudged) · M yaml blocks judged`

**重构**

- 无。

### S3. 把闸门接进它必须在的地方,并写清它守不住什么

**红(写测试)**

- 文件:`crates/smix-cli/src/guide_gate.rs`
- 新增自查臂 `this_gate_runs_where_it_must`:
  - `scripts/dev/preflight.sh` 文本里必须出现 `guide_gate`
  - `.github/workflows/ci.yml` 与 `scripts/release/ship.sh` 里必须出现 `cargo test --workspace`
  - 三处断言**故意不同形**,并在断言失败文本里说明为什么:CI 与 ship 已经 `cargo test --workspace`,
    再加一行专名调用是冗余;preflight 不然 —— 它的 crate 列表由 `git diff crates/*` 导出,
    **而本闸门的输入是 `docs/`**,只改文档时它会被整段跳过
- 跑:红(preflight 尚未接)

**绿(实现)**

- 文件:`scripts/dev/preflight.sh`
- 在 `--- source gates` 之前加一段,无条件跑:

  ```bash
  echo "--- guide executability"
  cargo test -j 4 -p smix-cli guide_gate -- --nocapture
  ```

  连同「为什么无条件」的注释(理由与同文件里 android 段落无条件的理由同源:
  narrowing 会重造它要堵的洞)
- 文件:`crates/smix-cli/src/guide_gate.rs` 文件头 docstring,按本仓范式两段式写全:
  - **防的是哪一次事故** —— 2026-07-22 一天内两次同型,两次都不是闸门发现的:
    `04-actions.md` 的 `daemonProxy` 示例每次都被拒;`authoring suggest` 的裸字符串在 iOS 上
    结构性返回空。三道现存闸门各守一层(flag 存在 / yaml 解析得过 / 无死链),
    **没有一道守「示例执行会怎样」**
  - **WHAT THIS CANNOT SEE** —— 口径 2 的七条,逐条写出
- 文件:`docs/v2.md` 决策日志(§10),追加一行:
  `- 2026-07-22 [v2.4-C1] 指南示例的可执行性有闸门了;`require_runner_resolvable_selector` 提为
  `#[doc(hidden)] pub` 以便闸门直调同一份准入规则而不是重写一份。理由:…`
- 跑:全绿

**重构**

- 无。

---

## Checkpoint C1 验收

```bash
cargo test -p smix-cli guide_gate -- --nocapture 2>&1 | grep -E 'guide-executability:|test result:'
grep -c '| broken |' docs/guide-executability.md
bash scripts/dev/preflight.sh
```

期望:

1. 第一条输出两类行:一行 `guide-executability: <N> claims (2 runs / 3 broken / …) · <M> yaml blocks judged`
   且 `M >= 45`;以及 `test result: ok. … 0 failed`
2. 第二条输出 **≥ 3**(三条负对照记录在案)
3. 第三条最后一行 `preflight: clean`

三条同时成立 = 闸门有判定能力、双向对照当场对上、且它在本地习惯路径上真的会跑。

## 完成后动作

1. `mv docs/plan-hot.md docs/plan-history/v2.4-c1-hot.md`
2. 调 sub-agent 生成新 `docs/plan-hot.md`(覆盖 C2:CLI 层端口优先级链与 `05-cli.md` 对齐),
   按 CLAUDE.md §6 标准模板,附加本段专属 context:
   - 必读 `docs/guide-executability.md` 的 `N1` 行(依据栏已写明它会走的代码路径)
   - 必读 `docs/v2.md` 决策日志 2026-07-22 [v2.3-C3] 末条(`N1` 的原始发现记录)
   - 硬约束:C2 每修一条,对应行必须从 `broken` 转 `runs`,且**装回缺陷时闸门要能重新变红**

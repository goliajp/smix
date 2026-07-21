# plan-hot — v2.3 到 C4:两条范围承诺既没被实现、也没被撤回,而范围文件读起来像它们都在

## 目标 checkpoint

C4:`docs/v2.md`「做什么（in scope）」里**每一项承诺**都处在三种状态之一 —— **已实现(带活的实现锚点)**、
**已撤回(带决策日志行)**、**待拍板(带一份齐备的决策材料)** —— 并且这个三选一由闸门**每次运行重新求值**。

两项当事人:

- **`--stable`**(in-scope #4,`docs/v2.md:16`)—— 全仓零实现,决策日志零撤回记录。
- **MCP 的 `session` / `diagnostic-dump`**(in-scope #3,`docs/v2.md:15` 逐个点名的八类里的两类)—— 13 个
  已注册 tool 里没有它们。

做完的样子:

- 新文件 `docs/scope-evidence.md`:一张表,行覆盖 in-scope 的全部 7 项(拆行后 10 行),每行 =
  承诺摘要 + 状态(`shipped` / `withdrawn` / `pending`)+ **可执行判据** + 理由 + 核验日。
- 新文件 `docs/scope-decisions-pending.md`:三份决策材料(`--stable` / MCP `session` / MCP `diagnostic-dump`),
  每份**六个固定小标题一个不少**,包含承诺原文与出处、实证现状、若要做动哪几层、若要撤回按哪条先例怎么写、
  **不做也不撤回的代价**、需要用户拍的板。
- 新闸门 `scripts/dev/scope-promise-scan.py`:从 v2.md **导出**在场项数(不写死 7)、重新求值每一条判据、
  并做两条点名对账(范围文件里的 `--flag`、MCP 那条括号枚举 vs 已注册 tool)。接进 preflight / CI / ship 三处。
- 七次红向注入当场演示它会因为什么变红。
- **C4 不实现 `--stable`、不实现那两个 tool、不撤回任何一条承诺** —— 这一条是机器判定的(见验收第 5 组),
  不是承诺。

**这一段要消灭的东西**:一份范围声明写下时是承诺,实现没跟上,撤回也没写,而**两边都没有东西对账** ——
于是"v2 做了什么"这个问题的**定义文件本身**在说一件没发生的事。C1 治的是缺陷清单的漂移(写下时真、后来假);
本段治的是它的镜像:**写下时是意图、后来没兑现、也没人宣布放弃**。ledger 盯的是缺陷,这张表盯的是承诺,
两者的行集合互不相交(ledger 的行由 07-19 待办的圈号定义,这两项不在其中)。

---

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix

git status --short                    # 期望:只有 `?? docs/plan-hot.md`(本文件,尚未提交)
git branch --show-current             # 期望:feature/v2.0
test ! -f docs/plan-history/v2.3-c4-hot.md   # 期望:成立(本段尚未归档)
pgrep -fl 'cargo|xcodebuild|gradle'   # 进 S1 前先看别的会话/用户是否在编译
bash scripts/dev/preflight.sh         # 期望末行:preflight: clean
```

**热化时已完成的本机探测(后面按它写,不按冷计划的假设写)**:

- **preflight 在热化时没有跑** —— 探测发现另一个会话正在 `stables/mailrs` 跑
  `cargo clippy --workspace --all-targets` + `cargo test --workspace`(pid 16367),`uptime` load
  **6.88 / 8.89 / 9.19**。preflight 要对 4 个 crate 跑 fmt/clippy/test 外加 gradle,与之抢核。
  **S1 起手第一件事是重跑 `pgrep` 与 preflight**,不要把这条当已完成。
- 工作树**干净**(`git status --short` 空),HEAD = `b0308d5e9`(C3 收尾),`docs/plan-hot.md` 不存在 —— 冷计划入口条件的三条全部成立。
- **`--stable` 零实现,三种模式各查一次**(否定式结论在 grep 够宽之前不成立 —— 07-21 `impl FlowAttemptShape` 的教训):
  - 字面 flag:`grep -rn -- '--stable' crates/ swift-bridge/ android-runner/ npm/` → **0 命中**
  - 标识符族:`grep -rniE 'stable_mode|stablemode|freeze_animation|freezeanimation|slow_animations|slow_motion|reduce_motion|deterministic_mode'`
    → crates 里只有 `crates/smix-simctl/src/lib.rs:1839 set_reduce_motion` 与它自己的 README/CHANGELOG
  - clap 声明:`grep -rn 'long = "stable"' crates/` → **0 命中**;`Cmd::Run` 的 flag 清单里也没有
  - 环境变量:`git grep -n 'SMIX_STABLE'` → 只在 `docs/dogfood-archive/insight-roadmap.md:397/399`(源文档),
    实现树 0 命中
- **`--stable` 的三块积木,一块已在、两块不在**(材料要用):
  - `crates/smix-simctl/src/lib.rs:1839 set_reduce_motion` **存在但零调用方** ——
    `git grep 'reduce_motion' -- crates | grep -v smix-simctl` 空,`reduceMotion` / `reduce-motion` 全仓 0
  - `status_bar override`(源文档 §K 的第一步):`grep -rn 'status_bar\|statusBar\|status-bar' crates/smix-simctl/src/lib.rs` → **0 命中**
  - runner 侧禁用动画:`grep -rn 'setAnimationsEnabled' swift-bridge/` → **0 命中**;
    `swift-bridge/Sources/SmixRunnerCore/` 的 30 个路由里没有对应项
- **`--stable` 的承诺不止 v2.md 一处**(见完成后报告第 5 条):源头是
  `docs/dogfood-archive/insight-roadmap.md:390` 的 **§K,状态 `🔬 explored`、cost S**,原文写明它要
  **三步 + 消费方 app 侧配合**(`SMIX_STABLE=1` 由 RN root 或 qa-server 承认)。另有 `.claude/design/v2.0/`
  三处(`index.html:83` / `roadmap.html:57` / `features.html:245-256`,Spec D 状态标 `design`)——
  但 `git check-ignore` 实测命中 `.gitignore:15 .claude/*`,**dossier 是 gitignored 的**,
  clone 之后不存在,不能作证据(写进闸门规则)。`CHANGELOG.md` / `README.md` / `llms.txt` / `llms-full.txt` /
  `web/` / `dashboard/` / `npm/` / `docs/ai-guide/` / `docs/roadmap.md` 全部 **0 命中** —— 对外没承诺过。
- **MCP 13 个 tool 的名字**(`grep -c '#\[tool(' crates/smix-mcp/src/main.rs` = **13**,与 fact-scan 检查 2 的计数口径同源):
  `smix_describe` / `smix_tree` / `smix_find` / `smix_tap` / `smix_fill` / `smix_swipe` / `smix_scroll` /
  `smix_launch_app` / `smix_stop_app` / `smix_assert_visible` / `smix_assert_not_visible` /
  `smix_press_key` / `smix_screenshot`。
  与 in-scope #3 点名的八类对照(子串匹配,`-`→`_` 规范化后):
  `fill`✓ `swipe`✓ `scroll`✓ `launch`✓(`smix_launch_app`)`stop`✓(`smix_stop_app`)
  `assert`✓(两个)**`session`✗** **`diagnostic-dump`✗**。
- **那两类的成本形状不一样,材料必须分开写**:
  - `diagnostic-dump` 是**薄包装** —— `crates/smix-runner-client/src/lib.rs:1113 pub async fn diagnostic_dump`
    已存在,`crates/smix-cli/src/main.rs:2313` 已在调它,`smix-runner-wire` 有 `DiagnosticDumpResponse`。
    但 `smix-sdk` **不转发它**(`git grep diagnostic_dump -- crates/smix-sdk` 空),而 `smix-mcp` 的
    依赖里只有 `smix-error` / `smix-sdk` / `smix-screen` —— 所以要么给 sdk 加转发,要么给 mcp 加 runner-client 依赖。
  - `session` 可能**根本不是一个 tool** —— `smix_launch_app` 已经 `open_session_in_place`
    (`crates/smix-mcp/src/main.rs:297`),session 生命周期对外部 agent 已是隐式的。
    S1 要回答:in-scope #3 那串枚举的出处在哪、它说的 "session" 指的是一个 tool 还是一种已兑现的语义。
- **in-scope 是 7 个编号项**(`docs/v2.md:11` 到 `:21` 之间 `^[0-9]\. ` 计数 = 7);该块的 sha1 =
  `8de9186b9ee59133ccd9e116fb6e98adcc45bd9a`(验收第 5 组用它钉住"范围没被我改过")。
  括号用的是**全角 `（）`**,枚举分隔是 `/` —— 闸门的抽取规则必须按全角写,ASCII 括号取不到。
- **撤回该长什么样,仓里有唯一的范例**:`docs/v2.md:58`,`- 2026-07-17 [C3 撤回·spec F] **不建 OCR 键盘字符 fallback**。`
  它给出五个要素(材料的「若要撤回」栏按这五条写):(a) **回源核对原文**并逐字引用源文档
  (`insight-v1.0-comprehensive.md:85`);(b) 说明**底层需求是否已被别的机制满足**(§P0-A 已 SHIPPED);
  (c) 说明**重启条件从未满足**(无消费者报告);(d) 说明**它本身是不是降级**(拿确定性换有已知误判模式的概率机制);
  (e) 说明**是否让步 parity**(maestro 同样没有,故不让步)。
- **既有闸门的接线点**:`scripts/dev/preflight.sh:81` 的 `for gate in …` 列表(现 6 个);
  `.github/workflows/ci.yml:80-91` 的 `source-gates` job;`scripts/release/ship.sh:157` 的
  audit-ledger 段(fact scan 在 `:215`)。
- `docs/audit-ledger.md` 已在 `scripts/dev/hygiene-scan.py:66` 的 `EXCLUSIONS` 里带理由挂着 ——
  本段两份新文档按同一形状处理(见 S3)。
- 产品目录相对 `origin/develop` 的改动文件名单 sha1 = `15f900f8cb72398bf66a27f279a790c5743ab2cb`(13 个文件)。

---

## 步骤(线性,无分叉)

### S1. 把两项的实证现状查到能写进材料的程度,并给全部 7 项定出(状态,判据)

**这一步不写任何新文件,不改任何代码**(冷计划「TDD 要点」明写 C4 是文档产出;红绿三段式由 S2/S3 的闸门承担 ——
不给纯核实步骤编一个假的红相)。产出落在本文件下方的「S1 实测记账」段。

**预先定死的口径(执行期不得再议)**:

1. **C4 只产出材料,不做决策,也不实现**。三条禁令是机器判定的(验收第 5 组):产品目录零改动;
   `docs/v2.md` 不新增撤回行;in-scope 块字节不变。**建闸门不算实现承诺** —— 闸门盯的是"三选一还成立吗",
   不替任何一项选。
2. **`pending` 是一个合法的、被盯住的终局状态**,不是"我还没想好"。它要求材料齐备 + 一条
   `none` 形式的判据(见下)在树里持续为真。
3. **不给任何一项写推荐**。材料的第六栏只提问题与列后果,不含"我建议 X"
   (`guideline/utility-judgment-is-not-mine`:值不值得做的判断不属于我)。

**判据文法(与 ledger 同源,两边都按这一份;闸门与写表不在执行期另议)**,写在反引号里,只有两种形式:

- `` `at <仓库相对路径>:<行号> "<字面量>"` `` —— 该字面量必须**出现在该文件的该行**
- `` `none "<字面量>" in <glob>` `` —— 该字面量在该 glob 下**零命中**

**按状态分的判据规则(这是这张表区别于 ledger 的地方,三条是全部)**:

1. **`shipped`** —— 判据必须是 `at` 形式,且路径 **git 追踪** 且 **不在 `docs/` 下**。
   理由承重:一条承诺**不能拿另一份文档当自己被实现的证据** —— `--stable` 正是靠这个在四份文档之间
   自洽了七个月(v2.md 承诺、dossier Spec D 标 design、roadmap.html 列进里程碑、源文档标 explored),
   而实现树里一处都没有。同理 `.claude/design/v2.0/` 是 gitignored,连"读者看得到"都不成立。
2. **`withdrawn`** —— 判据必须是 `at docs/v2.md:<行号> "<字面量>"`,且该行匹配撤回行首形态
   `^- \d{4}-\d{2}-\d{2} \[[^\]]*撤回`,且该行含本承诺的关键符号。范例 = `docs/v2.md:58`。
3. **`pending`** —— 判据必须是 `at docs/scope-decisions-pending.md:<行号> "## <标题>"`,指向该项的材料小节;
   且该小节的 `### 实证现状` 里**必须至少有一条 `none` 判据**,由闸门一并重新求值。
   这条是不对称设计的另一半:**"这项承诺现在仍然不存在"这个事实本身被机器盯着** ——
   有人把 `--stable` 实现了,`none` 判据当场失配,表被逼着改状态。ledger 用不对称引文堵住
   "已修却仍标未修";这里用它堵住"已做却仍挂待拍板"。

**拆行规则**:一项 in-scope 含多个可独立去留的交付物时拆行,编号 = 项号 + 一个 ASCII 小写字母
(`3a` / `3b` / `3c`);闸门做覆盖对账时按**去掉尾部字母的前缀**匹配,拆行不破坏与 v2.md 的一一对应。
**只在子交付物的状态真的不同时才拆**。

**7 项的定状态清单与热化时的起点**(起点是热化时 grep 到的,S1 要往下走完,不是照抄):

| 项 | 承诺摘要 | 热化时的起点 | S1 要回答的问题 |
|---|---|---|---|
| 1 | 围栏式 AI 断言层 `assertCondition` / `extractWithAI` | `crates/smix-adapter-maestro/src/{parser,runtime,entry}.rs` 三处均有 | 哪一行最适合当**不在 docs/ 下**的实现锚点 |
| 2 | iOS + Android 全 parity 作发布门槛 | 门禁表在 gitignored dossier 里 —— **不能当证据** | 实现树里哪一处能证明 parity 这件事落了地(Android driver / parity 测试) |
| 3a | MCP 驱动面主体(fill/swipe/scroll/launch/stop/assert 六类) | 13 个 `#[tool(` 已注册 | 锚一处 `#[tool(` 即可;**不试图证明"成熟"**(见 S2「不查什么」) |
| 3b | MCP `session` | 无 tool;`main.rs:297` 已 `open_session_in_place` | 那串枚举的**出处**在哪;"session" 指一个 tool 还是一种已兑现的语义 → 决定材料怎么写 |
| 3c | MCP `diagnostic-dump` | `runner-client:1113` 有 `diagnostic_dump`,`smix-sdk` 不转发,`smix-mcp` 不依赖 runner-client | 接它要动 `sdk` 还是 `mcp` 的依赖图;两条路各自的连带 |
| 4a | `--stable` | 四种模式全 0 命中(见前置条件) | 源文档 §K 的三步各自今天要多少工作;`set_reduce_motion` 零调用方这件事说明什么 |
| 4b | 真 animation-idle(frame-diff 取代固定 sleep) | `ceiling_ms` 在 `crates/smix-adapter-maestro/` 四个文件里(C3/v2 已落) | 锚在实现 frame-diff 判定的那一行,不是锚在 yaml 字段名 |
| 5 | 六项破坏性变更 + `smix migrate` | `crates/smix-migrate/src/lib.rs` | 锚一处 codemod 实现 |
| 6 | 代码 & 文档 hygiene | `scripts/dev/hygiene-scan.py` | 锚在闸门实现上(它不在 docs/ 下,合规) |
| 7 | 面向 AI 的文档 `llms.txt` / `llms-full.txt` | 仓库根有 `llms.txt` / `llms-full.txt`,`scripts/dev/gen-llms.py --check` 已在 preflight | 锚在生成器还是产物 —— 取**会因实现回退而失配**的那一处 |

**过程约束**:

- 否定式结论(X 不存在)在 **grep 模式够宽之前不成立** —— `--stable` 已按四种模式查过(见前置条件),
  其余项的否定式结论同样**至少换两种模式**(带路径前缀 / 只取符号名)各查一次。
- **发现新的"承诺无实现"项就照三选一处置**:它天然属于本表(表的行集合由 in-scope 的项数定义,
  不像 ledger 那样有外部锚),不需要另开清单;但**不得**因此实现或撤回任何东西。
- **不起模拟器 / emulator**。本段全部判据都不依赖设备 —— 这是 C3 定下的收窄判据(§5:checkpoint
  要能半年后重跑给出确定结论),C4 天然满足。
- 材料里**不写版本坐标形态的串**(`2.0.0` / `@goliapkg/smix@x.y.z` 之类)—— fact-scan 检查 1/5
  会把它当安装坐标扫,而这两份文档不是安装面。

**记账(S1 完成后把结果写回本文件此处,原样进 plan-history 作审计痕迹)**

> *(执行期填写:10 行的 `项 | 承诺摘要 | 状态 | 判据 | 理由` 表;外加「三份材料的事实底稿」小节 ——
> 每份记下承诺原文出处、别处承诺的位置及其可否作证据、实证现状的 grep 模式与结果、要动的层、
> 撤回按五要素各该写什么、以及不做也不撤回时谁会读到它并得出什么错结论)*

**重构**

- 无。本步只产生事实,不改任何文件。

---

### S2. 闸门先行:`scope-promise-scan.py` 在两张表还不存在时必须是红的

**为什么闸门写在表之前**:表是被检查的对象。先有表再补闸门,闸门就会被写成"能让这两张表过"的形状 ——
那正是"一致 ≠ 为真"。先写闸门、看它对着空气变红,再让表去满足它。(C1 / C2 同一条理由。)

**红(判据先于实现)**

```bash
python3 scripts/dev/scope-promise-scan.py; echo "exit=$?"
```

期望 **exit=1**,且失败消息里同时出现两条:

- `docs/scope-evidence.md` 不存在;
- 本脚本未被 `scripts/dev/preflight.sh` / `.github/workflows/ci.yml` / `scripts/release/ship.sh` 调用。

第二条是自指检查:**adb-guard 就是死在这里的 —— 脚本提交了,让它运行的那一行没提交**。
所以它在 S2 阶段就得红,不能等接完线才第一次跑。

**绿(实现)**

- 新文件:`scripts/dev/scope-promise-scan.py`
- **docstring 按本仓惯例写它防的是哪一次事故**:2026-07-22 发现 in-scope #4 承诺的 `--stable` 全仓零实现
  且决策日志零撤回记录;in-scope #3 逐个点名的八类能力里 `session` / `diagnostic-dump` 在 13 个 tool 里不存在。
  **范围文件是"v2 做了什么"的定义,而没有任何东西把它跟实现树对过账** —— fact-scan 只核对
  "声称 13 个 = 实际 13 个",不核对"范围文件要求的八类都在不在"。

**闸门的检查项(八条,逐条给出它防的是什么)**

1. **在场项数从磁盘导出**:在 `docs/v2.md` 里按锚句 `## 做什么（in scope）` 定位**唯一**一节
   (0 节或 ≥2 节都判红),取到下一个 `^## ` 为止的 `^\d+\. ` 编号行,得到 N(**不写死 7**)。
   将来 in-scope 补第 8 项,表当场变红要求补行 —— `android-gate-scan` 把期望值从磁盘导出的同一手法。
2. **覆盖对账**:`docs/scope-evidence.md` 的 `#` 列去掉尾部 ASCII 字母后必须**恰好覆盖** 1..N,不多不少。
3. **表结构**:恰好一张表、六列
   (`| # | 承诺（in-scope 原文摘要） | 状态 | 判据 | 理由 | 核验日 |`)、
   状态 ∈ `{shipped, withdrawn, pending}`、核验日是 ISO 日期且不在未来。封闭词表的理由:
   `done` / `部分做了` 这类词一旦允许,状态栏就退回散文。**解析出 0 行直接红**(而不是"表变短了")——
   一个把表读成空的正则错误会让后面每条检查空洞地通过。
4. **判据可执行**:逐条求值 `at` / `none` 两种形式。`at` 失配时**打印该字面量在该文件里的真实行号**,
   让修法是一次改数字而不是一次重新调查;一处都没有时明说"字面量已不在该文件 —— 这一行的状态很可能已经变了,
   重新核实并把核验日改成今天"。**材料文件里 `### 实证现状` 段中的 `none` 判据一并求值**。
5. **按状态分的锚点约束**(把 S1 的三条判据规则钉成机器可判):
   - `shipped`:`at` 形式、路径 git 追踪、**路径不在 `docs/` 下**
   - `withdrawn`:`at docs/v2.md:<行>`,该行匹配 `^- \d{4}-\d{2}-\d{2} \[[^\]]*撤回`
   - `pending`:`at docs/scope-decisions-pending.md:<行>` 指向 `## ` 小节;该小节下六个固定 `### ` 小标题
     (`承诺原文与出处` / `实证现状` / `若要做:动哪几层` / `若要撤回:先例与草稿` / `不做也不撤回的代价` /
     `需要你拍的板`)**顺序一致、各恰好一次**;`### 实证现状` 下 ≥1 条 `none` 判据。
     **材料文件仅在存在 `pending` 行时才被要求存在** —— 全部拍完板后它可以消失,表仍然完整。
6. **CLI flag 点名对账**:in-scope 文本里每个反引号包裹、以 `--` 开头的 token,必须在表里**自成一行**
   (该行的承诺列含这个 token)。这条直接对着 `--stable` 而来:它使"承诺了一个 flag 却既不实现也不撤回"
   这件事**在结构上无法沉默**。
7. **MCP 能力点名对账**:从 in-scope 里那条含 `MCP` 的项抽取**全角括号内以 `/` 分隔**的名字表(现为 8 个),
   逐个 `-`→`_` 规范化后,在 `crates/smix-mcp/src/main.rs` 的 `async fn smix_*` 名字集合里做子串匹配。
   **未命中的名字必须在表里各有一行** `pending` 或 `withdrawn`(承诺列含该名字)。两侧都从磁盘导出,
   谁增删都对账 —— 加一个 `smix_session` tool,`session` 就命中了,而它那一行的 `none` 判据会同时失配(检查 4),
   两条一起把表推向更新。
8. **自指**:本脚本必须被 preflight / CI / ship **三处**调用。三处不是一处:preflight 是本地习惯,
   CI 是分支,ship 是发布;单缺 ship 那一处,漏的正好是通向用户的那条路。

**不做的检查,以及为什么**(写进 docstring —— 一个说不出自己不查什么的闸门会被读成全知):

- **不判断程度词是否兑现**。in-scope #3 说的是"MCP 驱动面**成熟**",#6 说的是 hygiene,#2 说的是"**全** parity"。
  一处 `at` 引文证明不了"成熟 / 全",它只证明这一格**不是整个不存在**。程度判断仍需人做 ——
  这是 §13 意义上的诚实标注,不是遗漏。
- **不逐个校验 in-scope 里所有反引号 token**。那份文本的反引号还包着路径(`.smix/config.yaml`)、
  产物名(`llms.txt`)、章节号(`§9#4`)—— 全当符号会逼出一张手工豁免表,又一份没人维护的清单,
  正是本段在治的病。**代价说清**:被检查 6/7 覆盖之外的子交付物,仍可能静默缺席,靠拆行(人做)补,
  **闸门看不见拆得够不够**。
- **不检查 `pending` 行挂了多久**。"合理时间内该拍板"机器判不了,写一个假的 SLA 只会训练出无脑改日期的反射
  (ledger 不查提交日期的同一条理由)。

**重构**

- 不把这条检查并进 `audit-ledger-scan` / `fact-scan` / `hygiene-scan`。四者问的是不同的问题
  (自家缺陷记账还成立吗 / 对外说的是真的吗 / 读起来像内部吗 / **范围承诺的东西在树里吗**),
  合并会让失败消息说不清是哪一层。**ledger 尤其不能合**:它的行集合由 07-19 待办的圈号定义并有硬覆盖检查,
  往里加行会当场破坏那条承重对账。

---

### S3. 两张表落地、接线、七次红向注入

**绿(表落地)**

- 新文件 `docs/scope-evidence.md`:S1 记账段的 10 行,按 S2 的文法。开头三段(不超过 15 行)说清:
  它是什么(v2.md in-scope 每项承诺的实证状态)、**判据栏是可执行的、由 `scripts/dev/scope-promise-scan.py`
  每次重新求值**、改一行时要同时改核验日。
- 新文件 `docs/scope-decisions-pending.md`:三份材料(`--stable` / MCP `session` / MCP `diagnostic-dump`),
  每份六个固定 `### ` 小标题。**内容清单(定死,不在执行期增减)**:
  1. `### 承诺原文与出处` —— 逐字引 in-scope 原文 + `docs/v2.md:<行>`;并列出**别处的同一承诺**及其
     可否作证据(dossier 三处 gitignored ⇒ 不可;源文档 `insight-roadmap.md:390` §K 状态 `🔬 explored` ⇒
     它本来就不是承诺,是探索记录)。
  2. `### 实证现状` —— 至少两种 grep 模式的模式串与结果 + 代码位置;**已存在的半成品积木点名**
     (如 `set_reduce_motion` 存在且零调用方);**必须含至少一条 `none` 判据**。
  3. `### 若要做:动哪几层` —— 层用 ledger 的固定词表(`parser` / `rust-client` / `driver` / `sdk` /
     `mcp` / `cli` / `swift-runner` / `kotlin-runner` / `docs`),逐层写要加什么;并写**跨层连带**
     (stone crate 的 additive-only ABI 约束、fact-scan 的 tool-count 会随 `#[tool(` 数变、
     `llms.txt` 要重生成、CHANGELOG / 破坏性变更表要不要动、**消费方 app 侧要不要配合**)。
     **它是给用户排序用的,不是估工时。**
  4. `### 若要撤回:先例与草稿` —— 按 `docs/v2.md:58`(2026-07-17 OCR 键盘 fallback)的**五要素**
     逐条给出本项的答案:(a) 回源核对原文并逐字引;(b) 底层需求是否已被别的机制满足;
     (c) 重启条件是否满足过;(d) 它本身是不是降级 / 代价在哪;(e) 是否让步 parity(maestro 有没有)。
     再附一段**待填草稿**,放进围栏代码块,**不填日期、不追加进 v2.md** —— 撤回是用户的决定。
     (结构上也拦得住:检查 5 只认 `at docs/v2.md:<行>` 的 `withdrawn` 判据,材料里的草稿永远满足不了它。)
  5. `### 不做也不撤回的代价` —— 具体到**谁会读到它、会得出什么错结论**:范围文件是发布就绪度的定义;
     dossier 与官网 IA 从它取材;下一个大版本的边界文件从它继承。
  6. `### 需要你拍的板` —— 一句话的问题 + 三条路径(做 / 撤回 / 改写承诺措辞)各自的后果。
     **不含推荐**。
- **为什么是这两个文件,不是别处**(把选择写死,不在执行期挑):
  - **不进 `docs/audit-ledger.md`**:那张表的行集合由 07-19 待办的圈号定义并有硬覆盖检查(C1 的承重设计),
    这两项不在其中;加进去会当场破坏对账。且两者盯的是不同的东西 —— ledger 盯**缺陷**,这张表盯**承诺**。
  - **不进 `docs/v2.md` 决策日志**:那是 append-only 的历史,写下时是什么样就该是什么样。
    把待拍板材料塞进去,会让同一条承诺在同一文件里同时存在"承诺"与"其实没做"两种说法,读者无从知道哪条当真;
    闸门还要在 660+ 行里靠脆弱的锚点找结构。**v2.md 正文一字不动**,只在 §10 追加一行指向这两份文件。
  - **不进热计划 / 冷计划**:两者都会随 checkpoint 归档,而这两份文件要活到用户拍板之后。
  - **两份分开而不是一份**:寿命不同。`scope-evidence.md` 每个大版本重建、长期存在、被闸门盯;
    `scope-decisions-pending.md` 在最后一项被拍板时**应该消失**(检查 5 明写:无 `pending` 行时不要求它存在)。
    把会消失的东西和要长期存在的东西放一个文件,前者就永远消失不掉。
- 两份新文档是中文为主的**内部记账**,会命中 `hygiene-scan` 的 CJK 与记号规则。处置**预先定死**:
  在 `hygiene-scan.py` 的 `EXCLUSIONS` 加两条带理由的条目,与 `docs/audit-ledger.md` 同源同形 ——
  `("docs/scope-evidence.md", "internal scope accounting; the in-scope wording is its subject")` 与
  `("docs/scope-decisions-pending.md", "decision material awaiting a ruling; the promise text is its subject")`。
  该列表每次运行都把豁免连同剩余命中数打出来(豁免是有主的债,不是没走过的地)。
  **若 fact-scan 也报,一律先改措辞;确需豁免才加带理由的条目 —— 改闸门就范是最后手段。**
- **但 `POINTER_SKIP`(死指针豁免)不加**。两份文档里的每个路径正是本段要让它**保持活着**的东西;
  hygiene-scan 若在这里报死指针,那是**真信号**,改路径不改闸门。

**绿(接线,三处)**

- `scripts/dev/preflight.sh`:加进第 81 行那个 `for gate in …` 列表
- `.github/workflows/ci.yml`:`source-gates` job 里加一步,与 `audit ledger scan` 并列
- `scripts/release/ship.sh`:紧跟 `--- audit ledger scan` 段之后新增 `--- scope promise scan` 段,
  **非 bypass**,日志落 `/tmp/smix-ship-scope-promise.log`。放这里的理由:它与 audit-ledger 是同一族问题
  (一个问"自家缺陷记账是不是还成立",一个问"自家范围承诺是不是还成立"),相邻能让读 ship 输出的人一眼看出这两问。

**绿(v2.md §10 追加一行)**

在决策日志末尾追加,行首形态承重(验收按 `^- 20[0-9-]+ \[v2\.3-C4` 精确匹配 —— 模糊 grep 只找 `C4`
会被第 42 行「Checkpoint 概览」里 v2 自己的 C4 骗过去):记 `--stable` 与 MCP 两类的**待拍板**状态、
两份新文件的位置、闸门立起来了**以及它查不到什么**(程度词不查、反引号 token 不逐个查)。
**不含撤回、不含实现承诺**。
**追加时注意**:正文若写到被 sim-guard / adb-guard 拦的命令形状,heredoc 正文会被 guard 当命令读
(07-21 已发生过一次)—— 改措辞或改用编辑工具写入,**不改 guard**。

**红向注入(七次,每次看到红再还原;还原一律走 `cp` 备份,禁止 `git checkout <file>` —— 07-21 用它抹掉过未提交的改动)**

| # | 注入 | 期望的红 |
|---|---|---|
| R1 | `docs/v2.md` in-scope #4 里 `--stable` 改成 `--frozen` | 检查 6:范围文件点名了 `--frozen`,表里没有它的行 |
| R2 | 某 `shipped` 行的判据改成 `at docs/v2.md:16 "--stable"` | 检查 5:`shipped` 不得以 `docs/` 下的文件作证据 |
| R3 | 删掉某份材料的 `### 不做也不撤回的代价` | 检查 5:该 `pending` 小节缺必填小标题(点名缺的是哪一个) |
| R4 | 某行状态 `pending` → `open` | 检查 3:状态词不在封闭词表里 |
| R5 | `docs/v2.md` in-scope 追加第 8 项 | 检查 1/2:在场 8 项、表只覆盖 7 —— 同时证明 N **不是写死的** |
| R6 | 从 `preflight.sh` 删掉调用 | 检查 8:本脚本未被三处全部调用 |
| R7 | 把 `--stable` 材料里的 `none` 判据字面量改成树里存在的串 | 检查 4:该字面量有命中 —— "这项承诺已经不是缺席状态了,重新核实" |

R7 是承重的:没有它,`pending` 就退化成一个只要材料写齐就永远绿的状态,而**材料写齐是最不会失效的那部分**。
R1 与 R7 合起来才证明这张表在两个方向上都会因为现实变化而变红。

**重构**

- 表里不加"责任人" / "目标版本" / "预计工时"栏。它们不可机器判定,会变成又一处静默过期的散文;
  且"值不值得排期"的判断不属于我。
- 不动 `docs/v2.md` 的 in-scope 正文与既有决策日志条目文字。

---

## Checkpoint C4 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

# 1. 闸门本身绿,且真的重新求了值
python3 scripts/dev/scope-promise-scan.py

# 2. 三处接线都在(缺 ship 那处 = 漏掉通向用户的那条路)
grep -q 'scope-promise-scan' scripts/dev/preflight.sh
grep -q 'scope-promise-scan' .github/workflows/ci.yml
grep -q 'scope-promise-scan' scripts/release/ship.sh

# 3. 材料齐备:三份 × 六个固定小标题
grep -c '^### ' docs/scope-decisions-pending.md
grep -c '^## ' docs/scope-decisions-pending.md

# 4. 两份新文件进了 git(不是只在磁盘上)
git ls-files --cached --others --exclude-standard -- docs/scope-evidence.md docs/scope-decisions-pending.md | wc -l | tr -d ' '

# 5. C4 没有实现、没有撤回、没有改写范围
git status --porcelain -- crates swift-bridge android-runner npm web dashboard examples | wc -l | tr -d ' '
git diff --name-only origin/develop...HEAD -- crates swift-bridge android-runner npm web dashboard examples | shasum | cut -c1-40
sed -n '/^## 做什么（in scope）/,/^## 不做什么/p' docs/v2.md | shasum | cut -c1-40
git diff origin/develop...HEAD -- docs/v2.md | grep -cE '^\+- [0-9]{4}-[0-9]{2}-[0-9]{2} \[[^]]*撤回'

# 6. §10 记了待拍板状态
grep -cE '^- 20[0-9-]+ \[v2\.3-C4' docs/v2.md

# 7. 既有闸门没被本段破坏
python3 scripts/dev/audit-ledger-scan.py
python3 scripts/dev/workflow-scan.py
python3 scripts/dev/fact-scan.py
python3 scripts/dev/hygiene-scan.py
bash scripts/dev/preflight.sh
```

期望:

- 第 1 条 exit 0,末行形如
  `scope-promise-scan: clean — 10 rows over 7 in-scope items (7 shipped / 0 withdrawn / 3 pending), 13 citations re-evaluated`
  (行数与计数以 S1 为准;**"over 7 in-scope items" 里的 7 必须是从 v2.md 数出来的**,不是常量)
- 第 2 条三行 exit 0(`grep -q` 静默)
- 第 3 条输出 **`18`** 与 **`3`**(三份材料 × 六个固定小标题)
- 第 4 条输出 **`2`**
- 第 5 条四行依次输出:
  - **`0`** —— 工作树里产品目录零改动
  - **`15f900f8cb72398bf66a27f279a790c5743ab2cb`** —— 相对 `origin/develop` 的产品改动文件名单
    (13 个文件,全部来自 v2.2 / v2.3 前几段)的 sha1。多一个文件、少一个、改个名,这串就变。
    **不用 `grep -v` 排除已知路径** —— 那又是一份手工维护的清单,正是本段在治的病。
    若 C4 期间 `origin/develop` 前进导致基线合法变化,**重新取一次哈希并在此处改写,同时在收尾记账里写明为什么变** ——
    不允许口头解释后放行。
  - **`8de9186b9ee59133ccd9e116fb6e98adcc45bd9a`** —— in-scope 块的 sha1,证明**范围一个字没被我改过**
    (承诺既没被偷偷删掉,也没被偷偷改软)
  - **`0`** —— 决策日志没有新增撤回行
- 第 6 条输出 **`1`**
- 第 7 条:四个闸门各自 exit 0,preflight 末行 `preflight: clean`

外加**已在 S2/S3 内完成并记录**的验证(它们要改工作树,不放进复跑命令):

- S2 红:两张表不存在 + 未接线时,闸门 exit 1 且两条消息都出现
- S3 的 R1–R7 七次注入各自变红一次,失败消息与上表逐条对上;还原走 `cp` 备份

---

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.3-c4-hot.md`
2. 按 §7 收尾 task 状态(S1/S2/S3 三个 task 全 `completed`)
3. **不自行热化 C5**(§6):把三件事报给用户,由用户说"开始 C5" ——
   - 三份材料的要点(每份:现状一句、若要做动哪几层、若要撤回按五要素各该写什么、不做也不撤回的代价),
     以及**等待拍板的正是这三条**;
   - 新闸门立起来了,**以及它查不到什么**(程度词、未被两条点名规则覆盖的子交付物);
   - S1 若查出 in-scope 里第三项"承诺无实现",一并列出并说明它已按三选一进了表。
4. **不替用户拍板**。用户拍板后,`--stable` / `session` / `diagnostic-dump` 的去处按拍板结果走:
   要做 → 进冷计划的后续 checkpoint;要撤回 → 按 `docs/v2.md:58` 的形态写决策日志行,
   表里那行从 `pending` 改 `withdrawn`,材料小节删除。**两条路都不在 C4 里发生。**

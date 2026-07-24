# C1 调研 — observation→local-claude→actionable-proposal 回路可得性 + 诚实形

> 研究先行 checkpoint（v2.11-C1，`.claude/rule/decomposition-discipline.md` =
> decomposition-before-attack）。回答:
> **「smix runtime 观察一次 flow 执行的结构化记录,经本机 `claude` CLI,能不能产出
> 可机械校验的 actionable proposal(改进提议),诚实的形是什么?」**
>
> 全程 read-only:读源码 / 读 `smix-ai-tier` 已证回路 / 读 parser / IR 类型。
> 不 edit 实现代码、不跑设备、不 commit。**§9#2:只考虑本机 `claude` CLI,网络 Claude API 路径不评。**
>
> decomposition「对手」= `smix-ai-tier`(`judge` @ `crates/smix-ai-tier/src/lib.rs:49`)已证的
> screenshot+condition → local claude → `StructuredVerdict` fenced 回路。propose 回路 = 这条的新实例
> (更富输入:bundle 而非单帧;更富输出:proposal 而非 bool)。

## Falsification rubric

**先于收证据钉死**。每条判据的 `Evidence:` 槽此刻留空,证据在 `## Evidence` 段回填,
证明 verdict 非事后合理化(同 c7-zorder 范式)。

### 轴 A(观察面)

- **判 `OBTAINABLE-A` 的充分证据**:一次 flow run **无需新 core 能力**即产结构化、
  LLM-可消费记录,且该记录携带 proposal 所需**定位**:①失败步 index + verb;
  ②失败步的**结构化 selector**(可反查原 Step);③失败时的 a11y tree;
  ④`visibleElements` / `suggestions`。全部经**现成** flag(`--debug-output` + `--format json`)可得。
  - Evidence: __(空)__
- **判 `NOT-OBTAINABLE-A` 的充分证据**:记录不结构化(仅人读文本 / 仅 PNG),
  或缺上述任一定位维度且无现成 flag 补齐。
  - Evidence: __(空)__

### 轴 B(proposal 形)—— 穷尽枚举 improvement 类

对下列**穷尽枚举**的 improvement 类,逐类判定(no-ceiling-words:负向结论须附枚举依据,
不许「结构性拿不到」hand-wave):

- **判某类 `OBTAINABLE`**:可表达为结构化、**可重新应用到 flow** 的编辑,
  落合法 `smix_adapter_maestro::Step` / `smix_selector::Selector` 变体。
- **判某类 `PARTIAL`**:可表达但有损 / 受既有 Step 词汇上界所限(能表达子集,子集外无 Step)。
- **判某类 `NOT`**:IR 无任何结构可承载。

枚举清单(与 plan-hot S1 一致):selector swap / waitFor 插入 / step reorder / 断言改 / verb 改。
- selector swap — Evidence: __(空)__
- waitFor 插入 — Evidence: __(空)__
- step reorder — Evidence: __(空)__
- 断言改 — Evidence: __(空)__
- verb 改 — Evidence: __(空)__

### 轴 C(验证,分两层)

- **well-formedness(device-free)**:`OBTAINABLE` iff proposal 反解为 amended flow 后,
  **现成 parser** 能校验其合法(合法 Step/Selector,parser 接受)。
  - Evidence: __(空)__
- **effectiveness(device replay)**:`OBTAINABLE` iff amended flow 从 fail→pass 的重跑回路
  **无需新能力**(`smix run` 现成)。本轴只判**可得性**,真跑属 C4,不在此实现。
  - Evidence: __(空)__

### Overall VERDICT 判定

- `OBTAINABLE`:三轴皆可 + **≥1 improvement 类端到端可得**(结构可表达 + 可反解 amended flow +
  device-free 校验 oracle 存在 + 重跑能力现成)。
- `PARTIAL`:仅部分改进类可得,或验证只到 well-formed(effectiveness 需新能力)。
- `NOT-OBTAINABLE`:穷尽枚举后无可得回路。

## Evidence

所有 file:line 对本机以下 crate 静态阅读,read-only。本机探测:`claude` = `/Users/doracawl/.local/bin/claude`
版本 2.1.218(Claude Code);全 workspace `grep -rn 'fn propose|improve_flow|self_heal'` 零命中 = 无
任何现成 propose 基础设施(propose 回路整体是净新建造)。

**未跑 live bundle,诚实标注**:轴 A 的观察契约是 `StepDebugRecord` / `build_summary_json` /
`write_step_debug` 的 **struct 字段契约**,source 可判(下证)。产一个**真** `--debug-output` bundle
需要 booted sim + 已构建 app + 运行中 runner(`write_step_debug` 内 `self.app.screenshot()` /
`self.app.tree()` 是跨进程 runner 调用,无 device-free 造法),且会占用/干扰 batch。跑前
`pgrep -f 'runner.ts|smix run|supervise'` = 无 batch owner,但**起 sim+app+runner 本身是重动作、
device replay 属 C4**,故本 checkpoint 走 source-only,不 live。轴 A 的每一维定位都能从 struct 定义
+ 写入路径判定,source 充分。

### 轴 A(观察面)→ `OBTAINABLE-A`

一次 flow run 经**两个现成 flag** 产结构化 LLM-可消费记录,二者**同一 run 内并存**
(`entry.rs:509-514`「Both work together — debug_output for the on-disk artifact, json format for
stdout」)。proposal 所需定位分布如下,**无一需要新 core 能力**:

1. **失败步 index + verb** — `StepDebugRecord`(`crates/smix-adapter-maestro/src/runtime.rs:677`)
   字段 `n: usize`(1-based step index)+ `verb: String`(`"tapon"` / `"assertvisible"` 等)+
   `verdict: String`(`"ok"|"skipped"|"expanded-subflow"|"failed"`)+ `failure_kind` /
   `failure_message`。写入路径 `write_step_debug`(`runtime.rs:1202`)对每步产
   `step-<N>-<verb>.json`,`run-summary.json` 由 `build_summary_json`(`entry.rs:636`)聚合
   `"steps": debug_records`(失败时 `runOutcome: "failure"` + partial trace 到失败步)。
2. **失败步的结构化 selector** — 不在 debug bundle 里(bundle 只有人读 `summary` + `verb`),
   但**可反查**:`StepDebugRecord.n` + 原 flow 文件 → `parse_flow_yaml`(`parser.rs:2942`)重解析
   得 `Vec<Step>`,第 n 步的 `Step` 变体携带 typed `selector: Selector`
   (`smix_selector::Selector`,11 变体,`smix-selector/src/lib.rs:324`)。**另**一路:`--format json`
   的 terminal `ExpectationFailure`(`emit_json_failure`,`entry.rs:669`)携 `selector: Option<Selector>`
   (`smix-error/src/lib.rs:82`)。两路都给结构化失败 selector。
3. **失败时 a11y tree** — `write_step_debug` 在失败分支调 `self.app.tree()`,写
   `step-<N>-<verb>.fail.tree.json`(整棵 `A11yNode` `serde_json::to_vec_pretty`,`runtime.rs:1328-1339`),
   `StepDebugRecord.tree_path` 记其相对路径。
4. **visibleElements / suggestions** — `ExpectationFailure`(`smix-error/src/lib.rs:72`)字段
   `visible_elements: Vec<ElementSummary>`(:88)+ `suggestions: Vec<String>`(:85)+ `hint`(:91)
   + `screenshot`(:107,base64 PNG)+ `device_log`(:111);经 `--format json` stdout 顶层 JSON 输出。

**诚实 nuance(不下调 verdict,但 C2 必须知)**:LLM-可消费记录是**两个 output surface 的并集** ——
`--debug-output` 磁盘 bundle(index/verb/verdict/failure_kind/message/a11y-tree/PNG)∪ `--format json`
stdout(结构化 `ExpectationFailure`:selector/suggestions/visibleElements/hint)。二者同 run 并存
(`entry.rs:509-514`),但 C2 的 record-assembler 需**读两处并 join**(或据 `StepDebugRecord.n` 重解析
flow 补 typed selector)。这是 assembler 的连接工作,非能力缺口 —— 两 surface 都已现成。

→ **轴 A 判 `OBTAINABLE-A`**:四维定位全经现成 flag 可得,无新 core 能力。

### 轴 B(proposal 形)—— 穷尽枚举 → 4 类 OBTAINABLE + 1 类 PARTIAL

IR 承载面:`Step` enum(`smix-adapter-maestro/src/lib.rs:330`,约 20 变体)+ `Selector` enum
(`smix-selector/src/lib.rs:324`,11 变体)。proposal = 对 `Vec<Step>` 的结构化编辑 op。

- **selector swap → OBTAINABLE**:替换某 selector-bearing Step 的 `selector: Selector` 字段。
  持 selector 的 Step:`TapOn`(:332)/`ExtendedWaitUntil`(:400)/`AssertVisible`(:411)/
  `InputTextInto`(:420)/`ScrollUntilVisible`(:490)/`RunFlowConditional.when_visible`(:449) 等。
  新 selector 落 11 变体之一(Id/Text/Label/Role/Anchor/OcrText/...)。候选来源现成:
  `ExpectationFailure.suggestions`(:85)或 `suggest_selectors`(`authoring.rs:38`,产
  `SelectorCandidate.spec` 如 `id: <>` / `text: "<>"`)。结构化 + 可反解。**OBTAINABLE**。
- **waitFor 插入 → OBTAINABLE**:在失败步前插入一个 `Step::ExtendedWaitUntil { selector, timeout_ms,
  expect_visible }`(`lib.rs:400`,smix 的 waitFor = extendedWaitUntil)或
  `Step::WaitForAnimationToEnd { ceiling_ms }`(:384)。`Vec<Step>` 是普通 Vec,插入是净结构 op。**OBTAINABLE**。
- **step reorder → OBTAINABLE(结构层;有效性属 C4)**:`Vec<Step>` 元素换位纯重索引,IR 对任意步序无约束。
  结构可表达、可反解。**OBTAINABLE**(reorder 是否**有效/安全**是 effectiveness 问题,轴 C/C4 判,非结构问题)。
- **verb 改 → OBTAINABLE**:`Step` 是 enum,换变体 = 构造另一变体(如 `TapOn`→`ExtendedWaitUntil`,
  或同变体内改字段:`TapOn.dispatch: Some(TapDispatch::DaemonProxy)`(:342 + :318 `Xcui|DaemonProxy`)/
  `TapOn.optional: true`(:338))。结构可表达、可反解。**OBTAINABLE**。
- **断言改 → PARTIAL**:smix 断言词汇 = `AssertVisible { selector }`(:411)/ `ExtendedWaitUntil`
  的 `expect_visible: bool`(visible vs notVisible 两 arm,:406)/ `WebViewEval { assert_eq }`(:358)。
  **可表达子集**:改 AssertVisible 的 selector(⊂ selector swap)、翻 expect_visible arm、改 webview
  assert_eq 值 —— 皆结构化字段。**子集外无 Step**:无 `toHaveText` / `toHaveValue` / 数值比较 /
  正则匹配值 等独立断言谓词 Step(smix 断言面刻意收敛到 maestro 集;文本存在断言只能靠 Text-selector
  近似)。故 improvement「把断言改成 X」当 X 落 {visible, notVisible, webview-eq} 内 = OBTAINABLE,
  X 需要更富谓词 = 无结构承载。**PARTIAL,上界 = 既有断言 Step 词汇(枚举已尽,非 hand-wave)**。

→ **轴 B 判**:5 类中 4 类(selector swap / waitFor 插入 / step reorder / verb 改)**OBTAINABLE**,
  1 类(断言改)**PARTIAL**(受既有断言 Step 词汇上界所限)。**≥1 类端到端结构可得**满足。

### 轴 C(验证,分两层)

- **well-formedness(device-free)→ OBTAINABLE(oracle 现成),附净新 emitter 说明**:
  校验 oracle = `parse_flow_yaml`(`parser.rs:2942`)—— 纯函数、device-free、`Result<Flow, ParseError>`,
  amended flow yaml 反解合法与否它直接判。**OBTAINABLE**。
  **诚实 nuance(C2/C3 必须知,非能力缺口)**:parser 是**手写** `parse_step_item`(parser.rs),
  与 `Step` 的 `#[derive(Serialize)]`(`#[serde(rename_all = "camelCase")]`,:328)**不同形** ——
  `Step` serde 序列化产 `{tapOn: {selector: {...}, optional: false, dispatch: ...}}` 一类嵌套形,
  **不等于** parser 读的 maestro yaml 形(`- tapOn: {id: foo}`)。故 proposal → amended flow **不能靠
  serde 免费 round-trip**:需一个 Step→maestro-yaml **emitter**。现成 `smix-recorder::generate_maestro_yaml`
  (`authoring.rs:544` 引用)已证此 emitter 模式,但只覆盖 `IRAction` 子集(tap/fill/clear/pressKey/
  swipe/goBack/waitFor/hideKeyboard,`smix-authoring-ir/src/lib.rs:33`),**非全 Step 集**。故「反解 amended
  flow」是 C2/C3 的净新 device-free 代码(扩展 recorder emitter,或走 yaml-AST 层编辑),**不是 core/runtime
  能力缺口**。校验 oracle 本身现成 → 本层 `OBTAINABLE`。
- **effectiveness(device replay)→ OBTAINABLE(能力现成,执行属 C4)**:重跑回路 = `smix run`
  (`run_flow` @ `entry.rs`),现成、无新能力;fail→pass 判据 = `FlowPlatform` 跑 amended flow 的 exit/
  `run-summary.json`。**OBTAINABLE(可得性)**。真跑需 sim/emulator,属 C4,本 checkpoint 不实现,只判可得。

### decomposition「对手」side-by-side:`smix-ai-tier::judge` vs propose 回路

| 段 | `judge`(已证,`smix-ai-tier/src/lib.rs`) | propose 回路(净新实例) |
|---|---|---|
| 输入面 | 单帧 `screenshot_png: &[u8]` + `condition: &str`(:49) | 整 bundle:`run-summary.json` + `step-N.json` + `fail.tree.json` + PNG + `ExpectationFailure`(轴 A) |
| claude 调用 | `Command::new(claude_bin).arg("--tools").arg("Read").arg("-p").arg(prompt).arg("--output-format").arg("text").kill_on_drop(true)` + `timeout`(:137-149) | **复用同范式**:bundle 文件已在盘(json/tree/png path),`--tools Read` 让 claude 直读;`ask()` helper(:137)原样可用。§9#2 本机 claude ✓ |
| 输出解析 | `parse_json_object::<StructuredVerdict>`(:207,容错 `find('{')..rfind('}')`)→ `{pass, reason}` | **同泛型** `parse_json_object::<T>`,T = proposal(edit-op 列表 / `Vec<ProposalEdit>`)。同容错抽取,输出更富 |
| fence 归位 | deletable test / opt-in / non-deterministic,坐 resolver **旁**不在其内(README) | **同 fence**:authoring aid、LLM 非确定、opt-in(`smix authoring propose`)、deletable,**不进 sense/act core**(§9#8 三层:这是 authoring,非感知/操作 core) |

**补强先例**:recorder 侧已有第二条 local-claude 结构化 authoring 产出路径 ——
`RecorderErrorReason::{CleanupFailed, CleanupEmptyOutput, CleanupInvalidOutput}`
(`smix-authoring-ir/src/lib.rs:142`)证 claude CLI 清洗产 authoring 输出已在项目内落地。propose 是同族第三例。

### 综合

- **轴 A OBTAINABLE**:四维 proposal 定位经现成 `--debug-output` ∪ `--format json` 可得,无新 core 能力
  (记录是两 surface 并集,assembler join 是连接工作非能力缺口)。
- **轴 B**:4/5 improvement 类(selector swap / waitFor 插入 / step reorder / verb 改)结构端到端可表达 +
  可反解;1/5(断言改)PARTIAL,受既有断言 Step 词汇上界(枚举已尽)。≥1 类端到端可得满足。
- **轴 C**:well-formedness OBTAINABLE(`parse_flow_yaml` 是现成 device-free oracle;Step→yaml emitter 是
  净新 device-free 代码,非能力缺口);effectiveness OBTAINABLE(可得性,`smix run` 现成,真跑属 C4)。
- 「对手」`smix-ai-tier` 回路逐段可复用:输入更富、claude 调用范式原样、输出解析同泛型、fence 同源归位。

三轴皆可 + ≥1(实为 4)improvement 类端到端可得 → 按 rubric = OBTAINABLE。诚实划界(不 oversell):
本 verdict 证到**能力可得性 + 结构可表达 + device-free 校验 oracle 存在**层;proposal 的 **LLM 质量**
与 **device-replay 有效性**是 C2/C4 的经验问题,非可得性阻塞。**唯一净新建造项 = Step→maestro-yaml
emitter**(扩展 recorder,device-free,非 core 能力缺口),C2 不可假设 serde 免费 round-trip。

VERDICT: OBTAINABLE — 轴 A(观察面)OBTAINABLE、轴 B(proposal 形)4/5 improvement 类 OBTAINABLE + 断言改 PARTIAL、轴 C(验证)well-formedness OBTAINABLE(parse_flow_yaml oracle 现成)+ effectiveness OBTAINABLE(smix run 现成,真跑属 C4)。observation→local-claude→actionable-proposal 回路可得,诚实形 = `--debug-output`∪`--format json` 记录并集 → 复用 ai-tier claude 调用范式 → 结构化 edit-op proposal(落合法 Step/Selector)→ `parse_flow_yaml` device-free 良构 gate → `smix run` device replay 有效性(C4)。唯一净新建造 = 全 Step→maestro-yaml emitter(扩展 smix-recorder,device-free,非 core 能力缺口)。

## Top-N「C2 建造 attack 候选」(不实施,只给 C2 起点)

1. **proposal schema 诚实形**(落 `smix-authoring-ir` 或新 `smix-authoring-propose` stone,复用
   `Selector`/`Step` 类型):edit-op 列表,每 op ∈
   - `ReplaceSelector { step_index: usize, new_selector: Selector }`(selector swap)
   - `InsertStep { before_index: usize, step: Step }`(waitFor 插入,step 限
     `ExtendedWaitUntil`/`WaitForAnimationToEnd`)
   - `ReorderStep { from_index: usize, to_index: usize }`(step reorder)
   - `ReplaceStep { step_index: usize, new_step: Step }`(verb 改 + 断言改的可表达子集)
   **v1 收哪些类**:selector swap / waitFor 插入 / verb 改 3 类(全 OBTAINABLE、reader 收益最高、
   有效性最可判);step reorder 收但标「有效性高风险,须 C4 gate」;断言改**v1 只收可表达子集**
   (改 AssertVisible selector / 翻 expect_visible / 改 webview assert_eq),不臆造 smix 无的断言谓词。
2. **claude 调用范式**:原样复用 `smix-ai-tier::ask`(`lib.rs:137`)—— `--tools Read -p <prompt>
   --output-format text` + `kill_on_drop` + `timeout`;prompt 指向 bundle 目录让 claude 直读
   `run-summary.json`/`fail.tree.json`/PNG(不 stage 单帧,bundle 已在盘)。输出解析复用
   `parse_json_object::<Proposal>`(:207)。§9#2 本机 claude 唯一路径。
3. **验证分层落点**:
   - well-formedness gate(C3,device-free)= apply(proposal, flow) → yaml → `parse_flow_yaml`
     → `Ok`。**先建 Step→maestro-yaml emitter**(扩展 `smix-recorder::generate_maestro_yaml` 到全 Step 集,
     或走 yaml-AST 层 in-place 编辑绕开 emitter);纯逻辑单测(fixture bundle → proposal → 合法 Flow)。
   - effectiveness gate(C4,device replay)= fail flow → propose → apply → `smix run` amended → fail→pass,
     真 sim/emulator。
4. **fence 归位(C5)**:同 `smix-ai-tier` —— deletable test / opt-in(`smix authoring propose`)/
   标 non-deterministic;**不进 resolver / sense / act 路径**(§9#8:authoring aid 非 sense/act core)。

## 与冷计划 / plan-hot 假设不符处(如实列)

- **plan-hot / 冷计划均假设 proposal「反解回合法 flow 编辑」**,未点明 **`Step` serde 序列化形 ≠ parser
  手写读入形**,故「反解」**不是免费 serde round-trip**,需净新 Step→maestro-yaml emitter(现成
  recorder emitter 只覆盖 IRAction 子集)。此为 C2/C3 的实打实建造项,非能力缺口,但计划未列 —— 建议
  C2 热化时显式建 emitter step。
- **观察面「携带定位」在 plan-hot 前置里表述为单一 bundle**;实证是**两 output surface 并集**
  (`--debug-output` bundle 无结构化 selector/visibleElements,须 `--format json` `ExpectationFailure` 补,
  或据 `StepDebugRecord.n` 重解析 flow)。二者同 run 并存(`entry.rs:509-514`),但 C2 assembler 需读两处 join。
- 其余(fenced 先例 = ai-tier judge、`--debug-output` + `StepDebugRecord` + `ExpectationFailure` 现成、
  无 propose 基础设施)与 plan 假设**相符**,已 file:line 证。

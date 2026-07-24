# plan-hot — v2.11 到 C2:proposal schema + 生成核心(device-free)

## 目标 checkpoint

C2:**建成「一次 flow run 的结构化 bundle → 本机 `claude` CLI → 结构化 machine-checkable proposal」的
device-free 生成核心 + 诚实的 proposal schema。** 通过后世界变成:

1. **新 crate `smix-authoring-propose`**(authoring lane 的 steel,fenced 同 `smix-ai-tier`):定义
   proposal schema —— `Proposal { edits: Vec<ProposalEdit> }`,`ProposalEdit` 四变体
   (`ReplaceSelector` / `InsertStep` / `ReorderStep` / `ReplaceStep`),复用真实
   `smix_selector::Selector`(untagged,11 变体)/ `smix_adapter_maestro::Step`(camelCase,~20 变体)
   —— 不臆造 smix 无的谓词。附纯逻辑 `validate(&self, flow_len) -> Result<(), ProposalError>`
   编码 **C1 verdict 的 v1 收敛策**(索引在界 + `InsertStep.step` 限 waitFor 类)。
2. **生成核心** `propose_from_bundle(flow_path, bundle_dir, cfg) -> Result<Proposal, ExpectationFailure>`
   —— **原样复用 `smix-ai-tier` 的 claude 调用范式**(`--tools Read -p <prompt> --output-format text`
   + `kill_on_drop` + `timeout`,把私有 `ask` / `parse_json_object` 提升为 pub 作单一原语),
   prompt 指向 bundle 目录让 claude 直读 `run-summary.json` / `fail.tree.json` / `failure.json` / PNG,
   `parse_json_object::<Proposal>` 抽出结构化 proposal。§9#2 唯一路径 = 本机 `claude`。
3. **测试全 device-free**:proposal schema 纯逻辑单测(fixture JSON → 反序列化 + validate 拒收越界/非法插入)
   + claude 调用范式单测(对 **stub 二进制**,不真调 API,复用 ai-tier `stub_cli` 范式)+
   mock 回复解析单测(prose 包裹 JSON 的容错抽取)。

**边界(诚实划界,不硬塞)**:
- **全 `Step`→maestro-yaml emitter 不在 C2 —— 归 C3。** 理由见下「本段预先定死的口径」。C2 的
  「machine-checkable」= proposal JSON 反序列化落合法 `Step`/`Selector` 变体(serde 免费保证词汇上界)
  + 索引在界 + 插入受限,**全程不产 amended flow yaml**。产 amended flow(apply proposal → emitter → yaml)
  是 C3 well-formedness gate 的定义性工作。
- **真 bundle 的现场装配(跑 smix 双 flag + join 两 surface)不在 C2。** C2 核心消费一个**已在盘的 bundle 目录**
  (fixture 提供全部文件);真跑 smix 产 bundle 属 C4/C5 device replay。这不虚构 wire(v2.9-C5 教训):
  核心只 `--tools Read` 消费真文件 + 真 claude CLI。
- **effectiveness(fail→pass 重跑)不在 C2 —— 归 C4**(需 sim/emulator)。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— C1 verdict 在且 = OBTAINABLE(据其建造)——
grep -qE '^VERDICT: OBTAINABLE' docs/research/c1-llm-authoring-loop.md
# —— proposal schema 要落的真实类型在 ——
grep -q 'pub enum Step' crates/smix-adapter-maestro/src/lib.rs                 # Step(edit-op 落点)
grep -q 'pub enum Selector' crates/smix-selector/src/lib.rs                    # Selector(edit-op 落点)
# —— claude 调用范式 + 输出解析原语在(C2 复用)——
grep -q 'async fn ask' crates/smix-ai-tier/src/lib.rs                          # claude 调用范式(私有,C2 pub 化)
grep -q 'fn parse_json_object' crates/smix-ai-tier/src/lib.rs                  # 泛型 JSON 抽取(私有,C2 pub 化)
grep -q 'fn stub_cli' crates/smix-ai-tier/tests/verdict.rs                     # stub 二进制测试范式(C2 复用)
# —— 失败面 serde(bundle assembler 输入形)在 ——
grep -q 'pub struct ExpectationFailure' crates/smix-error/src/lib.rs
which claude                                                                    # 本机 claude CLI 在(§9#2)
```

全部 exit 0 = 可开工。任一失败 → 按 §6「何时该拒绝热化」回报,不硬开。

## 已经查清、不必重查的事实(C1 证据 + 本机探测,C2 直接引用)

- **claude 调用范式(C2 复用,`smix-ai-tier/src/lib.rs`)**:私有 `async fn ask(prompt, cfg)`(:137)=
  `Command::new(claude_bin).arg("--tools").arg("Read").arg("-p").arg(prompt).arg("--output-format")
  .arg("text").kill_on_drop(true)` + `tokio::time::timeout`(:151);私有泛型
  `fn parse_json_object::<T: DeserializeOwned>(reply) -> Option<T>`(:207)= `reply.find('{')..=rfind('}')`
  → `serde_json::from_str`(容错 prose)。`pub struct AiTierConfig { claude_bin, timeout_secs }`(:14,
  Default = `"claude"` / 60s)。**C2 把 `ask` / `parse_json_object` 提升为 `pub`(加 doc,ai-tier 有
  `#![deny(missing_docs)]`),作单一 claude 原语,不 copy-paste。**
- **stub 二进制测试范式(C2 复用,`smix-ai-tier/tests/verdict.rs:38`)**:`fn stub_cli(dir, body) -> AiTierConfig`
  写 `#!/bin/sh\n{body}` 可执行文件 + `claude_bin` 指向它。C2 的调用范式单测照此:stub `echo '<canned Proposal JSON>'`
  → `propose_from_bundle` 返 `Ok(Proposal)`;stub `exit 1` → `Err`(driver 错,不静默,同 ai-tier `:129` 范式)。
- **edit-op 落的真实类型(C1 轴 B,本机确认)**:
  - `Selector`(`smix-selector/src/lib.rs:324`,`#[serde(untagged)]`,11 变体:`Text`/`Id`/`Label`/`Role`/
    `Focused`/`Anchor`/`LocalizedText`/`OcrText`/…)—— 无显式 tag,字段存在即判别。
  - `Step`(`smix-adapter-maestro/src/lib.rs:330`,`#[serde(rename_all="camelCase")]`,~20 变体):
    `TapOn { selector, optional, dispatch }`(:332)/ `ExtendedWaitUntil { selector, timeout_ms, expect_visible }`(:400)
    / `WaitForAnimationToEnd { ceiling_ms }`(:384)/ `AssertVisible { selector }`(:411)/
    `InputTextInto { selector, text }`(:420)/ `WebViewEval { js, assert_eq }`(:358)/ `ScrollUntilVisible`(:490)…
  - **v1 收敛(C1 Top-N #1,写进 `validate`)**:selector swap(`ReplaceSelector`)/ waitFor 插入
    (`InsertStep`,step **限** `ExtendedWaitUntil`|`WaitForAnimationToEnd`)/ verb 改 + 断言改可表达子集
    (`ReplaceStep`)3 类全 OBTAINABLE;step reorder(`ReorderStep`)结构收但**有效性高风险,标注留 C4 gate**,
    validate 不拦(结构良构)。**断言改不臆造**:smix 断言词汇 = `{AssertVisible, ExtendedWaitUntil.expect_visible,
    WebViewEval.assert_eq}`,词汇上界由 `Step` serde 反序列化**天然强制**(无 `toHaveText`/正则值比较变体 →
    claude 若吐这类,`from_str::<Step>` 直接失败)。故 `ReplaceStep` 无需额外谓词白名单,serde 即边界。
- **StepDebugRecord serde 形(bundle 内容,`runtime.rs:677`,`#[derive(Serialize)]` `#[non_exhaustive]`)**:
  `n: usize`(1-based)/ `verb: String` / `summary` / `verdict∈{ok,skipped,expanded-subflow,failed}` /
  `wall_ms` / `json_path` / `png_path?` / `tree_path?` / `failure_kind?` / `failure_message?`。
  `run-summary.json` 聚合 `steps: [StepDebugRecord]`。C2 fixture bundle 造此形。
- **ExpectationFailure serde 形(失败面,`smix-error/src/lib.rs:72`,`#[serde(rename_all="camelCase")]`)**:
  `{ ok:false, code, message, selector: Option<Selector>, suggestions: [String], visibleElements: [ElementSummary],
  hint?, smixVersion, screenshot?, deviceLog? }`。C2 fixture bundle 写一份 `failure.json` 承此形
  (C1 诚实 nuance:`--format json` 的 ExpectationFailure 走 stdout 非文件,现场装配把它落进 bundle =
  C5 CLI wiring;C2 fixture 直接提供该文件,device-free)。
- **emitter 现状(C1 唯一净新建造,确认归 C3)**:`generate_maestro_yaml(actions: &[IRAction], app_id)`
  (`smix-recorder/src/generator_maestro_yaml.rs:23`)**只吃 `&[IRAction]`**(tap/fill/clear/pressKey/swipe/
  goBack/waitFor/hideKeyboard,8 变体),**非 `&[Step]`**。全 `Step`→maestro-yaml emitter 不存在 —— C3 建。
- **良构 oracle(C3 用,确认签名)**:`parse_flow_yaml(yaml: &str) -> Result<Flow, ParseError>`
  (`smix-adapter-maestro/src/parser.rs:2942`)纯函数、device-free。C2 不碰(C2 不产 yaml)。
- **crate 落点 = 新 crate `smix-authoring-propose`(§9#8 + steel-cement-stone 依据,见下口径)**。workspace
  `members = ["crates/*"]`(`Cargo.toml:5`),新 crate 自动纳入。`smix-adapter-maestro → smix-ai-tier` 依赖已存在
  (无环:authoring-propose 坐两者之上)。

## 本段预先定死的口径(防 scope 漂移与自欺)

- **emitter 放 C3,不放 C2 —— 理由**:C2 冷计划概要 = 「产 machine-checkable proposal」,不含「落 flow」。
  proposal 的 machine-checkable 性由 serde(edit-op 落合法 `Step`/`Selector` 变体)+ validate(索引在界 +
  插入受限)**完全 device-free 证得**,无需产 amended flow。emitter(apply proposal → yaml)只在 C3 的
  well-formedness gate(amended flow 反解 → `parse_flow_yaml` 接受)才需要。把 emitter 拉进 C2 = 提前干 C3 的活、
  撑爆 C2「产数据结构」边界。C1 明许此分法(「C2 只到产 proposal 数据结构不落 flow → emitter 可留 C3」)。
- **crate 落 `smix-authoring-propose`(新 crate),不落 smix-cli / smix-ai-tier —— 理由(§9#8 + 三分类)**:
  - **§9#8 三层**:proposal 生成 = **authoring aid**,非 sense/act core,须 deletable / opt-in / non-deterministic,
    fence 同 `smix-ai-tier`。独立 crate 把 fence 铸成**编译期边界**(sense/act 无一依赖它),镜像 ai-tier 自身隔离
    (「nothing that senses may depend on this crate」)。
  - **steel-cement-stone**:propose 引擎知领域模型(Step/Selector/flow)但不绑单条业务流,是 **steel**;埋进
    smix-cli 会把可复用 steel 沉进 CLI binary 的 **cement**(§13 反模式)。C5 只在 CLI 挂 `smix authoring propose`
    薄 wire 调此 crate。
  - **不落 smix-ai-tier**:ai-tier charter = 「AI-assertion tier」(assertCondition/extractWithAI,sense-adjacent
    的**断言** lane),把 authoring lane 塞进去混淆两 lane。C2 只从 ai-tier **借 claude 原语**(pub 化 ask/parse_json_object),
    不喧宾。
- **别造虚构 wire**(v2.9-C5 教训):C2 核心只消费**真** bundle 文件(fixture 提供)+ **真** claude CLI
  (调用范式测走 stub 二进制)。不新造 route、不假设 serde 免费 round-trip。
- **§9#2**:全程本机 `claude`,网络 Claude API 路径不碰。

## 步骤(线性,2 个)

### S1. proposal schema:edit-op 类型 + v1 收敛策 validate(纯逻辑,device-free)

**红(写测试)**
- 文件:`crates/smix-authoring-propose/tests/schema.rs`
- 断言(4 个 test,咬真实类型):
  1. `proposal_deserializes_four_edit_ops` — fixture JSON(含 `ReplaceSelector{step_index, new_selector:{id:...}}`
     / `InsertStep{before_index, step:{extendedWaitUntil:{...}}}` / `ReorderStep{from_index, to_index}` /
     `ReplaceStep{step_index, new_step:{tapOn:{...}}}`)→ `serde_json::from_str::<Proposal>` 成功,`edits.len()==4`,
     各变体命中 + 内嵌 `Selector`/`Step` 落合法变体(如 `new_selector` 判为 `Selector::Id`)。
  2. `validate_rejects_out_of_range_index` — `ReplaceSelector{step_index: 9,..}` 对 `flow_len=3` → `validate` 返 `Err`。
  3. `validate_rejects_insertstep_non_wait` — `InsertStep{step: Step::TapOn{..}}` → `Err`(v1 策:插入限
     `ExtendedWaitUntil`|`WaitForAnimationToEnd`)。
  4. `validate_accepts_v1_classes` — selector swap + waitFor 插入(`ExtendedWaitUntil`)+ verb 改
     (`ReplaceStep`→`ExtendedWaitUntil`)+ 断言改(`ReplaceStep`→`AssertVisible` 换 selector / 翻 `expect_visible`)
     + step reorder → 全 `validate` 返 `Ok`(reorder 结构良构,有效性风险留 C4)。
- 跑红(须先失败一次:crate/类型未建 → 编译失败):
  ```bash
  cargo test -p smix-authoring-propose --test schema
  ```
  期望:红(`error: package ID ... did not match` 或 类型未定义编译错)。

**绿(实现)**
- 文件:`crates/smix-authoring-propose/Cargo.toml` — 新 crate。deps:`smix-selector`(Selector)/
  `smix-adapter-maestro`(Step;C3 起加 parse_flow_yaml)/ `smix-error`(ExpectationFailure)/ `serde` / `serde_json`。
  (ai-tier dep 在 S2 加。)
- 文件:`crates/smix-authoring-propose/src/lib.rs`
- API:
  ```rust
  pub struct Proposal { pub edits: Vec<ProposalEdit> }          // #[derive(Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]                            // 与 Step/Selector 同 camelCase 契约
  pub enum ProposalEdit {
      ReplaceSelector { step_index: usize, new_selector: Selector },
      InsertStep { before_index: usize, step: Step },
      ReorderStep { from_index: usize, to_index: usize },
      ReplaceStep { step_index: usize, new_step: Step },
  }
  pub enum ProposalError { IndexOutOfRange { .. }, InsertNotWait { .. } }
  impl Proposal { pub fn validate(&self, flow_len: usize) -> Result<(), ProposalError> }
  ```
- 关键点:①`ProposalEdit` serde 标签形选**内部 tag**(`#[serde(tag="op", rename_all="camelCase")]`)或
  externally-tagged,**测试 fixture 与之对齐**(选定后写进 doc,别两处不一致);②`validate`:所有携 index 的 op
  index `< flow_len`(InsertStep `before_index <= flow_len`),越界 `IndexOutOfRange`;`InsertStep.step`
  非 `ExtendedWaitUntil`/`WaitForAnimationToEnd` → `InsertNotWait`;`ReplaceStep`/`ReplaceSelector` 词汇上界
  由 serde 天然强制,validate 不加谓词白名单;③`ReorderStep` validate 通过(结构层),risk 属 C4。
- 跑绿:上红命令转绿,`4 passed`。

**重构(可选)**
- 无。

### S2. 生成核心:bundle → claude(ai-tier 范式)→ Proposal(device-free,stub 二进制)

**红(写测试)**
- 文件:`crates/smix-ai-tier/tests/verdict.rs`(或新 `tests/primitive.rs`)—— 加 1 个 test 证 pub 原语:
  `ask_pub_runs_stub`(stub `echo hi` → `smix_ai_tier::ask("p", &cfg)` 返 `Ok("hi\n")`)。第一次红(`ask` 私有,
  `smix_ai_tier::ask` 不可见)。
- 文件:`crates/smix-authoring-propose/tests/generate.rs`(3 个 test):
  1. `parse_proposal_reply_tolerates_prose` — `"Sure! {\"edits\":[{...}]}"` → `parse_proposal_reply(reply)` 返
     `Some(Proposal)`(复用 pub `parse_json_object`)。
  2. `propose_from_bundle_parses_stub_reply` — `stub_cli` 范式:stub `printf '<canned Proposal JSON>'`;fixture
     bundle 临时目录写 `run-summary.json`+`failure.json`(StepDebugRecord/ExpectationFailure 形)+ fixture flow 文件 →
     `propose_from_bundle(flow, bundle, &cfg).await` 返 `Ok(Proposal)`,`edits` 命中期望变体。
  3. `propose_from_bundle_surfaces_cli_failure` — stub `exit 1` → `Err(ExpectationFailure)`(driver 错,不静默,
     不塌成空 proposal)。
- 跑红(须先失败:`ask` 私有 + `propose_from_bundle` 未建 → 编译失败):
  ```bash
  cargo test -p smix-ai-tier ask_pub_runs_stub
  cargo test -p smix-authoring-propose --test generate
  ```
  期望:红。

**绿(实现)**
- 文件:`crates/smix-ai-tier/src/lib.rs` — `ask` / `parse_json_object` 由 `fn`/`async fn` 提升为 `pub`(加 doc
  满足 `#![deny(missing_docs)]`);签名、行为**不动**(向后兼容,judge/extract 内部调用不变)。
- 文件:`crates/smix-authoring-propose/Cargo.toml` — 加 `smix-ai-tier` dep + `tokio`(process/time)+ dev
  `tempfile`(fixture bundle)。
- 文件:`crates/smix-authoring-propose/src/lib.rs`
- API:
  ```rust
  pub fn parse_proposal_reply(reply: &str) -> Option<Proposal>;   // = smix_ai_tier::parse_json_object::<Proposal>
  pub async fn propose_from_bundle(
      flow_path: &std::path::Path,
      bundle_dir: &std::path::Path,
      cfg: &smix_ai_tier::AiTierConfig,
  ) -> Result<Proposal, smix_error::ExpectationFailure>;
  ```
- 关键点:①`propose_from_bundle` 构 prompt = 指令 claude `Read` `bundle_dir` 下 `run-summary.json` /
  `failure.json` / `*.fail.tree.json` / PNG **及** `flow_path` 原 flow,产「one JSON object:`{edits:[...]}`」
  (schema 说明写进 prompt,同 judge/extract 的 few-shot 形);② `let reply = smix_ai_tier::ask(prompt, cfg).await?;`
  → `parse_proposal_reply(&reply).ok_or_else(|| driver_error(...))`(claude 答但非期望形 = driver 错,不塌成
  「无改进」,同 ai-tier `:66` 范式);③**不** validate(schema 层 C3 apply 时再 validate,或此处可选 validate ——
  本 step 只到「产 Proposal」,validate 已在 S1 单测覆盖,生成核心不重复 gate);④§9#2:唯一外部调用 = 本机 claude。
- 跑绿:上两条红命令转绿(`ask_pub_runs_stub` 过;generate `3 passed`)。

**重构(可选)**
- 若 `driver_error` helper 在两 crate 重复,`smix-authoring-propose` 自建本地 helper(不 pub 化 ai-tier 私有
  `driver_error`,避免过度暴露 error 构造面);不改行为。

## Checkpoint C2 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

# —— schema(纯逻辑,device-free)——
cargo test -p smix-authoring-propose --test schema \
  && cargo test -p smix-authoring-propose --test generate \
  && cargo test -p smix-ai-tier ask_pub_runs_stub \
  && cargo build -p smix-authoring-propose \
  && echo GATE-C2-PASS
```

期望:各命令 exit 0 且打印 `GATE-C2-PASS`。分项含义(机器可判,零人工读图、零设备):
- `--test schema`:`4 passed`(proposal 四 edit-op 反序列化落合法 Step/Selector + validate 拒越界 + 拒非-wait 插入 + 收 v1 五操作)。
- `--test generate`:`3 passed`(prose 容错解析 + stub 二进制产 `Ok(Proposal)` + stub `exit 1` 产 `Err` 不静默)。
- `ask_pub_runs_stub`:`1 passed`(ai-tier claude 原语已 pub 化、行为不变,stub 二进制验)。
- `cargo build -p smix-authoring-propose`:新 crate 编译干净(纳入 workspace)。

**不在 C2 验收内(诚实划界)**:
- 全 `Step`→maestro-yaml **emitter**(apply proposal → yaml)+ well-formedness gate(`parse_flow_yaml` 接受)→ **C3**。
- 真调 claude 产 proposal 的质量 / 真 bundle 现场装配(smix 双 flag → join 两 surface)→ C4/C5(现场 wiring + device)。
- effectiveness(amended flow fail→pass 重跑)→ **C4**(sim/emulator)。
- `smix authoring propose` CLI 挂载 + ai-tier 同源 deletability/fence 出口 → **C5**。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.11-c2-hot.md`。
2. **两条架构决策写入 `docs/v2.md` 决策日志**(§10):
   - `{date}` proposal schema + 生成核心落**新 crate `smix-authoring-propose`**(非 smix-cli/smix-ai-tier)。
     理由:§9#8 authoring aid 须 fenced 编译期边界(sense/act 无依赖)、steel 不沉 CLI cement、不混 ai-tier 断言 lane。
   - `{date}` 全 `Step`→maestro-yaml **emitter 归 C3**(非 C2)。理由:C2 machine-checkable 由 serde+validate
     device-free 证得,不需产 amended flow;emitter 只 C3 well-formedness gate 需。
3. **§9#2 网络路径不变量**:C2 全程本机 `claude`(pub 化 ai-tier 原语);网络 Claude API 路径未碰,待用户单独拍板。
4. C2 验收通过 + 用户/上层明确「开始 C3」→ 调 sub-agent 热化 C3(proposal 良构 gate:建全 `Step`→maestro-yaml
   emitter + apply proposal → yaml → `parse_flow_yaml` device-free 单测),见 CLAUDE.md §6。发布顺延待授权,不自作主张 publish。

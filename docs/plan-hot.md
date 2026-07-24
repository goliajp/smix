# plan-hot — v2.11 到 C3:proposal 良构 gate（device-free）

## 目标 checkpoint

C3:**建成「apply proposal → amended flow → 反解为合法 maestro yaml → `parse_flow_yaml` 忠实接受」的
device-free 良构硬门。** 通过后世界变成:

1. **全 `Step`→maestro-yaml emitter**（净新,落 `smix-adapter-maestro`,`parse_flow_yaml` 的逆）:
   `emit_flow_yaml(steps: &[Step], app_id: &str) -> Result<String, EmitError>`。**对 `Step` enum 穷尽
   match**(新变体不能静默漏);覆盖 round-trip 核心集,核心集外变体返 **显式 `EmitError::Unsupported`**
   (镜像现成 `generate_maestro_yaml` 的 `maestro_unsupported` refuse 门,不静默产会崩的 yaml)。
2. **apply proposal → amended flow**（落 `smix-authoring-propose`,它已 own `Proposal` + 依赖 `Step`）:
   `apply(proposal: &Proposal, steps: &[Step]) -> Result<Vec<Step>, ApplyError>`。**先 `validate`(C2 已有)**,
   再对 `Vec<Step>` apply 四 op(ReplaceSelector 换 selector / InsertStep 插入 / ReorderStep 移位 / ReplaceStep 换 step)。
3. **well-formedness 硬门**（device-free 纯逻辑单测）:fixture flow(round-trip 核心集 Step)+ fixture proposal
   → `apply` → amended `Vec<Step>` → `emit_flow_yaml` → yaml → `parse_flow_yaml(yaml)` 返 `Ok(Flow)` **且**
   `Flow.steps == amended`(忠实 round-trip,`Step: PartialEq` 已有)。零 claude、零 sim。

**边界(诚实划界,不硬塞)**:
- **effectiveness(amended flow 真跑 fail→pass)不在 C3 —— 归 C4**(需 sim/emulator)。C3 只证**良构**(parser 接受 + 步骤忠实),不证有效。
- **真调 claude 产 proposal 不在 C3**(C2 生成核心已 stub 覆盖;真 claude 属 C4/C5)。C3 全用 fixture proposal(手构 `Proposal` 值)。
- **`smix authoring propose` CLI 挂载不在 C3 —— 归 C5。**
- **emitter 全 Step 覆盖不在 C3**:C3 emitter 只保证 **round-trip 核心集**忠实,核心集外(AI-gated 步、非-Id `InputTextInto`、`Focused`/`Fallback`/regex selector 承载步、复杂/罕见 verb)**显式 refuse + 记 gap**,不臆造全覆盖(见「本段预先定死的口径」round-trip 可行性)。全覆盖 emitter 待后续 proposal 真需要时扩。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— C2 已闭合:proposal schema + validate + 生成核心在 ——
grep -q 'pub struct Proposal' crates/smix-authoring-propose/src/lib.rs                  # C3 apply 的输入类型
grep -q 'pub fn validate' crates/smix-authoring-propose/src/lib.rs                      # apply 先调的 gate(C2)
grep -q 'pub async fn propose_from_bundle' crates/smix-authoring-propose/src/lib.rs     # C2 生成核心在(不动)
# —— emitter 要落的逆:parse_flow_yaml + Flow + Step 在 ——
grep -q 'pub fn parse_flow_yaml' crates/smix-adapter-maestro/src/parser.rs              # 良构 oracle(C3 emitter 的逆)
grep -q 'pub struct Flow' crates/smix-adapter-maestro/src/lib.rs                        # parse 返回类型(steps 在此)
grep -q 'pub enum Step' crates/smix-adapter-maestro/src/lib.rs                          # emitter 穷尽 match 的对象
# —— emitter 净新确认:adapter-maestro 无现成 Step→yaml ——
! grep -q 'pub fn emit_flow_yaml' crates/smix-adapter-maestro/src/parser.rs crates/smix-adapter-maestro/src/lib.rs
# —— 无依赖环:adapter-maestro 不依赖 authoring-propose/recorder ——
! grep -qE 'smix-authoring-propose|smix-recorder' crates/smix-adapter-maestro/Cargo.toml
```

全部 exit 0 = 可开工。任一失败 → 按 §6「何时该拒绝热化」回报,不硬开。

## 已经查清、不必重查的事实(C1/C2 + 本机探测,C3 直接引用)

- **emitter 现状 = 净新(本机探测确认)**:`grep -rn 'to_yaml\|to_maestro\|Display.*for.*Step\|emit_flow_yaml'`
  在 `smix-adapter-maestro/` 零 Step→yaml 命中(仅 `emit_junit`/`emit_json_*` 是无关 report emitter)。现成
  `generate_maestro_yaml`(`smix-recorder/src/generator_maestro_yaml.rs:23`)**只吃 `&[IRAction]`(8 变体)**,
  非 `&[Step]`,且落 recorder 非 adapter-maestro。C3 emitter 是 adapter-maestro 内**净新**函数。
- **emitter 的正统建法(参照 `generate_maestro_yaml` 但不复用)**:`generate_maestro_yaml` **不靠 `Step` serde** ——
  它手工建 `serde_norway::Value`(yaml AST)节点产 maestro 形(`tapOn: {id: x}` / `extendedWaitUntil: {visible: ..., timeout: N}`),
  再 `serde_norway::to_string`。C3 emitter 照此:**对 `Step` 穷尽 match,每变体手建 maestro-form yaml AST**,
  envelope = `appId: <id>\n---\n<steps>`(与 `generate_maestro_yaml:47-69` 同封套)。C1 已揪出「`Step` serde
  camelCase 序列化形 ≠ parser 手写读入形」,故**不能靠 serde 免费 round-trip**,必须手建 maestro 形。
- **crate 落点 + §9#8 依据(本机 dep 图确认无环)**:
  - **emitter → `smix-adapter-maestro`**:它是 `parse_flow_yaml` 的**逆**,与 `Step`/`Flow` 同 crate、同类型、
    紧邻要满足的 parser。steel-cement-stone = 领域感知(知 Step/Selector/maestro 形)、可复用(recorder/propose/migrate
    都可用)、不绑单业务流 = **steel**;放这里**不新增依赖边**(adapter-maestro 已 own 一切所需)。**不落 recorder**:
    recorder emitter 是 IRAction 面,把 Step-emitter 塞 recorder 要 recorder→adapter-maestro 额外耦合 + 拆散 parse/emit 对。
    **不落 authoring-propose**:通用 maestro 序列化原语**不是** authoring aid,埋进 fenced crate = 把通用 steel 藏在 fence 后(违 §9#8 精神:emitter 不感知不操作、也不 authoring,它是 maestro serializer)。
  - **apply → `smix-authoring-propose`**:它 own `Proposal`(C2)、已依赖 `smix-adapter-maestro`(用 `Step`,
    `Cargo.toml` 确认)、`validate` 已在此。apply = validate-then-mutate `Vec<Step>`,是 authoring 回路自身逻辑,归此。
    依赖:authoring-propose → adapter-maestro 已存在,emitter 落 adapter-maestro 后 authoring-propose 直接调,**无环**
    (adapter-maestro 不依赖 authoring-propose/recorder,本机 grep 确认)。
- **良构 oracle(不动,C3 直接喂)**:`parse_flow_yaml(yaml: &str) -> Result<Flow, ParseError>`
  (`smix-adapter-maestro/src/parser.rs:2942`),纯函数、device-free。`Flow.steps: Vec<Step>`(`lib.rs:1125`)。
- **`Step` 忠实 round-trip 可比**:`Step` derive `PartialEq`(`lib.rs:328`),故 gate 可断 `parse(emit(steps)).steps == steps`
  (强门 = 步骤级相等,不止「parse 成 Ok」)。
- **`Proposal`/`ProposalEdit`/`validate` 真实形(C2 产物,`smix-authoring-propose/src/lib.rs`)**:`ProposalEdit`
  内部 tag on `op`(`replaceSelector`/`insertStep`/`reorderStep`/`replaceStep`),payload 字段 snake_case
  (`step_index`/`before_index`/`new_selector`/`new_step`/`from_index`/`to_index`)。`validate(flow_len)`:索引在界
  (`InsertStep.before_index <= flow_len`)+ `InsertStep.step` 限 `ExtendedWaitUntil`|`WaitForAnimationToEnd`。apply **先调它**。

## 本段预先定死的口径(防 scope 漂移与自欺)

### round-trip 可行性(C3 关键风险,已 read-only 穷尽探测)

`parse_flow_yaml` 认的 verb 键**极宽**(`parser.rs:2622-2709` dispatch 表 ~48 键:tapOn / waitForAnimationToEnd /
extendedWaitUntil / assertVisible / inputText / pressKey / back / runFlow / scrollUntilVisible / eraseText / swipe /
launchApp / openLink / stopApp / clearAppData / resetAppData / clearUserDefaults / scroll / hideKeyboard / assertNotVisible /
killApp / clearState / clearKeychain / takeScreenshot / setClipboard / pasteText / copyTextFrom / doubleTapOn / repeatTap /
longPressOn / assertTrue / repeat / retry / runScript / evalScript / webViewEval / setLocation / travel / setPermissions /
addMedia / setOrientation / startRecording / stopRecording / assertScreenshot / assertCondition / extractWithAI / expect /
expectLogClean / fixture）。**但**存在**多变体共享 verb + payload 路由**与**不可忠实 round-trip 的洞**,C3 emitter 覆盖据此收敛(no-ceiling-words,逐条枚举,不 hand-wave「结构性拿不到」):

- **共 verb / payload 路由(emitter 必须产对 payload 形,否则 parse 回错变体)**:
  - `Step::InputText(String)` ↔ scalar `inputText: "s"`;`Step::InputTextInto{selector,text}` ↔ map `inputText: {id, text}`
    (`parse_input_text` `parser.rs:1131-1177`)。**`InputTextInto` 只对 `Selector::Id` 忠实** —— parser map 分支
    **硬编 `Selector::Id`**(`:1164`),非-Id selector 无法从 `inputText` map 反解回来。
  - `Step::TapOn` ↔ `tapOn: <selector>`;`Step::TapAtPoint` ↔ `tapOn: {point:"X%,Y%"}`(共 `tapOn`,按 payload 路由)。
  - `Step::RunFlow` / `RunFlowConditional` / `RunFlowInline` 共 `runFlow`(按 file/when/commands 路由)。
- **不可忠实 round-trip 的洞(C3 emitter 显式 refuse,记 gap,不臆造覆盖)**:
  - **AI-gated 步** `assertCondition`/`extractWithAI` 经 `ai_gate`(`:2691-2698`),AI 未启用时 **parse 直接 reject** →
    device-free gate 里不可 round-trip。(不在 v1 proposal 词汇,天然不产。)
  - **selector 承载洞**(现成 emitter 已注 `maestro_unsupported`):`Selector::Focused` / `Selector::Fallback` 无 maestro 拼法;
    regex selector 有损(re-read 成字面)。承载这类 selector 的步不可忠实 round-trip。
  - **非-Id `InputTextInto`**(上述):emit 成 `inputText: {id,...}` 会丢非-Id selector。
- **C3 emitter round-trip 核心集(收敛,fixture + v1 proposal-introducible 全含)**:`LaunchApp` / `TapOn`(id/text/label
  selector,default optional/dispatch)/ `InputTextInto`(Id)/ `InputText` / `AssertVisible` / `AssertNotVisible` /
  `ExtendedWaitUntil` / `WaitForAnimationToEnd` / `Back` / `PressKey` / `EraseText` / `Swipe` / `ScrollUntilVisible` /
  `StopApp` / `HideKeyboard` / `Scroll`。**emitter 对 `Step` 穷尽 match**,核心集外一律 `Err(EmitError::Unsupported{verb})` ——
  **显式 refuse 优于静默产崩 yaml**(§9#8 不静默降级 + 现成 `generate_maestro_yaml` 同门风格)。
- **gap 记哪**:核心集外 refuse 清单 + 三洞进 `docs/v2.md` 决策日志一行(见「完成后动作」),供后续 proposal 真需要时扩 emitter。

### 其它口径

- **emit 输入 = `&[Step] + app_id`(镜像 `generate_maestro_yaml(actions, app_id)`)**,不吃 `&Flow`:与现成 emitter 同签名形状,gate 侧经 `parse_flow_yaml` 重建 `Flow` 取 `.steps` 比对即可。Flow 级 wrapper(带 app/launch_activity)非 C3 必需,不做。
- **apply 不改 C2 `propose_from_bundle`**:C2 生成核心原样不动,C3 只新增 `apply` + emitter。
- **别造虚构 wire**(v2.9-C5 教训):C3 全 fixture(手构 `Proposal` + `Vec<Step>`)+ 现成 `parse_flow_yaml`,零 route 新造、零 claude、零 sim。
- **§9#2 / §9#8**:C3 不碰网络,不进 sense/act,emitter 是纯 device-free 序列化。

## 步骤(线性,3 个)

### S1. 全 `Step`→maestro-yaml emitter(净新,落 smix-adapter-maestro,穷尽 match + 显式 refuse)

**红(写测试)**
- 文件:`crates/smix-adapter-maestro/tests/emit_roundtrip.rs`
- 断言(3 个 test,咬 round-trip):
  1. `emit_core_steps_round_trip` — 对 round-trip 核心集造一条 `Vec<Step>`(至少含 `LaunchApp` / `TapOn{Id}` /
     `InputTextInto{Id}` / `AssertVisible{Text}` / `ExtendedWaitUntil{Id, timeout_ms, expect_visible}` /
     `WaitForAnimationToEnd` / `Back` / `Swipe` / `ScrollUntilVisible`)→ `emit_flow_yaml(&steps, "com.x")` 返 `Ok(yaml)`
     → `parse_flow_yaml(&yaml)` 返 `Ok(flow)` → `assert_eq!(flow.steps, steps)`(忠实 round-trip,步骤级相等)。
  2. `emit_refuses_out_of_core_variant` — 造一个核心集外变体(如 `Step::AssertCondition{..}` 或
     `Step::RepeatTap{..}`)→ `emit_flow_yaml` 返 `Err(EmitError::Unsupported{..})`(显式 refuse,不 panic 不静默产)。
  3. `emit_refuses_unsupported_selector` — `Step::TapOn{selector: Selector::Focused{..}, ..}` →
     `Err(EmitError::Unsupported{..})`(selector 洞显式 refuse,镜像现成 `maestro_unsupported`)。
- 跑红(须先失败一次:`emit_flow_yaml`/`EmitError` 未建 → 编译失败):
  ```bash
  cargo test -p smix-adapter-maestro --test emit_roundtrip
  ```
  期望:红(`cannot find function emit_flow_yaml` / `EmitError` 未定义 编译错)。

**绿(实现)**
- 文件:`crates/smix-adapter-maestro/src/lib.rs`(或新 `src/emitter.rs`,`pub use` 出)
- API:
  ```rust
  pub fn emit_flow_yaml(steps: &[Step], app_id: &str) -> Result<String, EmitError>;
  #[derive(Debug, thiserror::Error)]
  pub enum EmitError { /* Unsupported { verb: &'static str }, Serialize(String) */ }
  ```
- 关键点:①**对 `Step` 穷尽 match**(编译器强制新变体不漏);核心集每变体手建 maestro-form `serde_norway::Value`
  (`tapOn: <sel>` / `inputText: {id,text}` / `assertVisible: <sel>` / `extendedWaitUntil: {visible|notVisible: <sel>, timeout: N}` /
  `waitForAnimationToEnd: {timeout: N}` / `back` bare / `swipe: {...}` / `scrollUntilVisible: {...}` / `launchApp: {...}` …),
  **payload 形对齐 parser 路由**(见口径:`inputText` scalar vs `{id,text}` map;`extendedWaitUntil` 的 visible/notVisible arm
  由 `expect_visible` 选);②selector→yaml 复用/参照 `generate_maestro_yaml::serialize_selector` 的 id/label/text 形 +
  `maestro_unsupported` refuse(Focused/Fallback);非-Id `InputTextInto` 走 `Unsupported`;③核心集外变体 →
  `Err(EmitError::Unsupported{verb: step_verb(step)})`;④envelope = `appId: <id>\n---\n<body>`(同 `generate_maestro_yaml:47-69`)。
- 跑绿:上红命令转绿,`3 passed`。

**重构(可选)**
- 若 selector→yaml 与 recorder 的 `serialize_selector` 实质重复,不跨 crate 强抽(recorder 依赖 adapter-maestro,反向抽会造环);adapter-maestro 内自建本地 `selector_to_maestro`,不改行为。

### S2. apply proposal → amended flow(落 smix-authoring-propose,validate-then-mutate)

**红(写测试)**
- 文件:`crates/smix-authoring-propose/tests/apply.rs`
- 断言(4 个 test,咬四 op + validate 传播):
  1. `apply_replace_selector_swaps` — `ReplaceSelector{step_index:1, new_selector: Id}` 对 3-step flow → amended[1] 的
     selector 变为新值,其余不变,`len==3`。
  2. `apply_insert_step_inserts_before` — `InsertStep{before_index:1, step: ExtendedWaitUntil{..}}` → amended `len==4`,
     `amended[1]` 是插入的 wait 步,原[1] 后移。
  3. `apply_reorder_moves` — `ReorderStep{from_index:0, to_index:2}` → amended 是原序列对应移位。
  4. `apply_rejects_invalid_via_validate` — `ReplaceSelector{step_index:9,..}` 对 3-step flow → `apply` 返 `Err(ApplyError)`
     (validate 越界经 apply 传播,不静默跳过)。
- 跑红(须先失败:`apply`/`ApplyError` 未建 → 编译失败):
  ```bash
  cargo test -p smix-authoring-propose --test apply
  ```
  期望:红。

**绿(实现)**
- 文件:`crates/smix-authoring-propose/src/lib.rs`
- API:
  ```rust
  pub fn apply(proposal: &Proposal, steps: &[Step]) -> Result<Vec<Step>, ApplyError>;
  #[derive(Debug)] pub enum ApplyError { Invalid(ProposalError) }   // 或 thiserror 包 ProposalError
  ```
- 关键点:①`apply` 起手 `proposal.validate(steps.len()).map_err(ApplyError::Invalid)?`(先 gate 再动);
  ②`let mut out = steps.to_vec();` 依 `edits` 顺序 apply:`ReplaceSelector`→`set_step_selector(&mut out[i], sel)`
  (对 selector-bearing 变体换 `selector` 字段;非 selector-bearing 步的 ReplaceSelector 是越界语义 → 归 `ApplyError`,
  或 validate 阶段已挡——**本 step 选:apply 侧对无 selector 字段的目标步返 `ApplyError`,不静默 no-op**);
  `InsertStep`→`out.insert(before_index, step)`;`ReorderStep`→`let s = out.remove(from); out.insert(to, s)`;
  `ReplaceStep`→`out[i] = new_step`;③不跑 emitter、不跑 parse(纯 `Vec<Step>` 变换),device-free。
- 跑绿:上红命令转绿,`4 passed`。

**重构(可选)**
- 无。

### S3. well-formedness 硬门:apply → emit → parse round-trip(device-free 单测,C3 headline)

**红(写测试)**
- 文件:`crates/smix-authoring-propose/tests/wellformed.rs`
- 断言(2 个 test):
  1. `amended_flow_round_trips_to_legal_flow` — fixture flow(round-trip 核心集 `Vec<Step>`,app_id `"com.x"`)+
     fixture `Proposal`(四 op 各一,用 v1-合法 step/selector:`ReplaceSelector`→Id / `InsertStep`→`ExtendedWaitUntil` /
     `ReorderStep` / `ReplaceStep`→`AssertVisible`)→ `apply` → amended → `emit_flow_yaml(&amended, "com.x")` 返 `Ok(yaml)`
     → `parse_flow_yaml(&yaml)` 返 `Ok(flow)` **且** `assert_eq!(flow.steps, amended)`(良构 + 忠实)。
  2. `wellformed_gate_holds_for_insert_and_reorder_only` — 仅 `InsertStep`(wait)+ `ReorderStep` 的 proposal →
     apply → emit → parse → `Ok` + `flow.steps == amended`(证结构 op 单独也良构 round-trip)。
- 跑红(须先失败:S1 emitter + S2 apply 若未落 → 编译失败;或断言未满足):
  ```bash
  cargo test -p smix-authoring-propose --test wellformed
  ```
  期望:红。

**绿(实现)**
- 无新生产代码 —— 本 step 是 S1(emitter)+ S2(apply)的**组合门**,绿由二者正确性给出。若红暴露 emitter/apply
  某变体 round-trip 不忠实 → 回对应 step 修(**不在 wellformed 里打补丁绕过**,§13 补根因)。
- 跑绿:`2 passed`。

**重构(可选)**
- 无。

## Checkpoint C3 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

cargo test -p smix-adapter-maestro --test emit_roundtrip \
  && cargo test -p smix-authoring-propose --test apply \
  && cargo test -p smix-authoring-propose --test wellformed \
  && cargo build -p smix-authoring-propose \
  && echo GATE-C3-PASS
```

期望:各命令 exit 0 且打印 `GATE-C3-PASS`。分项含义(机器可判,零人工读图、零设备、零 claude):
- `--test emit_roundtrip`:`3 passed` —— 核心集 `Vec<Step>` `emit_flow_yaml` → `parse_flow_yaml` → `Ok(Flow)` 且
  `flow.steps == steps`(忠实 round-trip);核心集外变体 + 不可拼 selector **显式 `Err(EmitError::Unsupported)`**(不静默)。
- `--test apply`:`4 passed` —— 四 op 正确变换 `Vec<Step>`;`validate` 越界经 `apply` 传播成 `Err`(不静默跳过)。
- `--test wellformed`:`2 passed` —— **C3 headline**:fixture flow + fixture proposal → apply → emit → `parse_flow_yaml`
  返 `Ok(Flow)` 且 `flow.steps == amended`(良构 + 忠实 round-trip);结构-only proposal 亦成立。
- `cargo build -p smix-authoring-propose`:crate 干净编译(新增 `apply`/`ApplyError`,依赖 adapter-maestro 新 `emit_flow_yaml`,无环)。

**不在 C3 验收内(诚实划界)**:
- effectiveness(amended flow 真跑 fail→pass 重跑)→ **C4**(sim/emulator)。
- 真调 claude 产 proposal 的质量 / 真 bundle 现场装配 → C4/C5。
- emitter **全 Step 覆盖**(核心集外 verb / AI-gated 步 / 非-Id InputTextInto / Focused·Fallback·regex selector)→ 后续按需扩,C3 只保证核心集忠实 + 显式 refuse。
- `smix authoring propose` CLI 挂载 + ai-tier 同源 deletability/fence 出口 → **C5**。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.11-c3-hot.md`。
2. **架构 + gap 决策写入 `docs/v2.md` 决策日志**(§10):
   - `{date}` 全 `Step`→maestro-yaml **emitter 落 `smix-adapter-maestro`**(`parse_flow_yaml` 的逆,steel 紧邻 parser,无新依赖边、无环)。理由:§9#8 通用 maestro serializer 非 authoring aid、不埋 fenced crate;steel-cement-stone 领域感知可复用 = steel;不落 recorder(拆散 parse/emit 对 + 反向耦合)。
   - `{date}` **apply proposal 落 `smix-authoring-propose`**(own Proposal + 已依赖 adapter-maestro),validate-then-mutate `Vec<Step>`。
   - `{date}` **C3 emitter round-trip 范围收敛 + gap**:核心集 = {LaunchApp/TapOn(id·text·label)/InputTextInto(Id)/InputText/AssertVisible/AssertNotVisible/ExtendedWaitUntil/WaitForAnimationToEnd/Back/PressKey/EraseText/Swipe/ScrollUntilVisible/StopApp/HideKeyboard/Scroll};核心集外(AI-gated 步、非-Id InputTextInto、Focused·Fallback·regex selector 承载步、复杂/罕见 verb)**显式 `EmitError::Unsupported`**,待后续 proposal 真需要时扩。理由:parse_flow_yaml 有共-verb payload 路由 + 三处不可忠实 round-trip 洞(已 read-only 穷尽枚举),不臆造全覆盖。
3. **C1/C2「~20 变体」实为 ~45+ 变体的更正**:探测显示 `Step` enum ~45+ 变体、`parse_flow_yaml` dispatch ~48 verb 键。C3 emitter 穷尽 match 覆盖真实变体数,核心集 refuse 边界据实收敛(非按「~20」臆估)。
4. C3 验收通过 + 用户/上层明确「开始 C4」→ 调 sub-agent 热化 C4(proposal 有效性 e2e:fail flow → propose → apply → 重跑 amended → fail→pass,真 sim/emulator),见 CLAUDE.md §6。发布顺延待授权,不自作主张 publish。

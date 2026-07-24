# plan-hot — v2.11 到 C4：proposal 有效性 e2e（真 claude + 真 emulator）

## 目标 checkpoint

C4：**在真设备上闭合「一条真的失败 flow → 真 `--debug-output` bundle → 真本机 `claude` propose →
apply → emit amended flow → 重跑 → fail→pass」的完整回路。** 这是 v2.11 首个同时碰
**真 `claude`**（§9#2 本机 CLI，非 stub）与 **真 emulator**（§9#1：模拟器，非物理机）的 checkpoint。
通过后世界变成：

1. **回路胶合成一条 device-free 可测的原语** `propose_and_amend(flow, bundle, cfg) -> Result<String, AmendError>`
   （落 `smix-authoring-propose`）：`parse_flow_yaml(flow)` → 取 `steps`+`app_id` → `propose_from_bundle`
   （C2，真/stub claude）→ `apply`（C3）→ `emit_flow_yaml`（C3）→ 返回 amended flow yaml。**example** 薄壳
   `examples/propose_amend.rs` 用 `AiTierConfig::default()`（真 `claude`）驱动它，脚本调它。
2. **回路机制的 device-free 硬证**（claude-stub + fixture bundle，零设备零真 claude）：喂一条**带 typo selector 的
   fixture flow** + fixture bundle（`failure.json` 内含 `suggestions`）+ stub `claude`（吐一个把 typo swap 回去的
   canned `Proposal`）→ `propose_and_amend` → amended yaml → `parse_flow_yaml` 返 `Ok(Flow)` **且** swap 生效。
   证**机制**（propose→apply→emit→parse 链路完整),把 C4 唯一剩的两个真变量（真 claude 质量 + 真设备）隔离出去。
3. **有效性 on-device e2e 脚本** `scripts/dev/v2.11-c4-android-propose-e2e.sh`（Android emulator-5554 +
   系统 Settings，确定性够强的可修复 fail + 有限次 retry）：baseline flow 真跑 PASS → 同一 selector 打一字 typo →
   真跑 FAIL（exit 3）→ 装配真 bundle → 断言 bundle 的 `suggestions` 确含正确 selector（**确定性前置,机器断言,不满足即诚实早失败**）→
   真 `claude` propose+amend（≤3 retry）→ amended flow 真跑 → exit 0 → 末行 `C4-E2E-PASS`。

**边界（诚实划界,不硬塞）**：
- **有效性 gate 的本质是 claude-in-loop,不可比特级复现**（每次 proposal 措辞不同）。C4 用「确定性够强的 fail
  （selector 打一字,正确值经 driver `build_suggestions` 落进 `suggestions`）+ 机器断言该 suggestion 在位 +
  有限次 retry」把 **PASS/FAIL 结论**做稳,但不假装每次 proposal 字节相同。**effectiveness ≠ well-formed-on-device**:
  脚本内分阶段 marker 把二者分开（见 §脚本分阶段判定）——`C4-E2E-PASS`=有效（amended 重跑翻绿）;
  `C4-WELLFORMED-ONLY`=proposal 良构且 amended flow 在设备上跑到 verdict,但 retry 内没翻绿（诚实的部分闭合,非零退出）。
- **`smix authoring propose` CLI 挂载不在 C4 —— 归 C5**。C4 经 crate 的 `examples/propose_amend.rs`（dev-only 薄壳）
  驱动真 claude,不经 CLI 子命令（`smix authoring` 现无 `propose`,本机探测确认）。
- **bundle 现场装配（`failure.json` = `--format json` stdout 重定向）在 C4 脚本里做,不进 CLI**。这是真设备提供的组装
  （shell 重定向,非新 route,非虚构 wire）;把它 bake 进 `smix authoring propose` 归 C5。
- **iOS sim 覆盖不在 C4**（见 §设备选择:选 Android 的依据 + iOS 为何顺延）。回路本身平台无关（adapter/driver/Step 全共享),
  Android 证的是同一条回路。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— C2/C3 产物在(C4 的回路零件)——
grep -q 'pub async fn propose_from_bundle' crates/smix-authoring-propose/src/lib.rs   # C2 生成核心
grep -q 'pub fn apply' crates/smix-authoring-propose/src/lib.rs                       # C3 apply
grep -q 'pub fn emit_flow_yaml' crates/smix-adapter-maestro/src/emitter.rs            # C3 emitter
grep -q 'pub fn parse_flow_yaml' crates/smix-adapter-maestro/src/parser.rs            # 回路里 parse flow 取 steps/app_id
grep -q 'pub app_id: String' crates/smix-adapter-maestro/src/lib.rs                   # emit 要的 app_id 从 Flow 取
# —— 回路原语净新确认:propose_and_amend / example 尚不存在 ——
! grep -q 'pub async fn propose_and_amend' crates/smix-authoring-propose/src/lib.rs
! test -f crates/smix-authoring-propose/examples/propose_amend.rs
# —— 真 claude 在(§9#2 本机 CLI 唯一路径)——
which claude
# —— 真 emulator 在 + 物理机在场(必须钉 serial,禁碰物理机)——
adb devices | grep -q 'emulator-5554'
# —— smix release 二进制在(脚本用它,不重 build)——
test -x target/release/smix
# —— 无 batch 占有者(runner 操作让位 batch-owner;有则拒开,见 §6)——
pgrep -f 'runner.ts|smix run|supervise' >/dev/null && echo "BATCH-OWNER-ACTIVE" || echo "batch-free"
```

前 8 条 exit 0 + 末条打印 `batch-free` = 可开工。任一失败 / 打印 `BATCH-OWNER-ACTIVE` → 按 §6「何时该拒绝热化」回报,
不硬开（尤其:emulator-5554 不在、或有 batch 占有者时,让位不抢）。

## 已经查清、不必重查的事实（C1/C2/C3 + 本机探测,C4 直接引用）

- **`--debug-output` bundle 现状 = MVP,只写 `run-summary.json`（本机探测,`main.rs:约616-634`）**:注释明写
  「Per-step files + on-fail screenshots are deferred to a future increment」。故 `*.fail.tree.json` / PNG /
  `failure.json` **当前 CLI 都不产**。`run-summary.json` 失败时 = `{flow, runOutcome:"failure", error:<字符串>, steps:[...]}`,
  **过薄**(只有 error 字符串,`steps` 是 partial trace,无结构化 selector/suggestions)。
- **修复信号只在 `--format json` stdout,不在磁盘 bundle（`main.rs:约669-689`）**:失败时 `emit_json_failure` 打印顶层 JSON
  `{failure:{code, message, selector:<Debug串>, suggestions:[...], visibleCount:N}}`。**`suggestions` 是 claude 修 typo 的
  唯一可靠信号**(注意:`visibleElements` 全列表**不**在 stdout,只有 `visibleCount` 计数;`selector` 是 Debug 串非结构化)。
  故 C4 脚本必须把 stdout 重定向进 `<bundle>/failure.json`,claude 才读得到 suggestions。这是真设备提供的组装,非新代码。
- **`suggestions` = driver 层「Did you mean」,平台无关(本机探测)**:`smix_error::build_suggestions(target, visible)`
  （`smix-error/src/lib.rs:264`,阈值 0.5、top-3、Levenshtein）在 **`smix-driver/src/lib.rs:404/558/885/1097`** 被调 ——
  driver 是 iOS/Android 共享层,resolve 命中零元素时据可见元素产 `Did you mean "<correct>"? (similarity X, field name/text)`。
  故 **一字 typo → similarity≈0.9 → suggestions 必含正确值**,对两平台都成立。**C4 确定性的根基**。
- **`propose_from_bundle` 真实签名（C2,不动）**:`async fn propose_from_bundle(flow_path:&Path, bundle_dir:&Path,
  cfg:&AiTierConfig) -> Result<Proposal, ExpectationFailure>`。prompt 令 claude `--tools Read` 读 `run-summary.json`/
  `failure.json`/`*.fail.tree.json`/PNG + 原 flow,吐一个 `{edits:[...]}`。**prompt 未指明 `step_index` 的 0/1-based 基**
  且引用了当前 bundle 不产的 `*.fail.tree.json`/PNG —— C4 的 prompt-hardening（S1）显式补 0-based + 指向 `failure.json`。
- **`apply` / `emit_flow_yaml` 真实签名（C3,不动）**:`apply(&Proposal, &[Step]) -> Result<Vec<Step>, ApplyError>`;
  `emit_flow_yaml(&[Step], app_id:&str) -> Result<String, EmitError>`。`apply` 的 `step_index` 是**0-based** 索进 `Vec<Step>`
  （`out[*step_index]`,`lib.rs:250/271`)—— claude 若吐 1-based（照 `run-summary.steps[].n`)会偏一位,故 prompt 必须钉 0-based。
- **`Flow` 取 `app_id`（本机探测,`lib.rs:1106-1111`)**:`pub app_id: String`（`appId:` header,Android 用包名字面）。
  回路 emit 时用它作 `appId:` header。flow 用 `appId: com.android.settings` 字面 → **无需 `smix-apps.yaml`**（apps-config 只为 `app:` 逻辑键）。
- **`smix run` 退出码语义（本机探测,`main.rs:约691-697`)**:`RunError::Sdk`（含 ElementNotFound / NotVisible / AssertionFailed）→ **3**;
  Parse → 2;UnknownKey/Direction → 4;RunFlowCycle/Io → 5;runner 不可达 → 6;成功 → 0。**exit 3 = 我们要的 typo 失败;exit 0 = 修复后 PASS**。机器可判。
- **Android 设备回路命令形（v2.10-c4 已证,`scripts/dev/v2.10-c4-android-record-e2e.sh`)**:`smix runner up "$SERIAL" --platform android`
  （Android runner **无需 `--bundle`**,per-request 取 target)+ `smix run --device "$SERIAL" --platform android <flow>`。
  收尾 `smix runner down --platform android --device "$SERIAL"`。物理机护栏 `case "$SERIAL" in emulator-*)` 已在该脚本落地,照抄。
  **对比 iOS**:`smix capsule up` 需 `--bundle`(本机探测,`capsule up --help`),启动即绑定一个固定 bundle,launch 系统 Settings 复杂化 —— 故 C4 选 Android。
- **`example` 驱动 fenced crate 的先例（本机探测)**:`smix-error`/`smix-sdk`/`smix-selector` 等已有 `examples/`。`smix-ai-tier` 全部测试走
  `stub_cli`（`tests/verdict.rs:38`,写 `#!/bin/sh` 可执行 stub + `claude_bin` 指它),**无 `#[ignore]` 真 claude 测试** —— 印证:
  真 claude+设备的验证是**脚本 opt-in**（如 v2.10-c4）,不是 CI 里的 cargo test。C4 照此:S2 device-free 用 stub cargo test,S3 用脚本。

## 本段预先定死的口径（防 scope 漂移与自欺）

### 设备选择:Android emulator-5554 + 系统 Settings（依据）

**选 Android,不选 iOS sim** —— 理由（哪个能造**确定性够强的可修复 fail** 更可靠为唯一判据,§13 质量优先）:

1. **可修复 fail 的确定性最高**:`search_action_bar` 是 Settings home 上 **v2.10-c4 已证可见**的 selector（该脚本 tap 它成功）。
   baseline `assertVisible {id: search_action_bar}` 必过 → 打一字成 `search_action_barX` 必 ElementNotFound → driver
   `build_suggestions` 必产 `Did you mean "search_action_bar"?` → claude 照抄 swap 回去。**每一环都有在库内证据**。iOS Settings
   我没有已证可见的稳定 selector,现造需 live 探 + 依赖 iOS 版本文案,确定性更低。
2. **设备 harness 已证**:v2.10-c4 的 `runner up --platform android` + Settings + `am start .Settings` 定位是**同仓近期跑绿**的路径。
   C4 唯一的**新变量**是 propose→amend→重跑回路,而非设备 harness —— 按方法论隔离单一新变量。
3. **runner up 无需固定 bundle**:Android per-request 取 target,`smix run --platform android` 按 flow 的 `appId` 逐 flow launch;
   iOS `capsule up` 强制 `--bundle`,把系统 app 生命周期绑死,起 Settings 麻烦。
4. **物理机护栏成熟**:emulator-5554 与物理机 R5CT52DF07D 共存,`case emulator-*` + `export ANDROID_SERIAL=emulator-5554` 双护栏
   （memory `android_gradle_installs_to_all_devices` 教训:Android 侧无 iOS 那样的显式-UDID 护栏,必须钉 serial）。

**iOS 顺延,非放弃**:回路（bundle→claude→propose→apply→emit→重跑)平台无关（adapter-maestro `Step` / smix-driver / smix-error
全共享),Android 证的是同一条回路。iOS 侧同型 e2e（dev sim iPhone 17 Pro `47ACEAE5-36BA-4C62-811B-F09B397910D7` + `capsule up --bundle`)
可作 v2.11 后续或 C6 增量,不在 C4。写入决策日志。

### 确定性前置是**机器断言**,不是假设（诚实的核心）

C4 有效性的确定性依赖「`suggestions` 含正确 selector」。Android `ElementSummary.name` 是否落 resource-id 我**未在设备上验**过 ——
故脚本**不假设**,而是在 corrupt 真跑后 **grep `failure.json` 断言 suggestion 确含 `search_action_bar` 子串**,不含即**诚实早失败**
（`C4-DETERMINISM-UNMET` + 非零退出 + 清晰诊断:「可修复 fail 的 bundle 未携带正确 selector 的 suggestion,确定性前置不成立」)。
这不是分叉,是线性断言（同 baseline-必过 断言)。具体稳定 selector 由**热化实现期 live 探**（`smix capsule tree` / `smix authoring suggest`
对活 runner)最终确认,`search_action_bar` 为起点候选。

### 脚本分阶段判定（effectiveness vs well-formed-on-device 的诚实区分)

脚本内按阶段各自断言,任一硬失败即非零退出 + 专属 message:
- (a) baseline flow 真跑 → **exit 0**（否则 `C4-BASELINE-FAIL`:fixture selector 陈旧,不是回路问题)。
- (b) corrupt flow 真跑 → **exit 3**（否则 `C4-CORRUPT-DID-NOT-FAIL`)。
- (c) 装配 bundle,grep `failure.json` → **含正确 selector 的 suggestion**（否则 `C4-DETERMINISM-UNMET`)。
- (d) 真 claude `propose_and_amend`（≤3 retry)→ **返回 amended yaml 且 `parse_flow_yaml` 接受**（否则 `C4-PROPOSE-MALFORMED`:
  claude 没吐良构 proposal / apply / emit / parse 断链)。**(d) 过 = on-device well-formed**。
- (e) amended flow 真跑 → **exit 0**（翻绿)。**(e) 过 = effective → `C4-E2E-PASS`（exit 0)**。
  (d) 过但 (e) 在 retry 内没翻绿 → `C4-WELLFORMED-ONLY`（非零退出,诚实的部分闭合)。

Checkpoint 验收要的是 `C4-E2E-PASS`。`C4-WELLFORMED-ONLY` 是诚实的**未达**,不是伪绿。

### 其它口径

- **别造虚构 wire（v2.9-C5 教训)**:回路只消费真 bundle 文件（脚本从真设备跑产)+ 真 claude CLI;S2 device-free 走 stub 二进制 + fixture bundle。不新造 route。
- **不动 C2/C3 已绿代码的行为**:S1 只新增 `propose_and_amend` + `AmendError` + **prompt 文本 hardening**（0-based + 指 `failure.json`,
  不改 `propose_from_bundle` 签名/解析,C2 generate 测试用 canned 回复不碰 prompt 文本,不受影响)。`apply`/`emit_flow_yaml`/`validate` 一字不改。
- **§9#2 / §9#8**:全程本机 `claude`,网络 API 不碰;propose 回路是 authoring aid,fenced（deletable / opt-in / non-deterministic),不进 sense/act core。
- **memory 设备纪律**:脚本钉 `ANDROID_SERIAL=emulator-5554` + 拒非 emulator serial;跑前 pgrep 让位 batch-owner;`trap` 收尾 runner down（Android 无 simx-sweep,down 即够);不碰物理机。

## 步骤（线性,2 个）

### S1. 回路原语 `propose_and_amend` + prompt hardening + device-free 机制硬证（claude-stub,零设备)

**红（写测试）**
- 文件:`crates/smix-authoring-propose/tests/amend_loop.rs`
- 断言（2 个 test,咬回路机制,全 device-free + 全 claude-stub):
  1. `stub_loop_closes_and_swaps` — 本地 `stub_cli`（照抄 ai-tier `tests/verdict.rs:38` 4 行范式,写 `#!/bin/sh` echo 一个 canned
     `{"edits":[{"op":"replaceSelector","step_index":1,"new_selector":{"id":"search_action_bar"}}]}` 的可执行 stub,`AiTierConfig.claude_bin` 指它)
     + fixture flow 文件（`appId: com.x` + `launchApp` + `assertVisible {id: search_action_barX}`,即 typo)+ fixture bundle 目录
     （写 `run-summary.json` + `failure.json`,后者含 `suggestions:["Did you mean \"search_action_bar\"? ..."]`)→
     `propose_and_amend(flow, bundle, &cfg).await` 返 `Ok(yaml)` → `parse_flow_yaml(&yaml)` 返 `Ok(flow2)` **且** `flow2.steps[1]`
     的 selector == `Selector::Id{ id:"search_action_bar", .. }`（typo 被 swap 回,机制闭合)。
  2. `stub_cli_failure_surfaces_not_silent` — stub `exit 1` → `propose_and_amend` 返 `Err(AmendError::…)`（driver 错经 `propose_from_bundle`
     传播,不塌成空 yaml,同 ai-tier/`propose_from_bundle` 范式)。
- 跑红（须先失败:`propose_and_amend`/`AmendError` 未建 → 编译失败）:
  ```bash
  cargo test -p smix-authoring-propose --test amend_loop
  ```
  期望:红（`cannot find function propose_and_amend` / `AmendError` 未定义)。

**绿（实现）**
- 文件:`crates/smix-authoring-propose/src/lib.rs`
- API:
  ```rust
  pub async fn propose_and_amend(
      flow_path: &std::path::Path,
      bundle_dir: &std::path::Path,
      cfg: &smix_ai_tier::AiTierConfig,
  ) -> Result<String, AmendError>;   // 返回 amended flow 的 maestro yaml

  #[derive(Debug)]
  pub enum AmendError {
      ReadFlow(std::io::Error),
      ParseFlow(String),                 // parse_flow_yaml 的 ParseError 转字符串
      Propose(smix_error::ExpectationFailure),
      Apply(ApplyError),
      Emit(smix_adapter_maestro::EmitError),
  }
  ```
- 关键点:①`propose_and_amend` = 读 `flow_path` → `parse_flow_yaml` 得 `Flow`(取 `flow.steps` + `flow.app_id`)→
  `propose_from_bundle(flow_path, bundle_dir, cfg).await?`（真/stub claude)→ `apply(&proposal, &flow.steps)?` →
  `emit_flow_yaml(&amended, &flow.app_id)?` → `Ok(yaml)`;各错 map 进 `AmendError` 对应臂（不吞,不 fallback)。
  ②**prompt hardening**（改 `propose_from_bundle` 的 prompt 字符串,签名/解析不动):明确「`step_index` / `before_index` /
  `from_index` / `to_index` 是 flow step 列表里的 **0-based** 位置」+「修复信号看 `failure.json` 的 `suggestions`（`*.fail.tree.json`
  与 PNG 可能不存在,忽略缺失)」。③`propose_and_amend` **不额外 validate**（`apply` 内已先 `validate`,C3)。④device-free:除 claude
  外零外部调用;本 step 的两测均走 stub,不碰真 claude、不碰设备。
- 文件:`crates/smix-authoring-propose/examples/propose_amend.rs` — 薄壳 `main`:读 argv `<flow> <bundle> <out>`,
  `tokio` runtime `block_on(propose_and_amend(flow, bundle, &AiTierConfig::default()))`（default `claude_bin="claude"` = 真本机 CLI)→
  成功写 yaml 到 `<out>` + exit 0;`Err(e)` → `eprintln!("{e:?}")` + exit 1。(example 无独立测试,其逻辑 = `propose_and_amend`,已被 S1 两测覆盖。)
- 文件:`crates/smix-authoring-propose/Cargo.toml` — example 需 `tokio`（rt + macros,若尚未在 deps 则加)。
- 跑绿:上红命令转绿,`2 passed`;`cargo build -p smix-authoring-propose --examples` 干净编译。

**重构（可选)**
- 无。

### S2. 有效性 on-device e2e 脚本（Android emulator-5554 + Settings,真 claude,分阶段断言,marker)

**红（写测试）**
- 本 step 的「测试」是 e2e 脚本自身:先写 baseline / corrupt 两条 flow + 脚本,首次跑**期望在某一阶段失败**（脚本或回路未就绪时非零退出),
  证脚本真在验回路而非空过。
- 文件:`scripts/dev/v2.11-c4-android-propose-e2e.sh`（`chmod +x`;结构照 `scripts/dev/v2.10-c4-android-record-e2e.sh`:
  `set -euo pipefail` / `SERIAL="${1:-emulator-5554}"` / `case emulator-*` 物理机护栏 / `export ANDROID_SERIAL=$SERIAL` /
  `mktemp -d` WORK / `trap cleanup EXIT`（runner down + rm WORK）/ health poll / `log`+`fail` helper)。
- 跑红（脚本存在但回路未接 / selector 未验 → 某阶段非零)：
  ```bash
  scripts/dev/v2.11-c4-android-propose-e2e.sh
  ```
  期望:红（非零退出,末行**不是** `C4-E2E-PASS`;打印命中的阶段 marker 如 `C4-BASELINE-FAIL`/`C4-DETERMINISM-UNMET`)。

**绿（实现）**
- 脚本主体（线性,分阶段各自断言,任一硬失败 `fail` 即非零 + 专属 message):
  0. **护栏 + 让位**:`case "$SERIAL" in emulator-*) ;; *) fail "serial must be emulator; never a physical phone";; esac`;
     `pgrep -f 'runner.ts|smix run|supervise'` 非空 → `fail "batch owner active — yielding"`;`which claude` / `test -x $SMIX`。
  1. **runner up**:`"$SMIX" runner up "$SERIAL" --platform android >up.log 2>&1 &` + `curl $R/health` poll（照 v2.10-c4)。
  2. **定位 Settings home**:`adb -s "$SERIAL" shell am force-stop com.android.settings`;`am start -n com.android.settings/.Settings`;`sleep 2`。
     （热化实现期 live 探确认 `search_action_bar` 可见:`"$SMIX" capsule tree` / `"$SMIX" authoring suggest 'id: search_action_bar' --port 28080`,
     不可见则换一个 live 探到的稳定 selector,并同步改下面两条 flow。)
  3. **baseline flow**（`$WORK/baseline.yaml` = `appId: com.android.settings\n---\n- assertVisible: { id: search_action_bar }`）真跑:
     `"$SMIX" run --device "$SERIAL" --platform android --no-launch "$WORK/baseline.yaml"` → **exit 0**,否则 `fail "C4-BASELINE-FAIL: fixture selector stale"`。
     （`--no-launch`:Settings 已由 adb 定位,不重 launch;避免 launch 抖动。)
  4. **corrupt flow**（`$WORK/corrupt.yaml` = 同 baseline 但 `search_action_barX`）真跑 + 装配 bundle:
     ```bash
     mkdir -p "$WORK/bundle"
     set +e
     "$SMIX" run --device "$SERIAL" --platform android --no-launch \
       --debug-output "$WORK/bundle" --format json "$WORK/corrupt.yaml" > "$WORK/bundle/failure.json" 2>"$WORK/corrupt.err"
     code=$?
     set -e
     [ "$code" = 3 ] || fail "C4-CORRUPT-DID-NOT-FAIL: expected exit 3 (Sdk), got $code"
     ```
     （`--debug-output` 写 `run-summary.json`;单 flow 是否嵌 `<flow-basename>/` 子目录由实现期确认——若嵌,`bundle_dir` 传含 `run-summary.json` 的实际目录。
     `failure.json` = stdout 重定向,含 `suggestions`。)
  5. **确定性前置断言（机器断言）**:`grep -q 'search_action_bar' "$WORK/bundle/failure.json"` → 否则
     `fail "C4-DETERMINISM-UNMET: bundle carries no suggestion naming the correct selector"`。
  6. **真 claude propose+amend（≤3 retry）**:
     ```bash
     ok=0
     for attempt in 1 2 3; do
       if cargo run -q -p smix-authoring-propose --example propose_amend -- \
            "$WORK/corrupt.yaml" "$WORK/bundle" "$WORK/amended.yaml" 2>"$WORK/propose.$attempt.err"; then
         "$SMIX" run --check "$WORK/amended.yaml" \
           && { ok=1; break; }   # --check:device-free parse-only 良构门(hello.yaml 注释证其形),不带 device flag;过 = amended 合法可上设备
       fi
       log "attempt $attempt: no passing amend yet"
     done
     [ "$ok" = 1 ] || fail "C4-PROPOSE-MALFORMED: no well-formed amended flow in 3 attempts"
     ```
     （example 用 `AiTierConfig::default()` = 真 `claude`。`--check` 是 device-free parse gate（`examples/hello.yaml` 注释证其存在),
     amended 良构 = well-formed-on-device 的前半。)
  7. **amended flow 真跑（effectiveness)**:
     ```bash
     if "$SMIX" run --device "$SERIAL" --platform android --no-launch "$WORK/amended.yaml"; then
       log "C4-E2E-PASS"
     else
       fail "C4-WELLFORMED-ONLY: amended flow well-formed + ran on device but did not flip fail->pass in 3 attempts"
     fi
     ```
- 跑绿:
  ```bash
  scripts/dev/v2.11-c4-android-propose-e2e.sh
  ```
  期望:exit 0 + 末行 `C4-E2E-PASS`。

**重构（可选)**
- 若阶段 marker + `fail` 样板与 v2.10-c4 重复,不跨脚本抽公共 lib（两脚本独立、体量小);就地清晰即可。

## Checkpoint C4 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

# (1) 回路机制:device-free + claude-stub 硬证(可复现,零设备零真 claude)
cargo test -p smix-authoring-propose --test amend_loop \
  && cargo build -p smix-authoring-propose --examples \
  && echo GATE-C4-MECHANISM-PASS

# (2) 有效性:真 emulator + 真 claude e2e(opt-in,claude-in-loop)
scripts/dev/v2.11-c4-android-propose-e2e.sh
```

期望:
- (1) 各命令 exit 0 且打印 `GATE-C4-MECHANISM-PASS`。含义(机器可判,零设备、零真 claude、可比特级复现):
  `--test amend_loop` `2 passed` = 回路 `propose_and_amend`(parse→propose[stub]→apply→emit→parse)闭合 + typo 被 swap 回 + driver 错不静默;
  `--examples` 编译干净 = `propose_amend` 薄壳可 build。
- (2) 脚本 exit 0 且末行 `C4-E2E-PASS`。含义(真 emulator + 真本机 claude,claude-in-loop **非比特级复现**,但 PASS/FAIL 结论由「确定性够强的
  fail + 机器断言 suggestion 在位 + ≤3 retry」做稳):baseline 真过 → typo 真失败(exit 3) → bundle 携正确 suggestion → 真 claude 产良构 proposal →
  apply/emit/`--check` 良构 → amended 真跑翻绿(exit 0)。

**诚实划界(哪些 device-free 可预验、哪些必须上设备+真 claude)**:
- **device-free 可预验(验收 (1),CI 可跑)**:回路**机制**——propose(stub)→apply→emit→parse 链路完整 + swap 生效 + 错误不静默。
- **必须真 emulator + 真 claude(验收 (2),opt-in 脚本,不进 CI cargo test)**:**有效性**——真 claude 从真 bundle 的 suggestion 推出 swap、
  amended flow 在真 Android 上从 fail→pass。这层**不可比特级复现**(每次 proposal 措辞不同),结论稳定性靠确定性前置 + retry,不靠"每次字节相同"。
- **不在 C4 验收内**:iOS sim 同型 e2e(平台无关回路,顺延);`smix authoring propose` CLI 挂载 + bundle 现场装配 bake 进 CLI(→ C5);
  `--debug-output` 写全 bundle(per-step tree/PNG,当前 MVP deferred,C4 用 `--format json` stdout 组装 `failure.json` 兜)。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.11-c4-hot.md`。
2. **决策 + 发现写入 `docs/v2.md` 决策日志(§10)**:
   - `{date}` C4 有效性 e2e **选 Android emulator-5554 + 系统 Settings**,iOS 顺延。理由:`search_action_bar` 是 v2.10-c4 已证可见的稳定 selector →
     确定性够强的可修复 fail;Android `runner up` 无需固定 `--bundle`(iOS `capsule up` 需);回路平台无关,Android 证同一条回路。
   - `{date}` C4 回路原语 `propose_and_amend` 落 `smix-authoring-propose`,经 `examples/propose_amend.rs` 薄壳驱动真 claude(`smix authoring propose` CLI 仍归 C5)。
   - `{date}` **有效性 gate 是 claude-in-loop、不可比特级复现**;结论稳定性靠「确定性够强的 fail + 机器断言 suggestion 在位 + ≤3 retry」。诚实分层:
     机制(device-free+stub,可复现,验收(1))与有效性(真设备+真 claude,验收(2))分开;`C4-WELLFORMED-ONLY` 是诚实的部分闭合非伪绿。
   - `{date}` **发现:`--debug-output` bundle MVP 过薄** —— 只写 `run-summary.json`(error 字符串),`failure.json`/`*.fail.tree.json`/PNG 未产;
     修复信号(`suggestions`)只在 `--format json` stdout。C4 脚本用 stdout 重定向组装 `failure.json` 兜底;把它 bake 进 `smix authoring propose` +
     `--debug-output` 补写 tree/PNG 归后续(C5 / 独立增量)。
   - `{date}` **发现:`propose_from_bundle` prompt 原未钉 `step_index` 0/1-based 且引用当前不产的 `*.fail.tree.json`/PNG** —— C4 S1 已 harden
     (钉 0-based + 指 `failure.json`,忽略缺失文件),签名/解析未动。
3. **§9#2 网络路径不变量**:C4 全程本机 `claude`;网络 Claude API 路径未碰。
4. C4 验收((1)+(2)均绿) + 用户/上层明确「开始 C5」→ 调 sub-agent 热化 C5(`smix authoring propose` CLI 挂载 + bundle 现场装配 bake +
   ai-tier 同源 deletability/fence 出口),见 CLAUDE.md §6。发布顺延待授权,不自作主张 publish。

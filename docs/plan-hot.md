# plan-hot — v2.11 到 C5:`smix authoring propose` CLI 挂载 + 同源 fence 出口(v2.11 收官)

## 目标 checkpoint

C5:**把 C4 已在设备上闭合的 `propose_and_amend` 回路挂进 `smix authoring propose` CLI 子命令,并给
`smix-authoring-propose` 立**项目首个机器可判的 deletability/fence 测试**——证 sense/act 路径无一依赖
authoring-propose(删掉它只影响 CLI 那一层薄 wire)。通过后世界变成:

1. **`smix authoring propose <flow> --bundle <dir> -o <out>`** 存在:读**已在盘**的失败 bundle →
   本机 `claude` propose → apply → emit amended flow yaml → 写 `<out>`。device-free 的「消费 bundle →
   propose → amend → 写」部分**全部 bake 进 CLI**;device-bound 的「跑 flow 产 bundle」**不 bake**(见口径)。
   薄 wire 只调 `smix_authoring_propose::propose_and_amend`,不重造回路。
2. **fence 是编译期 + 测试期双重事实**:`crates/smix-authoring-propose/tests/fence.rs` 用 `cargo metadata`
   反查依赖图,断言「直接依赖 `smix-authoring-propose` 的 workspace crate ⊆ {`smix-cli`}」。删掉此 crate 只断
   `smix-cli` 一处薄 wire,sense/act 全路径照常编译——镜像 `smix-ai-tier` README 承诺的 deletability,
   但**这次由真测试守,不是仅靠 prose**(见「已查清事实」§ai-tier fence 现状)。
3. **v2.11 LLM-in-loop authoring 阶段闭合**:五 C 全绿——OBTAINABLE(C1)/ schema+生成核心(C2)/ 良构 gate(C3)/
   有效性 device e2e(C4)/ CLI 挂载+fence 出口+文档(C5)。§9#2 全程本机 `claude`,§9#8 三层不破。零 publish(顺延待授权)。

**边界(诚实划界,不硬塞)**:
- **bundle 现场装配只 bake device-free 那半。** C4 脚本里「`smix run --debug-output <dir> --format json > <dir>/failure.json`」
  这一步**跑真 flow 产 bundle,是 device-bound 的**。把它 bake 进 `smix authoring propose` 会:(a) 把设备依赖拖进本该
  device-free 的 authoring 子命令;(b) 在子命令内重造一遍 `smix run`(§v2.9-C5 虚构 wire 教训:authoring 不该私接
  一条平行 run 路径)。故 C5 只 bake「消费**已在盘** bundle → propose → amend → 写 yaml」——正是 `propose_and_amend`
  的既有边界。产 bundle 那步由调用方/脚本用 `smix run` 做,`smix authoring propose` 消费其产物。两步在 05-cli.md 写清。
- **不铺四生态 SDK。** `propose` 是 dev-time authoring aid、CLI 面,与 `generate` / `tap-record` 同层(录制/生成也只在
  CLI,不进 TS/Swift/Kotlin/Rust 的 driving SDK 面)。runtime SDK 驱动的是**跑** flow,不是**改** flow。v2.11 收官 =
  CLI 子命令 + fence 出口 + 文档,**不**给四 SDK 加 propose parity(硬塞 = 把 authoring aid 错当 runtime 能力)。
- **ai-tier README 的 deletability 承诺当前是 prose,无对应测试(本机探测确认)。** C5 为 authoring-propose 立的 fence test
  是项目**首个**真 dependency-graph fence。给 ai-tier 补同型测试(使其 README 属实)是独立 crate 的顺手修,**不进 C5 步骤**
  (§8.1 不顺便修别的),仅记进决策日志留用户拍板。
- **真 claude 端到端跑不进 cargo test。** `cmd_propose` 是 C4 已在设备上证过的 `propose_and_amend` 的薄壳;其 device-free 机制
  由 C4 的 `tests/amend_loop.rs`(stub claude)守。真 `claude` 经 CLI 跑一遍是 opt-in(可选,见验收末尾),不进机器 checkpoint。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— C4 回路原语 + 薄壳 example 在(C5 挂它)——
grep -q 'pub async fn propose_and_amend' crates/smix-authoring-propose/src/lib.rs
test -f crates/smix-authoring-propose/examples/propose_amend.rs
# —— authoring lane 在(propose 照 Generate/TapRecord 挂)——
grep -q 'enum AuthoringAction' crates/smix-cli/src/main.rs
grep -q 'pub async fn cmd_generate' crates/smix-cli/src/authoring.rs
grep -q '### `smix authoring`' docs/ai-guide/05-cli.md
# —— dep + sub-action 净新确认(C5 加)——
! grep -q 'smix-authoring-propose' crates/smix-cli/Cargo.toml
! grep -q 'AuthoringAction::Propose' crates/smix-cli/src/main.rs
! test -f crates/smix-authoring-propose/tests/fence.rs
# —— 本机 claude 在(§9#2,cmd_propose 用真 CLI)——
which claude
```

前 9 条 exit 0 = 可开工。任一失败 → 按 CLAUDE.md §6「何时该拒绝热化」回报,不硬开。

## 已经查清、不必重查的事实(C4 产物 + 本机探测,C5 直接引用)

- **`propose_and_amend` 真实签名(C4,不动,`smix-authoring-propose/src/lib.rs:340`)**:
  `async fn propose_and_amend(flow_path:&Path, bundle_dir:&Path, cfg:&AiTierConfig) -> Result<String, AmendError>`。
  内部 = 读 flow → `parse_flow_yaml` → `propose_from_bundle`(真/stub claude)→ `apply` → `emit_flow_yaml` → 返 amended yaml。
  各错入 `AmendError` 臂,不吞不 fallback。`cmd_propose` 只调它 + 写文件,**零回路重造**。
- **薄壳 example 形(C4,`examples/propose_amend.rs`)**:argv `<flow> <bundle> <out>`,`AiTierConfig::default()`
  (`claude_bin="claude"` = 真本机 CLI),成功写 yaml + exit 0。**`cmd_propose` = 把这三参数搬成 clap 子命令**,逻辑等价。
- **authoring 挂载真实形(本机探测,`smix-cli/src/main.rs`)**:`Cmd::Authoring { action: AuthoringAction }`(`main.rs:606`);
  `AuthoringAction` 现有 6 变体 `Generate`/`TapRecord`/`Suggest`/`CaptureTree`/`DiffTree`/`Record`(`main.rs:613+`);dispatch
  在 `main.rs:2056` 的 `Cmd::Authoring { action } => match action { … }`,每臂 `return authoring::cmd_xxx(...).await;`。
  各 `cmd_xxx` 是 `pub async fn ... -> Result<ExitCode, CliError>`(`authoring.rs`)。`Generate` 的参数形
  (`input: PathBuf` 位置参 + `#[arg(long, short)] output: PathBuf`)= `Propose` 的模板。
- **smix-cli 当前**不**依赖 smix-authoring-propose / smix-ai-tier(本机探测,`smix-cli/Cargo.toml`)**。C5 加**一个** dep
  `smix-authoring-propose`;为让 CLI 只需这一个 dep(不直接引 ai-tier),authoring-propose **re-export** `AiTierConfig`
  (`pub use smix_ai_tier::AiTierConfig;`)。于是:authoring-propose 的**直接** dependents = {smix-cli};ai-tier 的直接
  dependents = {smix-authoring-propose}(smix-cli 经 re-export 用它,不直接列 dep)——fence allowlist 因此干净。
- **ai-tier fence 现状 = 仅 prose,无测试(本机探测,重要)**:`smix-ai-tier/README.md:21-23` 写「Delete `smix-ai-tier` and
  the sense path still compiles … enforced by a test rather than asserted in a comment」;`Cargo.toml:9` description 亦称
  「nothing that senses may depend on this crate」。但 `crates/smix-ai-tier/tests/` **只有 `verdict.rs`**(全是 verdict 功能测,
  无 dependency-graph fence 测);全 workspace grep `deletab|not depend|sense path|cargo_metadata` **无任何 fence 测试命中**。
  故该承诺当前是 prose,README 说的「enforced by a test」**不属实**(memory `comments_are_claims_code_is_truth`)。C5 立的
  fence test 是项目首个真 fence——顺带证明「照 ai-tier 立一个」时,ai-tier 本身其实没有可照抄的测试。
- **`cargo metadata` 反查可行(本机探测已跑通)**:`cargo metadata --format-version 1 --no-deps` 列 workspace 各 package 及其
  `dependencies[].name`(直接依赖)。当前无任何 package 直接依赖 smix-authoring-propose(反查为空)。**直接边扫描对本 fence 充分**:
  任何 sense/act crate 若(经中间 workspace crate)传递依赖 authoring-propose,那条中间 workspace crate 必在直接边扫描里现身
  (它直接依赖 authoring-propose 且不在 allowlist)→ 被逮。故无需解析完整 resolve 图。测试内用 `std::env::var("CARGO")`
  (cargo 跑测试时注入)定位 cargo,`serde_json`(authoring-propose 已在 `[dependencies]`)解析 stdout。
- **CLI 文档一致性 gate 真实形(本机探测,`smix-cli/tests/documented_flags_exist.rs`)**:三 test —
  ① `every_command_the_guides_print_matches_the_cli_surface`(guide 里印的 `smix … --flag` 的 flag 必须真存在 + positional 数对);
  ② `every_repo_path_the_guides_name_exists`(guide 引的仓内路径必须在);③ `every_command_the_cli_offers_is_in_the_reference`
  (`smix --help` 的**顶层** command 必须在 `05-cli.md`)。**`propose` 是 `authoring` 下的 sub-action,非顶层 command**,故
  ③ 不受影响(`authoring` 顶层早已文档化,`05-cli.md:266`)。C5 若在 guide 里写 `smix authoring propose … --bundle … -o …`,
  ① 要求 `--bundle`/`--output` 真存在(实现后成立)。**任务提到的 `route-conformance` gate 无对应独立测试文件**——CLI 一致性
  gate 实为 `documented_flags_exist.rs` + in-crate `guide_gate.rs`(冷计划/任务假设与实际不符,记决策日志)。
- **`guide_gate.rs`(in-crate corpus gate)只执行 yaml flow 示例到 wire 层,不执行 CLI 调用示例(本机探测,`src/guide_gate.rs` 头注)**。
  故 `smix authoring propose …` 作为**命令行示例**写进文档**不**触发 corpus 执行——不要把它塞进 guide_gate 跑的 flow corpus。
- **main.rs 已有 clap-parse 测试范式(本机探测,`main.rs:3174` `#[cfg(test)] mod tests`)**:`Cli::try_parse_from([...]).unwrap()`
  → `let Cmd::… { … } = cli.cmd else { panic!() }`(见 `exec_parses_hyphen_args_verbatim` `main.rs:3250`)。S1 红绿测试照此。
- **emit/parse/EmitError 已导出(前置已证)**:`emit_flow_yaml`(`emitter.rs:44`)/ `parse_flow_yaml`(`parser.rs:2942`)/
  `EmitError`(`emitter.rs:22`)/ `Flow.app_id`(`lib.rs:1111`)——`propose_and_amend` 内部用,C5 不碰。

## 本段预先定死的口径(防 scope 漂移与自欺)

- **bake 边界 = device-free 半条**:`cmd_propose` 只消费**已在盘** bundle(`propose_and_amend`),不跑 flow 产 bundle。理由见目标§边界。
- **不铺四 SDK**:propose 是 CLI dev-time authoring aid,同 generate/tap-record。理由见目标§边界。
- **fence 只守 authoring-propose**:C5 不给 ai-tier 补 fence(§8.1 不顺便修别的);ai-tier README 不属实的发现记决策日志,留用户拍板。
- **不动 C4 已绿行为**:`propose_and_amend`/`apply`/`emit_flow_yaml`/`propose_from_bundle` 签名与逻辑一字不改。C5 只**新增**:
  authoring-propose 的 `pub use AiTierConfig` re-export + `tests/fence.rs`;smix-cli 的一个 dep + `AuthoringAction::Propose` +
  `cmd_propose` + 一条 dispatch 臂;`05-cli.md` 一段文档。
- **§9#2 / §9#8**:全程本机 `claude`(`AiTierConfig::default()`),网络 Claude API 不碰;propose 回路 fenced(deletable /
  opt-in / non-deterministic),sense/act 无一依赖它(fence test 机器证)。
- **§13 质量 >> 成本**:fence 用真 dependency-graph 测试(机器守),不用 prose/注释(prose 恰是 ai-tier 现状的坑)。

## 步骤(线性,2 个)

### S1. `smix authoring propose` 子命令 + `cmd_propose`(device-free wire over `propose_and_amend`)+ dep + 文档

**红(写测试)**
- 文件:`crates/smix-cli/src/main.rs` 的 `#[cfg(test)] mod tests`(`:3174`)——加 1 个 clap-parse test:
  - `authoring_propose_parses_flow_bundle_out`:
    ```rust
    let cli = Cli::try_parse_from([
        "smix", "authoring", "propose", "corrupt.yaml",
        "--bundle", "bundle-dir", "-o", "amended.yaml",
    ]).unwrap();
    let Cmd::Authoring { action: AuthoringAction::Propose { flow, bundle, output } } = cli.cmd
        else { panic!("expected authoring propose") };
    assert_eq!(flow, std::path::PathBuf::from("corrupt.yaml"));
    assert_eq!(bundle, std::path::PathBuf::from("bundle-dir"));
    assert_eq!(output, std::path::PathBuf::from("amended.yaml"));
    ```
- 跑红(须先失败:`AuthoringAction::Propose` 变体不存在 → 编译失败):
  ```bash
  cargo test -p smix-cli authoring_propose_parses_flow_bundle_out
  ```
  期望:红(`no variant named Propose` / `Propose` 未定义编译错)。

**绿(实现)**
- 文件:`crates/smix-authoring-propose/src/lib.rs` — 加 re-export(带 doc,满足 `[lints] workspace` 的 missing_docs):
  ```rust
  /// Re-exported so a CLI wire needs only this crate as a dependency, not
  /// `smix-ai-tier` directly — keeping the authoring-propose fence's allowlist
  /// to a single crate.
  pub use smix_ai_tier::AiTierConfig;
  ```
- 文件:`crates/smix-cli/Cargo.toml` — `[dependencies]` 加
  `smix-authoring-propose = { path = "../smix-authoring-propose", version = "2.0.0" }`(**唯一**新 dep;ai-tier 经 re-export 用)。
- 文件:`crates/smix-cli/src/main.rs` — `AuthoringAction` 加变体(照 `Generate` 参数形):
  ```rust
  /// Read a failed flow's on-disk bundle, ask a local `claude` to propose
  /// edits, and write the amended flow. Device-free: consumes a bundle already
  /// on disk (produce it with `smix run --debug-output <dir> --format json >
  /// <dir>/failure.json`); this subcommand does not run the flow itself.
  Propose {
      /// The failed flow yaml.
      flow: PathBuf,
      /// The on-disk bundle dir (run-summary.json + failure.json + …).
      #[arg(long)]
      bundle: PathBuf,
      /// Output path for the amended flow yaml.
      #[arg(long, short)]
      output: PathBuf,
  },
  ```
  dispatch(`Cmd::Authoring { action }` match 内,`:2056` 那组)加一臂:
  ```rust
  AuthoringAction::Propose { flow, bundle, output } => {
      return authoring::cmd_propose(flow, bundle, output).await;
  }
  ```
- 文件:`crates/smix-cli/src/authoring.rs` — 加:
  ```rust
  use smix_authoring_propose::{AiTierConfig, propose_and_amend};

  /// `smix authoring propose` — device-free wire over `propose_and_amend`:
  /// read a failed flow + its on-disk bundle, ask the local `claude` to propose
  /// edits, write the amended flow yaml. Producing the bundle (running the flow
  /// on a device) is the caller's step, not this one.
  pub async fn cmd_propose(
      flow: PathBuf,
      bundle: PathBuf,
      output: PathBuf,
  ) -> Result<ExitCode, CliError> {
      let cfg = AiTierConfig::default();
      let yaml = propose_and_amend(&flow, &bundle, &cfg)
          .await
          .map_err(|e| CliError::Other(format!("propose: {e:?}")))?;
      std::fs::write(&output, &yaml)
          .map_err(|e| CliError::Other(format!("write {}: {e}", output.display())))?;
      println!("wrote {}", output.display());
      Ok(ExitCode::SUCCESS)
  }
  ```
  关键点:①薄 wire,零回路重造;②`AmendError` 不吞——map 进 `CliError::Other` 明确报(`propose_and_amend` 本身已不 fallback);
  ③device-free(不接受 `--device`/`--port`;不跑 flow)。
- 文件:`docs/ai-guide/05-cli.md` — 在 `### \`smix authoring\``(`:266`)段内加 `propose` 子条,含**两步**可复制命令:
  ```
  # 1) produce the bundle by running the (failing) flow on a device:
  smix run --device <SERIAL> --platform android --debug-output ./bundle --format json corrupt.yaml > ./bundle/failure.json
  # 2) device-free: propose + amend from the on-disk bundle via local claude:
  smix authoring propose corrupt.yaml --bundle ./bundle -o amended.yaml
  ```
  (①用真存在的 flag:`--device`/`--platform`/`--debug-output`/`--format`/`--bundle`/`-o`;②不引任何仓内不存在路径——
  `corrupt.yaml`/`amended.yaml`/`./bundle` 是占位,非仓内 path claim,`documented_flags_exist` 的 path 检查只查带扩展名且以
  `examples/`/`docs/` 等已知 ROOTS 开头的 token,占位不触发。)
- 跑绿:
  ```bash
  cargo test -p smix-cli authoring_propose_parses_flow_bundle_out   # 转绿
  cargo build -p smix-cli                                           # 干净编译
  cargo test -p smix-cli --test documented_flags_exist             # 三 test 仍绿(新 flag 有据、无 bogus path)
  ```

**重构(可选)**
- 无。

### S2. `smix-authoring-propose` deletability/fence 测试(cargo metadata 反查,机器守)

**红(写测试)**
- 文件:`crates/smix-authoring-propose/tests/fence.rs`
- 断言(1 个 test,咬真实依赖图):
  - `nothing_but_the_cli_wire_depends_on_this_crate`:`std::env::var("CARGO").unwrap_or("cargo")` →
    `Command::new(cargo).args(["metadata","--format-version","1","--no-deps"])` → `serde_json::from_slice` →
    遍历 `packages[]`,收集所有「`dependencies[].name == "smix-authoring-propose"`」的 `packages[].name` 为 `dependents` 集 →
    断言 `dependents ⊆ ALLOWED`,不在 allowlist 的以清晰 message 列出(「a crate on the sense/act path took a dependency on
    the fenced authoring-propose tier — the fence is that only the CLI wire may」)。**先写 `const ALLOWED: &[&str] = &[];`**
    (空 allowlist)。
- 跑红(S1 已让 smix-cli 直接依赖本 crate → 空 allowlist 必逮到 smix-cli,证测试**真在读依赖图**而非空过):
  ```bash
  cargo test -p smix-authoring-propose --test fence
  ```
  期望:红(assert 失败,message 含 `smix-cli`)。

**绿(实现)**
- 文件:`crates/smix-authoring-propose/tests/fence.rs` — 把 `ALLOWED` 改为 `&["smix-cli"]`(唯一允许的 dependent = C5 挂的
  CLI 薄 wire)。加一条 sanity 下限:`assert!(!all_workspace_pkgs.is_empty(), …)`(metadata 解析出的 package 非空,防解析静默空过、
  测试因「什么都没读到」而假绿,同 `documented_flags_exist` 的 `checked >= N` 下限精神)。
- 关键点:①**直接边扫描对本 fence 充分**(理由见「已查清事实」§cargo metadata):任何传递依赖必经某条 workspace 直接边现身;
  ②测试内**不**依赖 ai-tier / 任何 sense crate,只读 metadata——它守的是「谁依赖我」,天然不需被守方在场;③含义 = 删掉
  smix-authoring-propose 只断 smix-cli 的 propose 子命令,sense/act 全路径照常编译 → deletability 机器证成立(ai-tier README
  仅以 prose 声称的那件事,这里由测试兑现)。
- 跑绿:
  ```bash
  cargo test -p smix-authoring-propose --test fence
  ```
  期望:绿(`1 passed`,dependents = {smix-cli} ⊆ allowlist)。

**重构(可选)**
- 无。

## Checkpoint C5 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix

cargo test -p smix-cli authoring_propose_parses_flow_bundle_out \
  && cargo test -p smix-authoring-propose --test fence \
  && cargo test -p smix-authoring-propose --test amend_loop \
  && cargo test -p smix-cli --test documented_flags_exist \
  && cargo build --workspace \
  && cargo run -q -p smix-cli -- authoring propose --help 2>&1 | grep -q -- '--bundle' \
  && echo GATE-C5-PASS
```

期望:各命令 exit 0 且末行打印 `GATE-C5-PASS`。分项含义(机器可判,零人工读图、零设备、零真 claude、可复现):
- `authoring_propose_parses_flow_bundle_out` `1 passed` = `smix authoring propose <flow> --bundle <dir> -o <out>` clap 挂载成形。
- `--test fence` `1 passed` = 直接依赖 authoring-propose 的 workspace crate ⊆ {smix-cli};删此 crate 只断 CLI 薄 wire,sense/act 照常编译(deletability 机器证)。
- `--test amend_loop` `2 passed` = C4 回路机制(propose[stub]→apply→emit→parse + swap 生效 + 错不静默)未被 C5 改动破坏(回归)。
- `--test documented_flags_exist` 三 test 绿 = 文档里 `smix authoring propose … --bundle … -o …` 的 flag 真存在、无 bogus path、顶层 command 无遗漏。
- `cargo build --workspace` 干净 = 新 dep + re-export + 子命令全 workspace 编译通过(含 `[lints] workspace` deny)。
- `authoring propose --help` 含 `--bundle` = 子命令真在二进制里、help 面成形(与文档同源)。

**opt-in(不进 checkpoint,需真 claude + 真设备,同 C4 定位)**:`smix authoring propose` 端到端真跑一遍——用 C4 的
`scripts/dev/v2.11-c4-android-propose-e2e.sh` 装配一份真 bundle,把脚本的 propose 步从 `cargo run --example propose_amend`
换成 `smix authoring propose … --bundle … -o …`,断言 amended flow 真跑翻绿。`cmd_propose` 是 C4 已在设备上证过的
`propose_and_amend` 的薄壳,该 opt-in 只做**人手确认**,不进机器 checkpoint(claude-in-loop 非比特级复现)。

**诚实划界(哪些机器可判、哪些必须人手 opt-in)**:
- **机器可判(验收 6 条,CI 可跑,零设备零真 claude)**:CLI 挂载成形 + fence 机器证 + C4 机制回归 + 文档一致 + workspace 编译 + help 面。
- **人手 opt-in(不进 CI cargo test)**:真 claude 经 `smix authoring propose` 从真 bundle 产 amended flow、在真设备翻绿——由 C4 回路 + C5 薄壳的组合正确性推得,可选脚本确认。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.11-c5-hot.md`。
2. **决策 + 发现写入 `docs/v2.md` 决策日志(§10)**:
   - `{date}` C5 `smix authoring propose` CLI 挂载 = device-free 消费**已在盘** bundle → `propose_and_amend` → 写 amended yaml
     (薄 wire,零回路重造)。**flow-run-produces-bundle 不 bake**:device-bound + 会在子命令内重造 `smix run`(v2.9-C5 虚构 wire 教训)。
     两步文档化(先 `smix run --debug-output … --format json > …/failure.json` 产 bundle,再 `smix authoring propose` 消费)。
   - `{date}` C5 fence:新增 `smix-authoring-propose/tests/fence.rs`(cargo metadata 反查,直接 dependents ⊆ {smix-cli}),机器证 deletability。
     smix-cli 只加**一个** dep,authoring-propose `pub use AiTierConfig` re-export 使 CLI 不直接引 ai-tier(allowlist 干净)。
   - `{date}` **发现:ai-tier README/Cargo.toml 声称 deletability「enforced by a test」,实际无该测试(仅 prose,`tests/` 只有 verdict.rs)**。
     C5 为 authoring-propose 立了项目**首个**真 dependency-graph fence;建议 mirror 给 ai-tier 使其 README 属实——**独立 crate,待用户拍板,不顺手改**(§8.1)。
   - `{date}` **发现:任务/冷计划提及的 `route-conformance` gate 无对应独立测试文件**;CLI 文档一致性实际由 `documented_flags_exist.rs`
     (3 test)+ in-crate `guide_gate.rs`(只执行 yaml flow 示例到 wire)守。`propose` 是 sub-action 非顶层 command,不触发
     `every_command_the_cli_offers_is_in_the_reference`。
   - `{date}` **四生态:propose 不铺四 SDK**,是 CLI dev-time authoring aid(同 generate/tap-record);runtime SDK 驱动跑 flow 非改 flow。
3. **§9#2 网络路径不变量**:C5 全程本机 `claude`(`AiTierConfig::default()`);网络 Claude API 路径未碰。**§9#8 三层**:propose fenced,
   sense/act 无一依赖它(fence test 机器守)。
4. **v2.11 LLM-in-loop authoring 阶段闭合声明**:五 C 全绿(OBTAINABLE / schema+生成核心 / 良构 gate / 有效性 device e2e / CLI+fence+文档)。
   是否热化 **v2.12(federation)** 待用户明确授权(见 roadmap);发布顺延待授权,**不自作主张 publish**。见 CLAUDE.md §6。

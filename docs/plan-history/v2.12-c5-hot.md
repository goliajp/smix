# plan-hot — v2.12 到 C5:CLI 收口 + 文档(`smix run --nodes`,v2.12 闭合 = 折入阶段收官)

## 目标 checkpoint

C5:**federation 表面挂进真 CLI,双节点出口 e2e 经 `smix run --nodes` 全链跑通,
v2.12 闭合 = v2.8–v2.12 折入阶段全部完成**。通过后世界变成:`smix run <flows...>
--nodes <nodes.yaml>` 一条命令走完 federation lane —— parse_nodes → 本地 flow
存在性检查 → 逐节点 readiness gate(全过才扇出)→ 按槽 spawn-all-then-join 并发
`run_ssh` 扇出(照 `run_parallel` 形)→ `fold_slot_results` 按节点折叠 →(有
`--debug-output` 时)per-node artifact rsync 回收 → `merge_reports` → merged JSON
单文档打 stdout → `ExitCode::from(aggregateExit)`。`main.rs` 的
`#[cfg(test)] mod federation` 变 `mod federation`(runtime caller 就位,zero-warning
build 即全表面被消费的机器证明)。文档:`05-cli.md` 加 `--nodes` 行 + Distributed
runs 小节(documented_flags_exist 三 gate 绿)。出口 e2e:双节点(studio 经
`ssh localhost` + mini)各自 sim 分片跑,经**真 CLI**(非 ignored 测试),末行
marker `C5-FED-E2E-PASS`。完成后:归档 + 决策日志 + **折入阶段全完成声明,
v2.0.0 ship 决策交还用户,零 publish**。

**C5 拍板一(--nodes CLI 形)**:`--nodes <PATH>` 为 `Option<PathBuf>`,clap
`conflicts_with_all = ["device", "also_device", "parallel"]`(设备定位归 roster,
本机 device 链与 federation lane 互斥;`--parallel` 默认值不触发 conflict ——
clap default 不算 present,显式 `--parallel 1` 触发,正确)。**已查清的 clap 语义
后果**:env 来源的值**会**触发 conflict → 导出 `SMIX_UDID` 时 `--nodes` 报
conflict —— 这是 fail-fast 正确方向(产品自述「ambiguity is a bug, not a
feature」),不特判;integration 测试与 e2e 一律 `env_remove("SMIX_UDID")` /
不导出。federation lane 恒打 merged JSON 单文档到 stdout(叶子行远端本就恒
`--format json`,merged 报告没有 human 形,不看本地 `--format`);本地
`--runner-port` 不进 lane(异构端口单值表达不了)—— per-node 端口走 roster 新
optional 字段 `runnerPort`(serde rename,与 registry 字段同名),扇出时给该节点
passthrough 追加 `--runner-port <p>`。以上三条「不看的 flag」全部写进 05-cli.md,
不静默。

**C5 拍板二(同步/重建不进 CLI,gate 只判不修)**:`readiness_argv` 的 doc 注释
C3 起已钉「repair, i.e. rebuilding, is the sync script's job, never the gate's」。
把 rsync 源同步 + 远端 cargo build 塞进 `smix run --nodes` = 在 run 子命令里重造
provisioning(v2.9-C5「虚构 wire」同型教训)。单路径:CLI 只跑 gate,gate 红即
`FederationRunError::Gate` 快败(报节点名 + stderr);节点准备(同步 / 重建 /
stamp)是 operator/脚本职责,05-cli.md 写明 prep 两步命令。flow 文件契约:路径
repo-relative、须在每个节点 repo 同路径存在(scheduler repo 即权威源);CLI 在
扇出前做本地存在性检查快败(CLI 参数 = 信任边界)。

**C5 拍板三(artifact 回收进 CLI + FED_ARTIFACT_DIR 去 checkpoint 名)**:
`--debug-output <dir>` 在 federation lane 的语义翻译 = 远端 passthrough 带
`--debug-output <FED_ARTIFACT_DIR>`(节点 repo 相对),join 后逐节点
`run_rsync(artifact_pull_argv(...))` 拉回 `<dir>/<node>/`(消费 C4 建好的双腿,
回路一条命令闭合)。pull 失败 = `FederationRunError::ArtifactPull`(不吞),发生
在 merged 打印**之前** → 整体 CliError exit 1,无半成功歧义。const 值
`.smix/fed-c4-artifacts` 改 **`.smix/fed-artifacts`**:它随本段成为 CLI 文档化
机制的永久面,checkpoint 编号不进产品永久面(§13 质量 > 改动成本);跟改 =
pinned 单测 `artifact_pull_argv_pins_the_rsync_command` 期望串 + C4 脚本
line 23 `ARTIFACT_DIR` 变量(该脚本驱动的 ignored 测试用 const,不跟改则其
teardown 清错目录 —— 1 行一致性跟改,非改 C4 验收语义)。远端 staging 目录
不由 CLI 预清(不在产品加破坏性远端操作);同名输出原地覆盖,清理归脚本
teardown,文档写明。

**C5 拍板四(teardown 纪律,C4 事故余波,逐字执行)**:`runner down`(iOS 形)
在 recorded-handle 处理后有**端口无关**兜底 `pkill -INT -f "xcodebuild.*SmixRunner"`
(runner.rs:1053-1061),`SMIX_RUNNER_PORT`/flag 只影响 recorded handle 与 health
检查,兜底不分端口 —— C4 e2e 带着 `SMIX_RUNNER_PORT=22097` 照样杀掉了 insight
的 22087 常驻 runner(v2.md 2026-07-24 事故行)。**修法待用户拍板,C5 绝不顺手改
runner.rs**。因此:
- **studio 侧 teardown 禁用任何形式的 iOS `smix runner down`**(带 env / 带 flag
  都不行)。改为 recorded-handle 精确收:`target/release/smix diagnostic store`
  (stdout 干净 JSON,已实测;kevy AOF 行走 stderr)取 `["one:runner-ios"].pid`
  (smix-store SINGLETON prefix `one:` + key `runner-ios`,RunnerState 含 pid),
  `ps -p $PID -o command=` 验证含 `xcodebuild`(pid-reuse 防护)后 `kill -INT`,
  循环等 ≤30s,残留才 `-9`。stale handle 留在 store 不清 —— 产品自会 drop
  (runner.rs:1045-1047「already gone — dropping stale handle」),脚本不碰 store。
- **mini 侧远端 `smix runner down` 可用**(mini 无他方 runner)。
- **insight 的 sim-insight(FFC57DAE,Booted)与其任何进程绝不碰**;studio guard
  不含裸 `pgrep xcodebuild`;sim 操作全程显式 UDID。
- **22087 此刻实测无人监听**(insight runner 已倒,lsof exit 1),但**不假设可用**
  —— insight 可能随时重启;studio 节点仍走 22097 隔离(lsof guard 先证空闲)。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
grep -q 'pub fn merge_reports' crates/smix-cli/src/federation.rs        # C4 产物在
grep -q 'pub fn artifact_pull_argv' crates/smix-cli/src/federation.rs   # C4 产物在
! grep -q 'pub fn run_federation' crates/smix-cli/src/federation.rs     # C5 编排腿净新
! grep -q 'runnerPort' crates/smix-cli/src/federation.rs                # roster 端口字段净新
grep -B4 '^mod federation;' crates/smix-cli/src/main.rs | grep -q 'cfg(test)'  # 起点仍 test-gated
cargo test -p smix-cli --bin smix federation 2>&1 | grep -q 'ok. 17 passed; 0 failed; 2 ignored'  # C4 基线绿
cargo test -p smix-cli --bin smix 2>&1 | grep -q 'ok. 142 passed; 0 failed; 2 ignored'            # 全量基线绿
cargo test -p smix-cli --test documented_flags_exist 2>&1 | grep -q 'ok. 3 passed; 0 failed'      # 文档 gate 基线绿
ssh -o ConnectTimeout=5 -o BatchMode=yes mini \
  'test -x ~/workspace/goliajp/smix/target/release/smix'                # mini 可达 + 二进制在
ssh -o ConnectTimeout=5 -o BatchMode=yes localhost true                 # localhost 自授权仍在(C4 遗产)
xcrun simctl list devices available | grep -q 'sim-smix-02'             # 本机节点设备在
test -x target/release/smix                                             # 本机二进制在
test -f scripts/release/stress-corpus/launch-and-capture.yaml           # e2e 黄金 flow 在
test -f scripts/release/stress-corpus/screenshot-twice.yaml             # e2e 黄金 flow 在
```

全部 exit 0 = 可开工(2026-07-24 热化期已逐条实跑,全过)。任一失败 → 按 §6 拒绝开工回报。

## 已经查清、不必重查的事实(热化期实测,2026-07-24)

- **基线**:federation 17 passed 2 ignored / bin 全量 142 passed 2 ignored /
  documented_flags_exist 3 passed / parallel_run 3 test 全实跑吻合。
- **Run 命令 clap 现状**(main.rs:394-541 实读):`--device` 带 `env = "SMIX_UDID"`,
  `--parallel` 带 `default_value_t = 1`,`--also-device` 是 `Vec<String>`;全命令
  **零** `conflicts_with` 先例(唯二 grep 命中是无关 prose)。`--runner-port` 带
  `env = "SMIX_RUNNER_PORT"`。
- **parallel lane 形**(main.rs:1785-1867):passthrough 清单 = bundle-id /
  no-launch / animations / activate / verbose / fail-fast / retry(≠1 才)/
  platform(恒)/ apps-config / debug-output / env;`run_parallel`
  (parallel.rs:76-110)= spawn-all-then-join,spawn 失败计 1,`wait().code()`
  clamp 0-255 —— C5 扇出照此形(ssh 是本机进程,`Stdio::piped` stdout +
  inherit stderr,join 用 `wait_with_output`)。
- **CliError 出口**:`main()`(main.rs:1108-1117)对 `Err(e)` 打 `error: {e}` 并
  `ExitCode::from(1)` —— roster 解析错 / gate 红 / pull 失败走这条,exit 1 与
  flow 码空间 {0,2,3,4,5,6} 及 255 哨兵不混。
- **federation.rs C4 后全 pub API**(实读):`NodeSpec{name,host,repo,devices}` /
  `parse_nodes` / `expand_slots` / `SlotAssignment{node,device_ref,flows}` /
  `assign_flows` / `SSH_TRANSPORT_EXIT`=255 / `is_transport_failure` /
  `shell_quote` / `remote_argv`(BatchMode + cd repo + **无条件附 `--format json`**;
  passthrough 逐字进 remote 串不加 quote → lane 侧 passthrough token 需自带引号,
  统一 `shell_quote` 全部 token,恒引号与 `shell_quote` 自家契约同形)/
  `FED_BUILD_STAMP` / `FlowReport{flow,outcome,raw}` / `ReportError` /
  `parse_report_lines` / `readiness_argv`(只判不修)/ `RemoteOutput` / `run_ssh` /
  `FED_ARTIFACT_DIR` / `artifact_pull_argv` / `run_rsync` /
  `NodeResult{name,exit,reports}` / `MergedNode` / `MergedReport{nodes,aggregate_exit}`
  (camelCase serde)/ `merge_reports`(`parallel::aggregate_exit` 逐字复用)。
- **runner handle 形**:`runner up` 写 store singleton `runner-ios`
  (runner_state.rs:33,smix-store key = `one:runner-ios`,SINGLETON prefix
  lib.rs:41),`RunnerState{pid,udid,port,log,bundle,supervisor_pid}`
  (runner.rs:15-27)。`smix diagnostic store` dump 平铺 map(key→value),当前
  本 repo store dump 为 `{}`(无 runner 在录);**kevy AOF replay 行走 stderr,
  stdout 只有 JSON**(实测 `2>/dev/null` 后 stdout = `{}`)。
- **端口现状**:22087 与 22097 均 lsof exit 1(无监听)—— insight runner 已倒;
  按拍板四仍不假设 22087 可用。`sim-smix-02`(5D087114,Shutdown)在;
  `sim-insight`(FFC57DAE,**Booted,不碰**)。
- **mini**:BatchMode 通、二进制在;localhost 自授权仍在(公钥仍在
  authorized_keys,`ssh localhost true` exit 0)—— e2e 脚本自授权段保持幂等形
  即可,本轮预期走「已在则跳过」分支。
- **documented_flags_exist 三 gate 面**(tests/documented_flags_exist.rs 实读):
  ①guide 里每条 `smix …` 命令行的 flag 必须真在 `--help`(corpus 含 05-cli.md);
  ②guide 里带扩展名且以 examples/|flows/|docs/|crates/|scripts/|… 开头的 token
  必须真存在(`.smix/nodes.yaml`、`nodes.yaml` 不在 ROOTS 前缀内,**不会被咬**;
  含 `target/` 段的路径豁免);③顶层 command 必须已文档化(`--nodes` 是 flag
  非 command,不触发)。
- **C4 脚本 artifacts 变量**:`scripts/dev/v2.12-c4-federation-two-node-e2e.sh:23`
  硬编码 `ARTIFACT_DIR=".smix/fed-c4-artifacts"`,用于 line 69/76 teardown 清理
  —— const 改名须跟改此 1 行(拍板三)。
- **05-cli.md run 表现状**:未列 `--parallel`/`--also-device`(前期缺,文档 gate
  方向 docs→CLI 不咬未文档 flag)—— C5 只加 `--nodes` 段,**不顺手补** --parallel
  文档(§8.1),列入「不符处」记录。
- **integration 测试范式**:`tests/parallel_run.rs` 用 `CARGO_BIN_EXE_smix` +
  tempfile(dev-dep 已在)驱动真二进制 —— C5 的 CLI 面测试照此形新开
  `tests/federation_run.rs`。

## 步骤(线性,3 个)

### S1. roster runnerPort + `fold_slot_results`(device-free 纯逻辑)+ artifacts 目录去 c4 名

**红(写测试)**
- 文件:`crates/smix-cli/src/federation.rs`(tests mod 追加 4 个;另改 1 个 pinned 期望)
- 断言:
  - `parses_an_optional_per_node_runner_port`:roster yaml 一节点带
    `runnerPort: 22097`、一节点不带 → `nodes[0].runner_port == Some(22097)`、
    `nodes[1].runner_port == None`(serde rename `runnerPort`,与 registry 字段
    同拼写;`#[serde(default)]` 使旧 roster 不破)
  - `fold_groups_slots_by_node_with_max_exit_and_concatenated_reports`:节点 a
    两槽(exit 0 + exit 3,各 1 行真 shape 叶子)+ 节点 b 一槽(exit 0)→
    `Vec<NodeResult>` 长 2、`[0].exit == 3`(`parallel::aggregate_exit` 语义)、
    `[0].reports` 长 2 且槽序保持、`[1].exit == 0`
  - `fold_keeps_a_transport_lost_slot_empty_without_parsing_its_stdout`:槽
    exit 255 + stdout 为非 JSON 垃圾 → 不 Err,该节点 `reports` 空
    (255 槽 stdout 非报告通道,C4 语义;`is_transport_failure` 在此消费)
  - `fold_surfaces_a_protocol_violation_from_a_healthy_slot`:槽 exit 0 +
    stdout 含非 JSON 行 → `Err(ReportError::NotJson)`(不静默跳)
  - 改 `artifact_pull_argv_pins_the_rsync_command` 期望串:
    `mini:'/Users/doracawl/workspace/goliajp/smix/.smix/fed-artifacts/'`(拍板三)
  - 机械跟改:tests 内 3 处 `NodeSpec` 字面量与 `roster()` helper 加
    `runner_port: None`
- 跑红:`cargo test -p smix-cli --bin smix federation` 非零退出(`SlotResult` /
  `fold_slot_results` / `runner_port` 引用编译失败即红;pinned 串在 const 未改前红)

**绿(实现)**
- 文件:`crates/smix-cli/src/federation.rs` +
  `scripts/dev/v2.12-c4-federation-two-node-e2e.sh`(仅 line 23 变量值跟改)
- API:
  ```rust
  // NodeSpec 追加字段:
  #[serde(rename = "runnerPort", default)]
  pub runner_port: Option<u16>,

  pub const FED_ARTIFACT_DIR: &str = ".smix/fed-artifacts";

  pub struct SlotResult { pub node: usize, pub exit: u8, pub stdout: String }
  pub fn fold_slot_results(nodes: &[NodeSpec], slots: &[SlotResult])
      -> Result<Vec<NodeResult>, ReportError>
  ```
- 关键点:①折叠 = 按节点 index 归组(roster 序),节点 exit =
  `parallel::aggregate_exit(该节点各槽 exit)`(逐字复用,不重写),reports =
  各槽 `parse_report_lines` 结果按槽序 concat;②255 槽跳过 parse(空 reports),
  非 255 槽 parse 错误原样上抛;③纯函数零 IO,`merge_reports` 的直接上游
- 本步绿判:`cargo test -p smix-cli --bin smix federation` 21 passed 2 ignored;
  `cargo test -p smix-cli --bin smix` 146 passed 2 ignored

**重构**
- 无

### S2. `--nodes` CLI 挂载(删 cfg(test),federation lane 接 Run 臂)+ 05-cli.md 文档

**红(写测试)**
- 文件:`crates/smix-cli/tests/federation_run.rs`(净新,照 parallel_run.rs 范式:
  `CARGO_BIN_EXE_smix` + tempfile;每个 `Command` 一律 `.env_remove("SMIX_UDID")`
  `.env_remove("SMIX_RUNNER_PORT")` 保确定性)
- 断言(4 个 test,全 device-free、零网络拨出):
  - `nodes_conflicts_with_parallel`:`run f.yaml --nodes n.yaml --parallel 2` →
    exit 2(clap usage error)且 stderr 含 `cannot be used with`
  - `nodes_conflicts_with_device`:`run f.yaml --nodes n.yaml --device X` →
    exit 2 且 stderr 含 `cannot be used with`
  - `a_malformed_roster_is_named_and_exits_one`:temp flow 真写 + temp roster 写
    `nodes: []` → exit 1 且 stderr 含 `lists no nodes`(NodesError 显示串)
  - `a_flow_missing_locally_fails_before_any_ssh`:合法 roster(host 用
    TEST-NET-1 `192.0.2.1`,永不被拨 —— flow 检查先于 gate,实现钉此序)+
    不存在的 flow 路径 → exit 1 且 stderr 点名该 flow 路径
- 跑红:`cargo test -p smix-cli --test federation_run` 非零退出(红态 `--nodes`
  是 unknown argument,四测的 stderr/exit 断言各自失败)

**绿(实现)**
- 文件:`crates/smix-cli/src/main.rs`、`crates/smix-cli/src/federation.rs`、
  `docs/ai-guide/05-cli.md`
- API:
  ```rust
  // main.rs Run 臂新 flag:
  /// Distributed run: shard the flows across the nodes in a roster
  /// yaml (each node runs its own simulators; results merge into one
  /// JSON report on stdout, exit = worst of nodes).
  #[arg(long, conflicts_with_all = ["device", "also_device", "parallel"])]
  nodes: Option<PathBuf>,

  // federation.rs:
  #[derive(Debug, thiserror::Error)]
  pub enum FederationRunError {
      #[error("node '{node}' failed the readiness gate (stale or unreachable): {stderr}")]
      Gate { node: String, stderr: String },
      #[error("spawning ssh for node '{node}': {source}")]
      Spawn { node: String, source: std::io::Error },
      #[error(transparent)]
      Report(#[from] ReportError),
      #[error("artifact rsync for node '{node}' failed (exit {exit}): {stderr}")]
      ArtifactPull { node: String, exit: u8, stderr: String },
  }
  pub fn run_federation(
      nodes: &[NodeSpec],
      assignments: &[SlotAssignment],
      flows: &[String],
      passthrough: &[String],
      pull_to: Option<&std::path::Path>,
  ) -> Result<MergedReport, FederationRunError>
  ```
- 关键点(main.rs lane,置于 `--check` 块之后、parallel lane 之前):
  ①读 roster(`fs::read_to_string`)→ `parse_nodes`,`NodesError` →
  `CliError::Other`(exit 1);②本地 flow 存在性检查快败(点名缺失路径);
  ③base passthrough 照 parallel lane 清单组装但**不含** `--debug-output` 用户值,
  `--debug-output` 给出时追加 `["--debug-output", FED_ARTIFACT_DIR]`;全部 token
  `shell_quote`(remote 串单一确定形);④`expand_slots` → `assign_flows` →
  `run_federation(...)`;⑤Ok → `println!` merged JSON 单行(
  `serde_json::to_string`)+ `return Ok(ExitCode::from(merged.aggregate_exit))`;
  Err → `CliError::Other`(exit 1)
- 关键点(`run_federation` 内):①gate 阶段:逐节点 `run_ssh(readiness_argv)`,
  任一非零 → `Gate` 快败,**全过才扇出**;②扇出:逐槽(空 flow 槽跳过,照
  `run_parallel`)`Command::new("ssh")` + `remote_argv(node, 槽 flows, device_ref,
  槽 passthrough)`(槽 passthrough = base + 该节点 `runner_port` 存在时
  `["--runner-port", p]`),stdout `piped` / stderr `inherit`(远端进度实时可见,
  报告通道独占 stdout),**先全 spawn 再按序 join**(`wait_with_output`,exit
  clamp 照 `capture` 形,spawn 失败 → `Spawn`);每槽 join 后 eprintln 一行
  `smix run --nodes: node <name> device <ref> exited <code>`(255 追注
  `(ssh transport failure)`,`is_transport_failure` 消费);③`fold_slot_results`
  → ④`pull_to` 给出时逐节点 `run_rsync(artifact_pull_argv(node, FED_ARTIFACT_DIR,
  pull_to))`,非零 → `ArtifactPull`(**先 pull 后打印**,拍板三);⑤`merge_reports`
- 关键点(挂载):main.rs:16-20 的 `#[cfg(test)]` 与「Test-gated until C5」doc
  注释删除,换一行 doc;zero-warning build(deny dead_code)= federation 全表面
  被 runtime 消费的机器证明,残留死项必须接线或删,不许 `allow(dead_code)`
- 文档(05-cli.md):①run flag 表加一行
  `--nodes <PATH> | — | (unset) | Distributed run across machines; see below`;
  ②新小节 `### Distributed runs across machines (--nodes)`:roster 示例
  (name/host/repo/devices/runnerPort,只列 sim/emulator,§9#1)、prep 契约两步
  (rsync 源同步 exclude target/ + 远端 `cargo build --release -p smix-cli &&
  touch target/.smix-fed-stamp`;gate 拒 stale/缺 stamp)、flow 路径 repo-relative
  且各节点同路径在、merged JSON shape 一行示例、exit 语义(worst-of-nodes,
  255 = ssh transport)、`--debug-output` = 拉回 `<dir>/<node>/`(远端 staging
  `.smix/fed-artifacts`,原地覆盖不预清)、`--device/--also-device/--parallel`
  互斥 + `--format`/`--runner-port` 不进 lane(端口写 roster `runnerPort`)。
  示例命令里的 flag 必须全部真在(文档 gate ①面咬)
- 本步绿判:`cargo test -p smix-cli --test federation_run` 4 passed;
  `cargo test -p smix-cli --test documented_flags_exist` 3 passed;
  `cargo test -p smix-cli --bin smix` 146 passed 2 ignored(单测数不变 ——
  tests mod 本就 cfg(test) 编译);`cargo build --release -p smix-cli` 零警告过

**重构**
- 无

### S3. 出口 e2e 脚本:双节点经真 CLI 全链 + 无 sweep teardown + marker

**红(写测试)**
- 文件:`scripts/dev/v2.12-c5-federation-cli-e2e.sh`(净新,C4 脚本范式:
  `set -euo pipefail` + `log`/`fail`(`[c5-fed]` 前缀)+ `trap cleanup EXIT`;
  env 可覆盖:`SMIX_FED_NODE_HOST`(默认 `mini`)、`SMIX_FED_STUDIO_PORT`(默认
  `22097`)、`SMIX_FED_STUDIO_SIM`(默认 `sim-smix-02`)、`SMIX_FED_MINI_SIM`
  (默认 `sim-simx-001`);不改 C4 脚本除 S1 那 1 行变量跟改)
- 红判(guard 先失败一次,机器可判):
  `SMIX_FED_NODE_HOST=no-such-host.invalid scripts/dev/v2.12-c5-federation-cli-e2e.sh`
  → 非零退出且输出含 `[c5-fed] FAIL:`(脚本未写好前此命令因文件不存在同样非零 = 红)
- **脚本固定序**(每段机器判定,任一失败 = `fail` 停;lsof/pgrep 判定一律终端
  命令直读 `$?`,不进管道下游):
  1. **guards**:localhost 幂等自授权 + `ssh localhost true`(C4 形逐字);
     mini 可达 + `REMOTE_REPO`;双侧无活动 batch
     (`pgrep -f 'runner.ts|smix run|supervise'` 两侧无命中 —— 让位不抢占);
     构建让位(mini `pgrep -f 'cargo build|xcodebuild'` 无命中;studio 只查
     `pgrep -f 'cargo build'`,**不含裸 xcodebuild**);studio 端口
     `lsof -nP -i :$STUDIO_PORT` 无监听(22087 空闲不作依据,拍板四);
     两条 corpus flow 在;`SMIX_UDID`/`SMIX_RUNNER_PORT` 未导出断言
     (`[ -z "${SMIX_UDID:-}" ]`,防 clap env-conflict 假红)
  2. **源同步(仅 mini)**:C4 rsync 惯例逐字(exclude 集不缩水,exclude target
     保 stamp);studio repo = 源本身
  3. **config 权威同步(仅 mini)**:C4 第 3 步逐字
  4. **重建 + stamp(两节点)**:mini 经 ssh `cargo build --release -p smix-cli
     && touch target/.smix-fed-stamp`;studio 本地同命令(本地重建同时把 S1/S2
     新码打进 `target/release/smix` —— e2e 跑的就是本段产物)
  5. **gate 独立复核(两节点)**:对 localhost 与 mini 各跑 `readiness_argv` 同形
     ssh 命令 exit 0(脚本证操作序收敛,产品 gate 在 lane 内再走一遍,双跑刻意)
  6. **设备解析 + prep(两节点,§9#1 sim only,显式 UDID)**:逐节点
     `smix sim list` grep 专属 sim 名恰 1 行命中提取 UDID;`sim boot <UDID>`
     (`|| true`)+ studio `runner up <UDID_S> --bundle com.apple.Preferences
     --runner-port $STUDIO_PORT`、mini 经 ssh `runner up <UDID_M> --bundle
     com.apple.Preferences`(默认端口;两者阻塞到就绪)
  7. **真 CLI 全链**:`$WORK/nodes.yaml` 写双节点 roster(`c5-studio`: host
     `localhost` / repo `$ROOT` / devices `[<UDID_S>]` / `runnerPort:
     $STUDIO_PORT`;`c5-mini`: host `$HOST` / repo `$REMOTE_REPO` / devices
     `[<UDID_M>]`);`cd $ROOT && target/release/smix run <FLOW_A> <FLOW_B>
     --nodes $WORK/nodes.yaml --debug-output $WORK/pull > $WORK/merged.json`,
     exit 必须 0
  8. **merged 断言(python3,不引 jq 依赖)**:`$WORK/merged.json` 是单 JSON 文档
     且 `aggregateExit == 0`、`len(nodes) == 2`、节点名集合 ==
     `{c5-studio, c5-mini}`、每节点恰 1 个 flow 叶子且 `runOutcome == "success"`;
     另断言 `$WORK/pull/c5-studio/run-summary.json` 与
     `$WORK/pull/c5-mini/run-summary.json` 都存在(单 flow 批 = raw dir 根,
     C4 已查清)
  9. **teardown(trap,拍板四逐字)**:
     - studio:`PID=$(target/release/smix diagnostic store 2>/dev/null |
       python3 -c '...["one:runner-ios"]["pid"]')` → `ps -p $PID -o command=`
       含 `xcodebuild` 才 `kill -INT $PID`,循环等 ≤30s,残留 `kill -9`;
       **绝不调用 iOS 形 `smix runner down`(任何 env/flag 形)** ——
       端口无关 pkill 兜底缺陷未修(C4 事故,修法待用户拍板);stale handle
       留 store 由产品自 drop;`sim shutdown <UDID_S>`;
       `rm -rf $ROOT/.smix/fed-artifacts` + `rm -f` 三张截图 PNG
     - mini:经 ssh `runner down`(mini 无他方 runner,可用)+
       `sim shutdown <UDID_M>` + `rm -rf <repo>/.smix/fed-artifacts` +
       `rm -f` 截图 PNG
     - `rm -rf $WORK`(用完回收纪律);全程不碰 sim-insight / insight 进程
  10. **marker**:全过后末行 `[c5-fed] C5-FED-E2E-PASS`

**绿(实现)**
- 默认参数真跑:`scripts/dev/v2.12-c5-federation-cli-e2e.sh` → 末行含
  `C5-FED-E2E-PASS`、exit 0(一条 `smix run --nodes` 命令驱动双机各自 sim 并发
  跑 1 条 flow、merged JSON 单文档落盘、双份 run-summary.json 回收 —— federation
  全回路经产品 CLI 面闭合)

**重构**
- 无

## Checkpoint C5 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— device-free:21 单测 + 2 ignored,全量 146,CLI 面 4 测,文档 gate 3 测,
#    runtime caller 在(cfg(test) 已删的机器证明),c4 名残留清零,对手零改 ——
cargo test -p smix-cli --bin smix federation 2>&1 | grep -q 'ok. 21 passed; 0 failed; 2 ignored' \
  && cargo test -p smix-cli --bin smix 2>&1 | grep -q 'ok. 146 passed; 0 failed; 2 ignored' \
  && cargo test -p smix-cli --test federation_run 2>&1 | grep -q 'ok. 4 passed; 0 failed' \
  && cargo test -p smix-cli --test documented_flags_exist 2>&1 | grep -q 'ok. 3 passed; 0 failed' \
  && grep -q 'pub fn run_federation' crates/smix-cli/src/federation.rs \
  && grep -q 'federation::run_federation' crates/smix-cli/src/main.rs \
  && ! grep -B4 '^mod federation;' crates/smix-cli/src/main.rs | grep -q 'cfg(test)' \
  && ! grep -rq 'fed-c4-artifacts' crates/ scripts/ docs/ai-guide/ \
  && grep -q -- '--nodes' docs/ai-guide/05-cli.md \
  && git diff --quiet crates/smix-cli/src/parallel.rs crates/smix-adapter-maestro/src/entry.rs crates/smix-cli/src/runner.rs \
  && echo FEDERATION-C5-UNIT-OK
```

```bash
# —— opt-in 双节点出口 e2e(需 mini 可达 + 双侧无活动 batch;跑完自回收)——
scripts/dev/v2.12-c5-federation-cli-e2e.sh
```

期望:第一块打印 `FEDERATION-C5-UNIT-OK` 各命令 exit 0;第二块 exit 0 且末行含
`C5-FED-E2E-PASS`。含义 = `--nodes` CLI 面(互斥 / roster 错报 / flow 快败)与
折叠语义被测试钉死、文档与 CLI 一致、federation 获得 runtime caller 且
`runner.rs`(sweep 缺陷区)一字未动、真双节点经产品 CLI 一条命令全回路闭合 →
**v2.12 五 C 全过 = 冷计划出口验收成立**。

**诚实划界**:device-free = S1 全部 + S2 全部(含文档 gate);**必须双节点** =
S3 全程。C5 扇出为真**并发**(spawn-all-then-join)—— C4 e2e 是顺序,此并发
wiring 的行为验证只在 S3 e2e(单测不钉并发时序,只钉折叠语义)。**不在 C5 内**:
`runner down` 端口无关 sweep 缺陷修复(修法候选已录 v2.md 决策日志,待用户拍板,
本段绝不顺手改)、insight runner 恢复(不知其 up 参数,盲启 = 二次干扰,待用户)、
merged-junit emitter(C1 轴 D 定 JSON 包裹层)、同步/重建进 CLI(拍板二,明确
不做)、`--parallel`/`--also-device` 的 05-cli.md 补文档(前期缺,§8.1 不顺手,
已列入决策日志候选)。

## 与 C1/C4/冷计划假设不符处(热化期发现,如实列)

1. **22087 现状与 C4 期不同**:insight 的 22087 runner 已不在(C4 事故余波,
   lsof exit 1 无监听)。C5 不据此放松:studio 仍 22097 隔离,insight 可能随时
   重启(拍板四)。
2. **clap conflict 与 env 的交互是真实约束**:`--device` 带 `env = "SMIX_UDID"`,
   clap 对 env 来源的值同样触发 conflict → 导出 SMIX_UDID 的 shell 里
   `--nodes` 会报互斥。判定 = fail-fast 正确不特判;测试与 e2e 显式清 env
   (S2 `env_remove` / S3 guard 断言未导出)。C1/冷计划未预见此交互。
3. **`smix diagnostic store` 的 stdout 纯净性成立**(kevy AOF 行走 stderr,实测)
   —— teardown 的 handle-pid 提取可直接 `2>/dev/null | python3`;若它日 AOF 行
   混入 stdout,脚本 python3 解析会显式炸(不静默)。
4. **05-cli.md run 表缺 `--parallel`/`--also-device`**:v2.8-C4 挂 CLI 时未补文档,
   文档 gate 方向(docs→CLI)咬不到。C5 范围只加 `--nodes`;此缺列决策日志备忘,
   不顺手修(§8.1)。
5. **`FED_ARTIFACT_DIR` 的 c4 命名成为永久面**:C4 定名时它只活在 e2e;C5 挂 CLI
   后它进入文档化产品机制 → 去 checkpoint 名(拍板三),含 C4 脚本 1 行跟改
   —— 这是对 C4 假设「artifact 目录名无关紧要」的修正。
6. 其余与假设**相符**(实测证):C4 基线 17/142 + 2 ignored;mini 可达 + 二进制在;
   localhost 自授权仍在;`sim-smix-02` 在(Shutdown)/ `sim-insight` Booted;
   corpus 双 flow 在;`parallel_run.rs`/tempfile 范式可照搬;文档 gate 三面行为
   与 v2.md 记载一致。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.12-c5-hot.md`。
2. `docs/v2.md` 决策日志加行:①C5 闭合(`--nodes` CLI 形 + 三互斥 + roster
   runnerPort + 同步不进 CLI + artifact 回收进 CLI + fed-artifacts 去 c4 名);
   ②teardown 无 sweep 纪律实践(recorded-handle 精确收,sweep 修法仍待拍板);
   ③**v2.12 阶段闭合 + v2.8–v2.12 折入阶段全部完成声明:v2.0.0 ship 决策交还
   用户,零 publish,不自作主张发布**;④备忘:05-cli.md run 表补
   `--parallel`/`--also-device` 文档待用户授权。
3. **不热化下一段**(折入阶段收官,无 C6);等用户 ship 拍板。

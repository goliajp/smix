# plan-hot — v2.12 到 C4:结果合并(merged CI 报告 + worst-of-nodes 聚合 + 双节点 e2e)

## 目标 checkpoint

C4:**N 个节点的 run report 合并为单一 merged CI 报告 + 聚合退出码,机器可判**。通过后
世界变成:`crates/smix-cli/src/federation.rs` 长出合并腿 —— ①merged schema(纯包裹层
`{nodes:[{node, exit, flows:[<每行 JSON 叶子原样>]}], aggregateExit}`,serde 序列化,
C1 轴 D 定形)+ `merge_reports`(聚合 = `parallel::aggregate_exit` 逐字复用,255 哨兵
max-胜)、②artifact 回收腿(`artifact_pull_argv` rsync argv 纯函数 + `run_rsync` spawn,
per-node 子目录归置)—— device-free fixture 单测钉死(federation 17 测 2 ignored,bin
全量 142);加 1 个 `#[ignore]` env-gated **双节点** e2e 测试(2 节点 = 本机 studio 经
`ssh localhost` + mini,全链:parse_nodes → expand_slots → assign_flows → per-node
gate → run_ssh → parse_report_lines → merge_reports → artifact 回收 → 本地
run-summary.json 断言)。opt-in 脚本 `scripts/dev/v2.12-c4-federation-two-node-e2e.sh`
走完 自授权 → 源同步 → 重建 + stamp → gate → 双节点设备 prep → 驱动 ignored 测试 →
teardown,末行 marker `C4-FED-E2E-PASS` + exit 0。§9#1:两节点跑的全是 iOS sim。
CLI 收口 `smix run --nodes` 归 C5 —— 本段不碰。

**C4 拍板一(artifact 回收进 C4,单路径)**:冷计划 C4 概要明文「run report/**artifact**
合并为单一 merged CI 报告」,C3 明文「artifact 文件回收归 C4」—— 再 defer = scope 缩水
(exec/no-shrink-words),不做。形 = C1 轴 D-3 定的 rsync 回收:`--debug-output` 落远端
盘(相对 repo 的 bare-safe 目录,经 passthrough 转发),跑完 rsync 拉回 scheduler 侧
per-node 子目录(keyed by 节点名,防 basename 撞名 —— 包裹层解决,bundle 内容零改)。
merged JSON 里**不**内联 artifact 字节:叶子 JSON 行已含 per-flow verdict/steps,artifact
是文件面,归置到目录即回收完成(与单机 `--debug-output` 消费形一致)。

**C4 拍板二(节点 2 = 本机 studio 经 `ssh localhost`,真双节点)**:冷计划钉的就是
「双节点(本机 + mini)」。(a) 本机作节点 = 2 个 `NodeSpec`、2 条真 ssh transport、
2 台机器各自设备、2 份独立 report 流 —— merge 的输入是真 N=2;(b)「mini 单节点两槽」
是单节点,merge 输入 N=1,验不了跨节点合并,判伪双节点。§13 真 > 伪,选 (a)。
探测现状:本机 sshd 活着(`ssh localhost` 应答 publickey denied),但本机公钥不在
`~/.ssh/authorized_keys` → e2e 脚本 prep 做**幂等自授权**(公钥已在则跳过,不在则
append 一行;host key 用 `StrictHostKeyChecking=accept-new` 在 guard 步固定,产品侧
`run_ssh` 保持 BatchMode-only 零特殊分支)。nodes.yaml 语义对两节点完全同形。

**C4 拍板三(studio 节点 runner 走 dedicated port,与 insight 共存)**:本机 22087 此刻
被 insight 的活动 runner capsule 占用(实测 `SmixRunnerUITests-Runner` LISTEN 22087 +
其 xcodebuild 常驻)。产品现成机制 = per-run `--runner-port` flag(`smix run` 与
`runner up` 双侧都有;`main.rs:1888-1896` 优先级 flag → registry → env → 22087)→
studio 节点的 passthrough 带 `--runner-port <port>`(默认 22097,lsof guard 先证空闲),
mini 保持默认 22087(C3 已证)。零 registry 写入、零持久状态。teardown 侧 iOS
`runner down` **无 port flag**,端口只认 env(`main.rs:1406-1424` → `runner_port()`)→
studio 必须 `SMIX_RUNNER_PORT=<port> smix runner down`,**绝不裸跑**(裸跑打 22087 =
误杀 insight 的 runner)。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
grep -q 'pub fn parse_report_lines' crates/smix-cli/src/federation.rs   # C3 产物在
grep -q 'pub fn run_ssh' crates/smix-cli/src/federation.rs              # C3 产物在
! grep -q 'pub fn merge_reports' crates/smix-cli/src/federation.rs      # 合并腿净新
cargo test -p smix-cli --bin smix federation 2>&1 | grep -q 'ok. 12 passed; 0 failed; 1 ignored'  # C3 基线绿
cargo test -p smix-cli --bin smix 2>&1 | grep -q 'ok. 137 passed; 0 failed; 1 ignored'            # 全量基线绿
ssh -o ConnectTimeout=5 -o BatchMode=yes mini \
  'test -x ~/workspace/goliajp/smix/target/release/smix'                # mini 可达 + 二进制在
xcrun simctl list devices available | grep -q 'sim-smix-02'             # 本机节点设备在
test -x target/release/smix                                             # 本机二进制在(设备解析用)
test -f scripts/release/stress-corpus/launch-and-capture.yaml           # e2e 黄金 flow 在
test -f scripts/release/stress-corpus/screenshot-twice.yaml             # e2e 黄金 flow 在
```

全部 exit 0 = 可开工(2026-07-24 热化期已逐条实跑,全过)。任一失败 → 按 §6 拒绝开工回报。

## 已经查清、不必重查的事实(热化期实测,2026-07-24)

- **ssh localhost 现状**:sshd 应答(`Permission denied (publickey,...)`,EXIT=255)——
  服务活着,只缺授权;`~/.ssh/authorized_keys` 存在(1 行,不含本机 `id_ed25519.pub`),
  自授权 = 幂等 append 一行。known_hosts 已收 localhost 条目(探测期 accept)。
- **本机没有 `sim-simx-001`**(那是 mini 的专属名);本机专属 dev sim = `sim-smix-02`
  …`sim-smix-05`(iOS 26.5,全 Shutdown)。`sim-insight` 此刻 Booted,属 insight,
  不碰(sim-guard:全程显式 UDID)。
- **本机 22087 被占**:insight 的 `SmixRunnerUITests-Runner`(sim FFC57DAE)LISTEN
  22087,其 xcodebuild(`~/.local/share/smix/runner/SmixRunner.xcodeproj`,capsule 常驻)
  在跑 → studio 侧 guard **不得**含裸 `pgrep xcodebuild`(永久红),只 guard
  `cargo build`;端口隔离靠拍板三。v2.8-C4 已证同机多 XCUITest 会话共存。
- **`runner down`(iOS)无 `--device`/port flag**(`--device` 是 Android-only;C3 脚本
  传的 `--device $UDID` 实际被忽略)。iOS down 端口 = env `SMIX_RUNNER_PORT` → 22087
  (`main.rs:1406-1424`,`runner_port()` `main.rs:2533-2538`)。
- **本机 `.smix/` 无 `sims.json`**(与 mini 同)→ 双节点 devices 均用运行期解析的
  raw UDID(C3 同形,`resolve_device` 免 registry 路径)。
- **C3 产物真实形**(federation.rs 实读):`FlowReport { flow, outcome, raw:
  serde_json::Value }`(raw = 整行叶子,C3 注明「C4 合并器的输入,C3 不加工」)/
  `RemoteOutput { exit: u8, stdout, stderr }` / `run_ssh(argv) -> io::Result<RemoteOutput>` /
  `readiness_argv` / `remote_argv(node, flows, device_ref, passthrough)`(passthrough
  **逐字**进 remote 串,不加 quote → passthrough token 必须 bare-safe)。
- **聚合对手**:`pub fn aggregate_exit(shard_codes: &[u8]) -> u8` = max,空集 0
  (`parallel.rs:46-48`);`SSH_TRANSPORT_EXIT = 255` 与 smix 码空间不相交、天然 max-胜
  (federation.rs 已钉)。
- **`--debug-output` 落盘形**:单 flow 批 = raw dir(`run-summary.json` 在目录根);
  多 flow 才 per-flow 子目录(`main.rs:1920-1928` `multi_flow` 分支)。e2e 每节点恰
  1 flow → 远端 debug 目录根直接有 `run-summary.json`,回收断言简单。
- **CI 报告面参照**:workspace 现成 junit emitter 只有 per-flow 单 testsuite 形
  (`emit_junit`,`crates/smix-adapter-maestro/src/entry.rs:559`)—— merged 报告按
  C1 轴 D 走 JSON 包裹层,**不**新造 merged-junit 面(叶子零改 = adapter 零改动)。
- **测试基线**:federation 12 passed 1 ignored / bin 全量 137 passed 1 ignored(实跑)。
  C4 加 5 个 device-free 单测 + 1 个 ignored → 期望 federation 17 passed 2 ignored、
  全量 142 passed 2 ignored。serde(derive)/serde_json 已在依赖,零新依赖。
- **mini 现状**:BatchMode 通;`smix sim list` EXIT 0,`sim-simx-001` 在(Shutdown,
  iOS-26-5);C3 e2e 全链(同步/重建/gate/prep/teardown)上周期刚绿,脚本范式照搬。
- **脚本范式**:`scripts/dev/v2.12-c3-federation-single-node-e2e.sh`(guards → rsync →
  config 权威同步 → 重建+stamp → gate 复核 → 设备 prep → 驱动 ignored 测试 → trap
  teardown → marker)。C4 **新开脚本**,不改 C3 脚本(C3 验收产物保持可跑)。

## 步骤(线性,3 个)

### S1. merged schema + `merge_reports`(device-free 纯逻辑)

**红(写测试)**
- 文件:`crates/smix-cli/src/federation.rs`(tests mod 追加)
- 断言(4 个 test):
  - `merges_two_nodes_into_one_wrapped_report_pinning_the_json`:两个 `NodeResult`
    (真实 shape 叶子:success 行含 `warnings`/`steps`)→ `MergedReport`,
    `serde_json::to_string` **逐字节钉死**(形:`{"nodes":[{"node":"a","exit":0,
    "flows":[{…叶子原样…}]},…],"aggregateExit":0}`)—— 包裹层字段只有
    node/exit/flows + aggregateExit,叶子 Value 原样
  - `transport_255_node_merges_empty_and_wins_the_aggregate`:节点 b = exit 255 +
    零 report(transport 丢失,stdout 不可用)→ merged 仍含该节点
    (`flows: []`),`aggregateExit == 255` 且 `is_transport_failure(aggregate)` 真
  - `a_node_with_all_failed_flows_keeps_failure_leaves_verbatim`:某节点 exit 3、
    两行全 failure(含 `failure.code`)→ merged `flows[i]["failure"]["code"]` 可取
    (叶子零加工),`aggregateExit == 3`
  - `empty_inputs_merge_to_exit_zero`:`merge_reports(&[])` → `nodes: []` +
    `aggregateExit == 0`(对手空集语义逐字);节点 exit 0 + 零 flow(槽多 flow 少的
    空分配节点)→ 该节点 `flows: []` 且不拉高聚合
- 跑红:`cargo test -p smix-cli --bin smix federation` 非零退出(API 未实现,编译失败即红)

**绿(实现)**
- 文件:`crates/smix-cli/src/federation.rs`
- API:
  ```rust
  pub struct NodeResult { pub name: String, pub exit: u8, pub reports: Vec<FlowReport> }
  #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
  pub struct MergedNode { pub node: String, pub exit: u8, pub flows: Vec<serde_json::Value> }
  #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
  #[serde(rename_all = "camelCase")]
  pub struct MergedReport { pub nodes: Vec<MergedNode>, pub aggregate_exit: u8 }
  pub fn merge_reports(results: &[NodeResult]) -> MergedReport
  ```
- 关键点:①聚合 = `crate::parallel::aggregate_exit(&exits)` 逐字复用(worst-of-nodes,
  空集 0,255 哨兵天然 max-胜 —— 不重写聚合语义);②`flows` = `FlowReport.raw` 原样
  clone(C1 轴 D:merged schema 纯包裹层,叶子零改);③序列化就是
  `serde_json::to_string(&merged)`,不另造 emit 函数(CI 消费单 JSON 文档)

**重构**
- 无

### S2. artifact 回收腿 + 双节点 e2e ignored 测试

**红(写测试)**
- 文件:`crates/smix-cli/src/federation.rs`(tests mod 追加)
- 断言(1 个单测 + 1 个 `#[ignore]`):
  - `artifact_pull_argv_pins_the_rsync_command`:对 host=mini、
    repo=`/Users/doracawl/workspace/goliajp/smix` 的节点,
    `artifact_pull_argv(&node, FED_ARTIFACT_DIR, "/tmp/pull")` 逐字 =
    `["-a", "mini:'/Users/doracawl/workspace/goliajp/smix/.smix/fed-c4-artifacts/'", "/tmp/pull/mini/"]`
    (`"rsync"` 词不含,同 `readiness_argv` 约定;远端路径 `shell_quote`;双侧尾 `/`
    = 目录内容语义;本地侧 per-node 子目录 keyed by `node.name`)
  - `federation_e2e_two_nodes_merge_reports_and_recover_artifacts`(`#[ignore]`):
    env 取 `SMIX_FED_E2E_NODES` / `SMIX_FED_E2E_FLOWS`(2 条)/
    `SMIX_FED_E2E_RUNNER_PORTS`(可选,`name=port` 逗号表)/ `SMIX_FED_E2E_PULL_DIR`
    (缺失即 panic 报「由 e2e 脚本驱动」)。全链:`parse_nodes` 断言 2 节点 →
    `expand_slots` 断言 2 槽 → `assign_flows(2)` 断言每槽恰 1 flow → 逐节点:
    `run_ssh(readiness_argv)` exit 0 → passthrough =
    `["--debug-output", FED_ARTIFACT_DIR]`(+ 该节点在 ports 表时
    `["--runner-port", <port>]`,token 全 bare-safe)→ `run_ssh(remote_argv(…))`
    exit 0 且 `!is_transport_failure` → `parse_report_lines` 断言 1 行、
    `outcome == "success"`、`flow` 与分配一致 → 收 `NodeResult` → `merge_reports`
    断言 `nodes.len() == 2`、`aggregate_exit == 0`、`serde_json::to_string` 含两个
    节点名 → 逐节点 `run_rsync(artifact_pull_argv(…))` exit 0 →
    断言 `<pull_dir>/<node.name>/run-summary.json` 两个都存在(单 flow 批 = raw dir,
    `run-summary.json` 在根,已查清)
- 跑红:`cargo test -p smix-cli --bin smix federation` 非零退出(`artifact_pull_argv` /
  `run_rsync` / `NodeResult` 引用编译失败即红)

**绿(实现)**
- 文件:`crates/smix-cli/src/federation.rs`
- API:
  ```rust
  pub const FED_ARTIFACT_DIR: &str = ".smix/fed-c4-artifacts";
  pub fn artifact_pull_argv(node: &NodeSpec, remote_dir: &str, local_dir: &str) -> Vec<String>
  pub fn run_rsync(argv: &[String]) -> std::io::Result<RemoteOutput>
  ```
- 关键点:①`artifact_pull_argv` = `["-a", "<host>:<shell_quote(repo/remote_dir)>/",
  "<local_dir>/<node.name>/"]`,纯函数;②`run_rsync` 与 `run_ssh` 同形
  (`Command::new("rsync")`,exit clamp、双流 lossy 捕获,不重试不 timeout ——
  rsync 传输失败非零显式浮出);③`--debug-output` 转发走 C2/C3 已有的 passthrough
  形,federation.rs 对 `remote_argv` **零改动**;④ignored 测试不进默认 suite
- 本步 device-free 绿判:`cargo test -p smix-cli --bin smix federation` 17 passed
  2 ignored(e2e 测试编译进但不跑);e2e 真跑归 S3 脚本驱动

**重构**
- 可选:`run_ssh` / `run_rsync` 共 spawn 体抽私有 `capture(program, argv)`
  (两处同形 clamp + lossy;测试保持绿)

### S3. 双节点 e2e 脚本:自授权 → 同步 → gate → 双节点 prep → 驱动 → teardown → marker

**红(写测试)**
- 文件:`scripts/dev/v2.12-c4-federation-two-node-e2e.sh`(照 C3 脚本范式:
  `set -euo pipefail` + `log`/`fail`(`[c4-fed]` 前缀)+ `trap cleanup EXIT`;
  env 可覆盖:`SMIX_FED_NODE_HOST`(默认 `mini`)、`SMIX_FED_STUDIO_PORT`(默认
  `22097`)、`SMIX_FED_STUDIO_SIM`(默认 `sim-smix-02`)、`SMIX_FED_MINI_SIM`
  (默认 `sim-simx-001`))
- 红判(guard 先失败一次,机器可判):
  `SMIX_FED_NODE_HOST=no-such-host.invalid scripts/dev/v2.12-c4-federation-two-node-e2e.sh`
  → 非零退出且输出含 `[c4-fed] FAIL:`(脚本未写好前此命令因文件不存在同样非零 = 红)
- **脚本固定序**(每段带机器判定,任一失败 = `fail` 停):
  1. **guards**:
     - localhost 自授权(拍板二):`grep -qF "$(awk '{print $2}' ~/.ssh/id_ed25519.pub)"
       ~/.ssh/authorized_keys || cat ~/.ssh/id_ed25519.pub >> ~/.ssh/authorized_keys`
       (幂等,append 时 log 一行);随后
       `ssh -o ConnectTimeout=5 -o BatchMode=yes -o StrictHostKeyChecking=accept-new localhost true`
       必须 exit 0(host key 在此固定,后续产品侧 BatchMode-only `run_ssh` 零特殊分支)
     - mini 可达 + `REMOTE_REPO="$(rssh "cd $REPO && pwd")"`
     - 双侧无活动 batch:`pgrep -f 'runner.ts|smix run|supervise'` studio 与 mini
       都必须无命中(让位不抢占)
     - 构建让位:mini `pgrep -f 'cargo build|xcodebuild'` 无命中;studio 只查
       `pgrep -f 'cargo build'`(**不含 xcodebuild** —— insight capsule 常驻合法,
       已查清)
     - studio runner 端口空闲:`lsof -nP -i :$SMIX_FED_STUDIO_PORT` 无输出
     - 两条 corpus flow 文件存在
  2. **源同步(仅 mini)**:C3 惯例 rsync 逐字(exclude 集不缩水)。studio 节点
     repo = 源本身(`$ROOT`),不自拷(诚实分支,理由:源权威即本机)
  3. **config 权威同步(仅 mini)**:C3 第 3 步逐字(studio 侧自身即权威,无动作)
  4. **重建 + stamp(两节点)**:mini = `rssh 'cd <repo> && cargo build --release
     -p smix-cli && touch target/.smix-fed-stamp'`;studio = 本地同命令(失败即停,
     不带病跑)
  5. **gate 独立复核(两节点)**:对 localhost 与 mini 各跑一次 `readiness_argv`
     同形 ssh 命令,exit 0 才继续(脚本证操作序收敛,测试证产品 gate 函数,双跑刻意
     —— C3 同理)
  6. **设备解析 + prep(两节点,§9#1 sim only,显式 UDID)**:逐节点从
     `target/release/smix sim list`(studio 本地跑,mini 经 ssh)grep 专属 sim 名,
     断言恰 1 行命中,提取 UDID;`sim boot <UDID>`(`|| true`)+ `runner up <UDID>
     --bundle com.apple.Preferences`(mini 默认端口;studio 加
     `--runner-port $SMIX_FED_STUDIO_PORT`;两者都阻塞到就绪)
  7. **驱动 ignored 测试**:mktemp 写双节点 nodes.yaml
     (`c4-studio`: host `localhost` / repo `$ROOT` / devices `[<UDID_S>]`;
     `c4-mini`: host `$HOST` / repo `$REMOTE_REPO` / devices `[<UDID_M>]`),
     `SMIX_FED_E2E_NODES=… SMIX_FED_E2E_FLOWS='<launch-and-capture>,<screenshot-twice>'
     SMIX_FED_E2E_RUNNER_PORTS="c4-studio=$SMIX_FED_STUDIO_PORT"
     SMIX_FED_E2E_PULL_DIR=$WORK/pull
     cargo test -p smix-cli --bin smix federation_e2e_two_nodes -- --ignored --nocapture`,
     exit 0;随后脚本独立复核 `$WORK/pull/c4-studio/run-summary.json` 与
     `$WORK/pull/c4-mini/run-summary.json` 都存在(回收落地双跑复核)
  8. **teardown(trap)**:mini = `runner down` + `sim shutdown <UDID_M>` + 远端
     `rm -rf <repo>/.smix/fed-c4-artifacts` + `rm -f` 截图 PNG(经 ssh);studio =
     `SMIX_RUNNER_PORT=$SMIX_FED_STUDIO_PORT target/release/smix runner down`
     (**必带 env,绝不裸跑** —— 拍板三)+ `sim shutdown <UDID_S>` + 本地
     `rm -rf .smix/fed-c4-artifacts` + `rm -f launch-capture.png shot-1.png shot-2.png`;
     `rm -rf $WORK`(用完回收纪律)
  9. **marker**:全过后末行 `[c4-fed] C4-FED-E2E-PASS`

**绿(实现)**
- 默认参数真跑:`scripts/dev/v2.12-c4-federation-two-node-e2e.sh` → 末行含
  `C4-FED-E2E-PASS`、exit 0(两节点各真 sim 跑 1 条 flow、两份 JSON 行流合并为单一
  merged 报告 aggregateExit 0、两份 run-summary.json 回收到本地 per-node 子目录 ——
  这就是 S2 ignored 测试的真跑绿)

**重构**
- 无

## Checkpoint C4 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— device-free:17 单测 + 2 ignored,全量 142 零红,合并腿表面在,对手与叶子零改动 ——
cargo test -p smix-cli --bin smix federation 2>&1 | grep -q 'ok. 17 passed; 0 failed; 2 ignored' \
  && cargo test -p smix-cli --bin smix 2>&1 | grep -q 'ok. 142 passed; 0 failed; 2 ignored' \
  && grep -q 'pub fn merge_reports' crates/smix-cli/src/federation.rs \
  && grep -q 'pub struct MergedReport' crates/smix-cli/src/federation.rs \
  && grep -q 'pub fn artifact_pull_argv' crates/smix-cli/src/federation.rs \
  && git diff --quiet crates/smix-cli/src/parallel.rs crates/smix-adapter-maestro/src/entry.rs \
  && echo FEDERATION-C4-UNIT-OK
```

```bash
# —— opt-in 双节点设备 e2e(需 mini 可达 + 双侧无活动 batch;跑完自回收)——
scripts/dev/v2.12-c4-federation-two-node-e2e.sh
```

期望:第一块打印 `FEDERATION-C4-UNIT-OK` 各命令 exit 0;第二块 exit 0 且末行含
`C4-FED-E2E-PASS`。含义 = merged schema/聚合/回收 argv 被 fixture 单测钉死
(含 255 哨兵、全败节点、空输入边角)、全量零回归、聚合对手 `parallel.rs` 与叶子
emitter `entry.rs` 一字未动;真双节点(studio + mini)各自 sim 跑 flow → 双 JSON 流
合并 → artifact 双向回收 整条回路真通。

**诚实划界**:device-free = S1 全部 + S2 的编译与 17 单测;**必须双节点** = S2 ignored
测试的真跑与 S3 脚本全程。e2e 内两节点 `run_ssh` 为**顺序**执行 —— C4 验的是合并语义,
双节点**并发** spawn-all-then-join 属 CLI 编排 wiring,归 C5。**不在 C4 验收内**:
CLI 收口 `smix run --nodes` 与 `mod federation` 删 `#[cfg(test)]`(C5);merged-junit
emitter(不新造面,merged 报告 = JSON 包裹层,C1 轴 D 定形);给任何一侧建 sims.json
registry(raw UDID 免 registry 路径足够,C3 同判)。

## 与 C1/C3/冷计划假设不符处(热化期发现,如实列)

1. **冷计划「双节点(本机 + mini)」预设本机可当节点,但 `ssh localhost` 现状被拒**:
   sshd 活着(应答 publickey denied),本机公钥不在自己的 `authorized_keys`(实测
   1 行,不含 `id_ed25519.pub`)。→ e2e 脚本 guard 步做幂等自授权 + accept-new 固
   host key(拍板二),产品代码零特殊分支。
2. **本机没有 `sim-simx-001`**:该专属名在 mini 上;本机专属 dev sim 实为
   `sim-smix-02`…`05`(iOS 26.5)。→ studio 节点设备 = `sim-smix-02`(运行期解析
   raw UDID)。memory 里「dev sim 专属名 sim-simx-001」对本机不成立,已实查。
3. **本机 22087 被 insight 活动 runner 占用 + 其 xcodebuild 常驻**(实测 LISTEN +
   进程在):C1/冷计划未预见「scheduler 机自身作节点时与第三方 dogfood runner 共存」。
   → 拍板三:studio 节点 dedicated `--runner-port`(passthrough + `runner up` flag +
   teardown env),studio guard 不含裸 xcodebuild 检查。
4. **iOS `runner down` 无 `--device`/port flag**:C3 脚本的 `runner down --device
   $UDID` 里 `--device` 是 Android-only 参数,iOS 路径实际忽略(C3 未受害:mini 上
   仅默认端口一个 runner)。C4 的 studio teardown 若沿用该形会打错端口 → 必须
   `SMIX_RUNNER_PORT` env 形(拍板三)。
5. 其余与假设**相符**(实测证):mini 可达 + `sim-simx-001` 在;`FlowReport.raw`
   即合并器输入(C3 注明形与实现一致);`--debug-output` 经 passthrough 可转发;
   单 flow 批 `run-summary.json` 落 debug 目录根;聚合对手 `aggregate_exit` 签名形
   与 C1 记载一致;基线 12/137 + 1 ignored 实跑吻合。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.12-c4-hot.md`。
2. `docs/v2.md` 决策日志加三行:①artifact rsync 回收进 C4(冷计划兑现,merged JSON
   不内联 artifact 字节);②节点 2 = 本机经 ssh localhost(真双节点 > 伪双节点,
   幂等自授权进 e2e 脚本);③studio 节点 dedicated runner port 与 insight 共存 +
   iOS `runner down` 端口只认 env 的发现。
3. 由用户/上层拍板后热化 C5(CLI 收口 + 文档 + 出口 e2e,v2.12 闭合),见 CLAUDE.md §6。

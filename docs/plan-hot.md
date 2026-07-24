# plan-hot — v2.12 到 C3:单远程节点执行腿(scheduler 驱动 mini 跑真 flow)

## 目标 checkpoint

C3:**scheduler 机(studio)真的驱动 mini 跑 flow,回路每段机器可判**。通过后世界变成:
`crates/smix-cli/src/federation.rs` 长出执行腿三件 —— ①`parse_report_lines`(远端 stdout
JSON 行 → 逐 flow report,非 JSON 行 = 协议违约报错,fail-safe)、②`readiness_argv`
(新鲜度 gate 的 ssh argv,build-stamp 参照,纯函数钉形)、③`run_ssh`(真 spawn `ssh`,
捕获 stdout/stderr/退出码,255 哨兵可辨)—— 纯逻辑被 3 个新 device-free 单测钉死
(federation 12 测,bin 全量 137);加 1 个 `#[ignore]` env-gated 单节点 e2e 测试
(parse_nodes → expand_slots → assign_flows → readiness gate → `remote_argv` → `run_ssh` →
JSON 行断言,全链消费 C2 产物)。opt-in 脚本 `scripts/dev/v2.12-c3-federation-single-node-e2e.sh`
走完 源同步 → config 权威同步 → 远端重建 + stamp → 设备 prep → 驱动 ignored 测试 → teardown,
末行 marker `C3-FED-E2E-PASS` + exit 0。§9#1:mini 上跑的全是 iOS sim。
多节点合并归 C4,artifact 文件回收归 C4,CLI 收口归 C5 —— 本段不碰。

**C3 拍板一(新鲜度 gate 单路径 = 同步后必重建 + build-stamp 收敛断言)**:
rsync(`-a` 保 mtime)→ mini 上 `cargo build --release -p smix-cli`(cargo 是 freshness
的权威裁判;增量 build,源未变时近零成本)→ 成功后 `touch target/.smix-fed-stamp` →
gate = `test -f stamp && test -x smix && find crates -name '*.rs' -newer stamp 为空`
(stamp 缺失 = stale,fail-safe 方向)。
- 不采「检测 + 拒跑」:mini 现行就 stale(C1 实测 3 文件),且 rsync 每次同步都刷新源
  mtime → 拒跑形永久红,只能靠人工外带重建解锁,违 §5 机器可判 + §12.2 能力必补。
- 不采 C1 (d) 最强形「build-stamp 进 `--version`」:git hash 在 mini 上编不出(`.git` 是
  worktree 指针,C1 发现);时间戳嵌入则要强制每次 relink 且动 `--version` 表面。
  stamp 文件形零二进制改动,交付同等机器可判保证。
- **gate 参照物从 C1 (c) 的「二进制 mtime」改为 stamp 文件**(热化期新发现,见下节
  「与 C1/冷计划假设不符」第 1 条):C1 (c) 形与重建策略组合会死锁,不可沿用。

**C3 拍板二(执行腿落点 = `federation.rs` 本文件)**:`parallel.rs` 先例 —— 纯逻辑
(`shard_flows`)与 spawn(`run_parallel`)同文件同工艺。执行腿是调度核心的收尾一格,
不另立文件。`mod federation;` 保持 `#[cfg(test)]` 挂载:C3 的全部消费方是测试(单测 +
ignored e2e),无 runtime caller;删 cfg = zero-warning build 的 dead-code deny 直接红。
删 cfg 归 C5(CLI wiring 接上第一个 runtime caller 时);`main.rs:15-17` 的挂载注释
「until C3 wires the ssh execution leg」改为 until C5,不留过时断言(注释是断言纪律)。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
grep -q 'pub fn remote_argv' crates/smix-cli/src/federation.rs        # C2 产物在
grep -q 'pub fn parse_nodes' crates/smix-cli/src/federation.rs        # C2 产物在
! grep -q 'pub fn run_ssh' crates/smix-cli/src/federation.rs          # 执行腿净新
cargo test -p smix-cli --bin smix federation 2>&1 | grep -q 'ok. 9 passed'    # C2 基线绿
cargo test -p smix-cli --bin smix 2>&1 | grep -q 'ok. 134 passed; 0 failed'   # 全量基线绿
ssh -o ConnectTimeout=5 -o BatchMode=yes mini \
  'test -x ~/workspace/goliajp/smix/target/release/smix'              # mini 可达 + 二进制在
test -f scripts/release/stress-corpus/launch-and-capture.yaml         # e2e 黄金 flow 在
test -f scripts/release/stress-corpus/screenshot-twice.yaml           # e2e 黄金 flow 在
```

全部 exit 0 = 可开工(2026-07-24 热化期已逐条实跑,全过)。任一失败 → 按 §6 拒绝开工回报。

## 已经查清、不必重查的事实(热化期实测,2026-07-24)

- **mini 现状**:SSH BatchMode 通;`target/release/smix` mtime = Jul 23 23:15,
  `find crates -name '*.rs' -newer target/release/smix` = 3 文件(`smix-error/tests/
  sdk_driving_parity.rs`、`smix-node/build.rs`、`smix-node/src/lib.rs`)—— **现行轻度
  stale,C1 发现仍成立**;`cargo 1.97.1` 非交互 ssh 直接可用(`/Users/doracawl/.cargo/
  bin/cargo` 在 PATH);iOS 26.5 runtime,`sim-simx-001`(名字)在 sim list 中,Shutdown。
- **mini 无 registry 文件**:`.smix/` 只有 `kv / runner / store.lock`,无 `sims.json` →
  alias 形 device ref 在 mini 上解析不了。**raw UDID 不需要 registry**:`resolve_device`
  (`main.rs:1069-1071`)对 `is_udid` 直接大写透传,registry 缺席合法。→ e2e 的
  nodes.yaml `devices` 用**运行期解析出的 raw UDID**(仍是 C1 轴 C 的 verbatim 透传形)。
- **stdout 纯净性(合并回路的命脉)实测**:kv AOF 噪音行(`kevy: AOF ... replayed`)走
  **stderr**(mini 上 `smix sim list 2>/dev/null` stdout 只剩表格);run 的进度行
  `STEP i/N` = `eprintln!`(`entry.rs:365`);`--format json` 的每 flow 一行 JSON =
  `println!`(`emit_json_success`/`emit_json_failure`,`entry.rs:661-689`)。
  → `--format json` 下 stdout 只应有 JSON 行,任何非 JSON 行 = 协议违约,解析器报错不吞。
- **JSON 行真实形**(`build_summary_json`,`entry.rs:636-659`):成功 =
  `{"flow":"<path>","runOutcome":"success","warnings":[...],"steps":[...]}`;失败(Sdk)=
  `{"flow":...,"runOutcome":"failure","failure":{code,message,selector,suggestions,visibleCount}}`;
  失败(其它)= `{"flow":...,"runOutcome":"failure","error":"...","steps":[...]}`。
- **runner 生命周期范式**(`scripts/release/stress-gate.sh:88-93`):`smix sim boot <UDID>`
  (`|| true`,已 boot 幂等)→ `smix runner up <UDID> --bundle <bundle>`(**阻塞到就绪
  才返回**,不需 `&` 不需 health 轮询)→ 跑 flow → cleanup `runner down --device <UDID>`。
  单 sim 未注册 → runner 端口走默认(`main.rs:1890-1896` 注释:22087)。
- **e2e 黄金 flow**:`scripts/release/stress-corpus/launch-and-capture.yaml`(launchApp +
  takeScreenshot)与 `screenshot-twice.yaml`(launchApp + 2 张截图),appId =
  `com.apple.Preferences`,**零语义断言**(不依赖账户/locale 状态),v2.8-C5 在 mini 上
  20/20 GREEN 验证过。截图 PNG 落远端 cwd(mini repo 根)—— 回收归 C4,teardown 清掉。
- **rsync 惯例形**(memory `build_hosts_mini_lx64`,唯一现成命令形):
  `rsync -a --stats --exclude='target/' --exclude='.git/' --exclude='node_modules/'
  --exclude='.smix/' --exclude='swift-bridge/.build/' --exclude='*/build/'
  --exclude='.scratch/' ./ mini:workspace/goliajp/smix/`。`-a` 保 mtime;
  `target/` 被排除 → stamp 文件永不被同步覆盖(gate 参照物安全)。
- **`.smix/config.yaml` 两端现状**:studio 无、mini 无 → C2 拍板的 scheduler 权威同步
  今天走「确保远端也无」分支(`rsync` 排除 `.smix/` → config 须单独同步,见 S3)。
- **测试基线**:federation 9 passed / bin 全量 134 passed 0 ignored(实测)。C3 加 3 个
  device-free 单测 + 1 个 ignored → 期望 federation 12 passed 1 ignored、全量 137 passed
  1 ignored。`serde_json`/`serde_norway`/`thiserror` 均已在 smix-cli 依赖
  (`Cargo.toml:38,44,46`),零新依赖。
- **脚本范式**:`scripts/dev/v2.11-c4-android-propose-e2e.sh`(`set -euo pipefail` +
  guards + `trap cleanup EXIT` + `log`/`fail` helper + 末行 marker `C4-E2E-PASS`)。

## 步骤(线性,3 个)

### S1. report 行解析 + readiness gate argv(device-free 纯逻辑)

**红(写测试)**
- 文件:`crates/smix-cli/src/federation.rs`(tests mod 追加)
- 断言(3 个 test):
  - `parses_one_report_line_per_flow`:两行 fixture(真实 shape:一行 success 含
    `warnings`/`steps`,一行 failure 含 `failure.code`)→ 2 个 `FlowReport`,
    `flow` / `outcome` 字段逐一钉死,failure 行的 `raw["failure"]["code"]` 可取
  - `rejects_a_non_json_stdout_line`:fixture 中混入一行 `kevy: AOF ... replayed`
    噪音 → `Err(ReportError::NotJson{..})`,错误串含该行原文(协议违约不吞,fail-safe;
    行间空行跳过不算违约)
  - `readiness_argv_pins_the_gate_command`:`readiness_argv(&node)` 逐字 =
    `["-o", "BatchMode=yes", "mini", "cd '<repo>' && test -f target/.smix-fed-stamp && test -x target/release/smix && [ -z \"$(find crates -name '*.rs' -newer target/.smix-fed-stamp)\" ]"]`
    (stamp 缺失 → `test -f` 先失败 = stale,fail-safe 方向在形里钉死)
- 跑红:`cargo test -p smix-cli --bin smix federation` 非零退出(API 未实现,编译失败即红)

**绿(实现)**
- 文件:`crates/smix-cli/src/federation.rs`
- API:
  ```rust
  pub const FED_BUILD_STAMP: &str = "target/.smix-fed-stamp";
  pub struct FlowReport { pub flow: String, pub outcome: String, pub raw: serde_json::Value }
  pub enum ReportError { NotJson { line: String }, MissingField { field: &'static str, line: String } }  // thiserror
  pub fn parse_report_lines(stdout: &str) -> Result<Vec<FlowReport>, ReportError>
  pub fn readiness_argv(node: &NodeSpec) -> Vec<String>
  ```
- 关键点:①逐行 `serde_json::from_str::<Value>`,空行跳过,非 JSON 行 = `NotJson`
  (远端 stdout 是**信任边界**:C1 实测分流干净,但解析器不赌 —— 违约显式报错,不吞不猜);
  ②`flow`/`runOutcome` 缺失 = `MissingField`;`raw` 保留整行 Value(C4 合并器的输入,
  C3 不加工);③`readiness_argv` 与 `remote_argv` 同约定(不含 `"ssh"` 词、
  `BatchMode=yes`、`shell_quote(repo)`),gate 命令是**只读**探测(检测与修复分离:
  重建动作在 S3 脚本,不在 gate 里)

**重构**
- 无

### S2. `run_ssh` 执行腿 + 单节点 e2e ignored 测试(全链消费 C2 产物)

**红(写测试)**
- 文件:`crates/smix-cli/src/federation.rs`(tests mod 追加)
- 断言(1 个 `#[ignore]` test):`federation_e2e_single_node_runs_flows_on_mini` ——
  从 env 取 `SMIX_FED_E2E_NODES`(nodes.yaml 路径)与 `SMIX_FED_E2E_FLOWS`(逗号分隔、
  repo 相对的 flow 路径;两者缺失即 panic 报「由 e2e 脚本驱动」),然后走全链:
  `fs::read_to_string` + `parse_nodes` → `expand_slots`(断言 1 槽)→
  `assign_flows`(断言该槽拿到全部 flow 序号)→ `run_ssh(readiness_argv(&node))` 断言
  exit 0(gate 过 = 远端新鲜)→ `run_ssh(remote_argv(&node, &flows, &device_ref, &[]))`
  断言 exit 0 且 `!is_transport_failure` → `parse_report_lines(&out.stdout)` 断言
  行数 == flow 数、每行 `outcome == "success"`、`flow` 字段与传入路径一致
- 跑红:`cargo test -p smix-cli --bin smix federation` 非零退出(`run_ssh` 不存在,
  编译失败即红)

**绿(实现)**
- 文件:`crates/smix-cli/src/federation.rs`;`crates/smix-cli/src/main.rs:15-17`
  挂载注释 until C3 → until C5
- API:
  ```rust
  pub struct RemoteOutput { pub exit: u8, pub stdout: String, pub stderr: String }
  pub fn run_ssh(argv: &[String]) -> std::io::Result<RemoteOutput>
  ```
- 关键点:①`std::process::Command::new("ssh").args(argv).output()`,退出码
  `code().map_or(1, |c| c.clamp(0, 255) as u8)`(`run_parallel:96-105` 同形;被信号杀 =
  无 code = 1,不吞);②stdout/stderr `String::from_utf8_lossy` 各自完整捕获(C1 实测
  SSH 分流保真,федерation 靠它);③spawn 腿本身不做重试/timeout(wire-layer 纪律:
  ssh 自身故障以 255 哨兵显式浮出,聚合 max-胜,不静默);④`--ignored` 测试不进默认
  suite,device-free 世界零扰动
- 本步 device-free 绿判:`cargo test -p smix-cli --bin smix federation` 12 passed
  1 ignored(e2e 测试编译进但不跑);e2e 真跑归 S3 脚本驱动

**重构**
- 无

### S3. 单节点 e2e 脚本:同步 → gate → 驱动 → teardown → marker

**红(写测试)**
- 文件:`scripts/dev/v2.12-c3-federation-single-node-e2e.sh`(照 v2.11-c4 范式:
  `set -euo pipefail` + `log`/`fail` helper + `trap cleanup EXIT`;节点 host 可用
  `SMIX_FED_NODE_HOST` env 覆盖,默认 `mini`)
- 红判(guard 先失败一次,机器可判):
  `SMIX_FED_NODE_HOST=no-such-host.invalid scripts/dev/v2.12-c3-federation-single-node-e2e.sh`
  → 非零退出且输出含 `[c3-fed] FAIL:`(可达性 guard 真的会拦;脚本未写好前此命令
  因文件不存在同样非零 = 红)
- **脚本固定序**(每段带机器判定,任一失败 = `fail` 停):
  1. **guards**:`ssh -o ConnectTimeout=5 -o BatchMode=yes $HOST true` 可达;
     studio 与 mini 双侧 `pgrep -f 'runner.ts|smix run|supervise'` 无活动 batch
     (让位不抢占);mini 侧 `pgrep -f 'cargo build|xcodebuild'` 无用户构建;
     两条 corpus flow 文件存在
  2. **源同步**:上节 rsync 惯例命令逐字(exclude 集不缩水)
  3. **config 权威同步**(C2 拍板落地):studio `.smix/config.yaml` 存在 → `rsync` 到
     mini `.smix/`;不存在 → `ssh $HOST 'rm -f <repo>/.smix/config.yaml'`;随后断言
     两端存在性一致(scheduler 权威 = 远端与本机严格同形,switches 不得静默分叉)
  4. **重建 + stamp**:`ssh $HOST 'cd <repo> && cargo build --release -p smix-cli && touch target/.smix-fed-stamp'`
     (拍板一:cargo 裁 freshness,成功即落 stamp;失败 = 脚本停,不带病跑)
  5. **gate 独立复核**:用与 `readiness_argv` 同形的 ssh 命令跑一次,exit 0 才继续
     (检测面在 Rust 测试里还会再跑一次 —— 双跑是刻意的:脚本证「操作序收敛」,
     测试证「产品 gate 函数对真节点工作」)
  6. **设备解析 + prep**(§9#1 sim only,显式 UDID):
     `ssh $HOST 'cd <repo> && target/release/smix sim list'` 取 `sim-simx-001` 行的
     UDID(断言恰 1 行命中);`smix sim boot <UDID>`(`|| true`)+
     `smix runner up <UDID> --bundle com.apple.Preferences`(阻塞到就绪)
  7. **驱动 ignored 测试**:mktemp 写 nodes.yaml(name/host/repo + `devices: [<UDID>]`)、
     `SMIX_FED_E2E_NODES=<临时路径> SMIX_FED_E2E_FLOWS='scripts/release/stress-corpus/launch-and-capture.yaml,scripts/release/stress-corpus/screenshot-twice.yaml'
     cargo test -p smix-cli --bin smix federation_e2e -- --ignored --nocapture`,exit 0
  8. **teardown(trap)**:`runner down --device <UDID>` + `sim shutdown <UDID>`
     (用完回收纪律)+ 远端 `rm -f` 三张 flow 截图 PNG(repo 根不留渣)+ rm 本机 mktemp
  9. **marker**:全过后末行 `[c3-fed] C3-FED-E2E-PASS`

**绿(实现)**
- 默认参数真跑:`scripts/dev/v2.12-c3-federation-single-node-e2e.sh` → 末行含
  `C3-FED-E2E-PASS`、exit 0(远端真 sim 跑 2 条 flow、stdout 2 行 JSON 全 success、
  gate 收敛 —— 这就是 S2 ignored 测试的真跑绿)

**重构**
- 无

## Checkpoint C3 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— device-free:12 单测 + 1 ignored,全量 137 零红,执行腿表面在,对手零改动 ——
cargo test -p smix-cli --bin smix federation 2>&1 | grep -q 'ok. 12 passed; 0 failed; 1 ignored' \
  && cargo test -p smix-cli --bin smix 2>&1 | grep -q 'ok. 137 passed; 0 failed; 1 ignored' \
  && grep -q 'pub fn parse_report_lines' crates/smix-cli/src/federation.rs \
  && grep -q 'pub fn readiness_argv' crates/smix-cli/src/federation.rs \
  && grep -q 'pub fn run_ssh' crates/smix-cli/src/federation.rs \
  && git diff --quiet crates/smix-cli/src/parallel.rs \
  && echo FEDERATION-C3-UNIT-OK
```

```bash
# —— opt-in 单节点设备 e2e(需 mini 可达 + 无活动 batch;跑完自回收)——
scripts/dev/v2.12-c3-federation-single-node-e2e.sh
```

期望:第一块打印 `FEDERATION-C3-UNIT-OK` 各命令 exit 0;第二块 exit 0 且末行含
`C3-FED-E2E-PASS`。含义 = 解析/gate/执行腿被单测钉死、全量零回归、`parallel.rs` 一字未动;
scheduler 机对 mini 的 同步→重建→gate→真 sim 跑 flow→JSON 行回传 整条单节点回路真通。

**诚实划界**:device-free = S1 全部 + S2 的编译与 12 单测;**必须 mini** = S2 ignored
测试的真跑与 S3 脚本全程(同步/重建/gate 复核/设备 prep/flow 执行/teardown)。
**不在 C3 验收内**:多节点并发与结果合并、merged 报告 schema(C4);`--debug-output`
artifact 回收(C4);nodes.yaml 的 `workspace_root` 发现 wiring 与 CLI 表面、
`mod federation` 删 `#[cfg(test)]`(C5)。

## 与 C1/C2/冷计划假设不符处(热化期发现,如实列)

1. **C1 gate 形 (c)(`find -newer <二进制>`)与「重建」策略组合会死锁**:rsync `-a` 保
   studio 侧源 mtime(新于 mini 二进制),但 cargo 按内容指纹增量 —— 源 mtime 新而内容
   未变时 `cargo build` 不 relink,二进制 mtime 不动 → gate 永久假 stale,重建也洗不掉
   (mini 现行 3 个假 stale 文件全不在 smix bin 依赖图:napi lib / build.rs / 测试文件,
   正是这形态)。C1 只判了 (c)「现状可判」,未判它与重建的收敛性。→ C3 参照物改为
   build 成功后落的 stamp 文件(拍板一),fail-safe 方向不变。
2. **C1 轴 C 的「远端 alias 由远端 registry 解析」在 mini 现状走不通**:mini `.smix/`
   无 `sims.json`(只有 kv/runner/store.lock),alias 解析无源。raw UDID 是
   `resolve_device` 的合法免 registry 路径(`main.rs:1069-1071`)→ C3 e2e 的 devices
   用运行期解析的 raw UDID,透传语义与 C1 一致;给 mini 建 registry 不是 C3 必需,不做。
3. **任务书「C3 接 CLI/执行时删 cfg」的删 cfg 半句不成立**:C3 无 runtime caller
   (CLI 收口在 C5),删 `#[cfg(test)]` 即触 dead-code deny。cfg 保留到 C5,
   `main.rs:15-17` 注释同步改 until C5(见拍板二)。
4. 其余与假设**相符**(实测证):mini 可达 + cargo 非交互可用;stale 现状延续 C1 发现;
   stdout JSON 纯净(噪音全 stderr);corpus flow 在 mini 验证过;`.smix/config.yaml`
   两端均无(C2 权威同步今天走「确保无」分支)。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.12-c3-hot.md`。
2. `docs/v2.md` 决策日志加两行:①C3 新鲜度 gate 单路径(同步后必重建 + build-stamp,
   C1 (c) 形与重建组合死锁的发现);②执行腿落位 federation.rs 同文件 + cfg 保留到 C5 +
   raw-UDID device ref 现状。
3. 由用户/上层拍板后热化 C4(结果合并:merged 报告 schema + artifact rsync 回收 +
   双节点 e2e),见 CLAUDE.md §6。

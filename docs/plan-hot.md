# plan-hot — v2.12 到 C2:federation 调度核心(device-free 纯逻辑 + 节点清单 schema)

## 目标 checkpoint

C2:**federation 调度核心存在且被单测钉死,零设备零网络**。通过后世界变成:
`crates/smix-cli/src/federation.rs` 存在(与 `parallel.rs` 同位同工艺:纯函数 + 单测钉死),
含三块纯逻辑 —— ①节点清单 schema(`NodeSpec` + `parse_nodes`,`.smix/nodes.yaml` 的形)、
②节点×设备槽展开 + flow→槽分片(复用 `shard_flows`,round-robin over 全节点设备槽)、
③远端命令组装(`child_argv` 谱系 + SSH 包装 + **显式转发 `--format json`**,C1 S12 断段)+
255 传输哨兵分类(`aggregate_exit` 逐字复用,worst-of-nodes)。9 个 federation 单测全绿,
`parallel.rs` 对手零改动。C1 verdict(`docs/research/c1-federation-loop.md`)Top-N #1/#2 即本段,
真 ssh 执行 / rsync 分发 / artifact 回收归 C3,merged 报告归 C4,CLI 收口归 C5 —— 本段不碰。

**C2 拍板(C1 S5 config/env 继承断裂,要求单路径)**:**scheduler config 权威** ——
`.smix/config.yaml` 列入 C3 的 rsync 分发集,与 flow 文件同通道同步到远端 workspace root;
不采「per-node config 权威」。理由:switches 决定 flow 行为语义,per-node 分叉 = 同一 flow
在不同节点静默不同行为(违 §13 质量优先);rsync 已是 flow 分发推荐形,同工具零新机制。
本拍板 C2 落决策日志,C3 实施同步动作 —— C2 代码面不含任何 config IO。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
git log --oneline -3 | grep -q 'v2.12-C1 close'                        # C1 已闭合归档
grep -q '^VERDICT: OBTAINABLE' docs/research/c1-federation-loop.md     # verdict = 建造许可
grep -q 'pub fn shard_flows' crates/smix-cli/src/parallel.rs           # 复用对象在
grep -q 'pub fn aggregate_exit' crates/smix-cli/src/parallel.rs        # 复用对象在
grep -q 'pub fn child_argv' crates/smix-cli/src/parallel.rs            # 复用对象在
! test -f crates/smix-cli/src/federation.rs                            # 调度核心净新
grep -q 'serde_norway.workspace = true' crates/smix-cli/Cargo.toml     # yaml 解析依赖已在
grep -q 'thiserror.workspace = true' crates/smix-cli/Cargo.toml        # 错误派生依赖已在
cargo test -p smix-cli --bin smix parallel 2>&1 | grep -q '8 passed'   # 对手 8 单测基线绿
```

全部 exit 0 = 可开工。任一失败 → 按 §6「何时该拒绝热化」回报,不硬开。

## 已经查清、不必重查的事实(热化期已探测,file:line 为证)

- **复用对象真实形**(`crates/smix-cli/src/parallel.rs`,全 pub,8 单测):
  `shard_flows(flow_count, device_count) -> Vec<Vec<usize>>` round-robin 纯函数(`:23-32`,
  flow i → 槽 i%N,device_count=0 返回空不 panic);`aggregate_exit(&[u8]) -> u8` = max,
  空集 0(`:46-48`);`child_argv(shard_flows, udid, passthrough) -> Vec<String>` =
  `run <flows> --device <udid> + passthrough`(`:59-66`)。`udid` 参数就是裸字符串 ——
  federation 传远端 alias 原样透传即成(C1 S6 形推广),不动签名不碰本机 resolve。
- **yaml 库 = `serde_norway` 0.9**(workspace 统一,`Cargo.toml:48`;smix-cli 已依赖,
  `crates/smix-cli/Cargo.toml:38`,serde/thiserror 也在)。`.smix/config.yaml` 现行读法 =
  `fs::read_to_string(root.join(".smix/config.yaml"))` + `serde_norway::from_str`
  (`runner.rs:124-126`),root 经 `workspace_root` 上溯发现 `.smix/` 目录(`runner.rs:221`)——
  **不走 smix-store kv**。`.smix/nodes.yaml` 沿同范式(文件落 `.smix/` 下、C3+ 消费时用
  `workspace_root` 发现);C2 的 `parse_nodes` 只收 `&str`,零文件 IO,单测无盘。
- **nodes 清单落 smix-cli(federation.rs 内),不落 smix-simctl registry 旁**。理由:
  ①它是 scheduler 侧调度输入,唯一消费方 = federation 调度核心(C1 轴 C 形 (a):清单只活在
  scheduler 侧,设备 ref 由**远端**registry 解析);②registry 职责 = 本机确定性设备寻址
  (`registry.rs:1-11` 头注不变量),塞入跨机清单 = 职责污染,且 C1 已钉「`RegisteredSim`
  零新字段、resolve 语义一字不动」;③steel-cement-stone:调度纯逻辑 = 钢筋,与 `parallel.rs`
  同 crate 同位同工艺(§9#8 三层:调度是决策层编排,不碰 sense/act core)。
- **smix 退出码空间 {0,1,2,3,4,5,6,130,143} 与 ssh 故障 255 不相交**(C1 轴 A/D 已证,
  `entry.rs:691-698` + `main.rs:2003-2011`)→ 255 = 传输故障哨兵,天然 max-胜,fail-safe。
- **单机 `--parallel` passthrough 不转发 `--format`**(C1 S12,`main.rs:1808-1851` 无 format
  项)—— federation 合并回路依赖远端 `--format json`,远端命令组装必须显式附加。
- **测试基线**:`cargo test -p smix-cli --bin smix` 现 125 passed(8 parallel + 117 其它,
  2026-07-24 实测)。C2 加 9 个 federation 测 → 期望 134。

## 步骤(线性,3 个)

### S1. 节点清单 schema:`NodeSpec` + `parse_nodes`

**红(写测试)**
- 文件:`crates/smix-cli/src/federation.rs`(`#[cfg(test)] mod tests` 与实现同文件,
  `parallel.rs` 同形);`crates/smix-cli/src/main.rs` 加一行 `mod federation;`
- 断言(3 个 test):
  - `parses_the_documented_nodes_yaml_shape`:文档形 yaml(2 节点,含
    `name/host/repo/devices`,如 `- name: mini / host: mini / repo: /Users/doracawl/workspace/goliajp/smix / devices: [sim-smix-001]`)
    解析出 2 个 `NodeSpec`,四字段逐一钉死
  - `rejects_a_node_without_devices`:某节点 `devices: []` → `Err(NodesError::EmptyDevices)`,
    错误串含节点名
  - `rejects_duplicate_node_names`:两节点同 `name` → `Err(NodesError::DuplicateName)`
- 跑红:`cargo test -p smix-cli --bin smix federation` 非零退出(API 未实现,编译失败即红)

**绿(实现)**
- 文件:`crates/smix-cli/src/federation.rs`
- API:
  ```rust
  pub struct NodeSpec { pub name: String, pub host: String, pub repo: String, pub devices: Vec<String> }
  pub enum NodesError { Malformed{..}, Empty, EmptyDevices{node}, DuplicateName{name} }  // thiserror
  pub fn parse_nodes(yaml: &str) -> Result<Vec<NodeSpec>, NodesError>
  ```
- 关键点:①顶层 `nodes:` 列表,serde derive + `serde_norway::from_str`;②校验(非空清单 /
  每节点 devices 非空 / name 唯一)是**信任边界契约**(用户手写 yaml = 外部输入),非防御码;
  ③`devices` 条目 = 远端 registry 的 alias/UDID 字符串,scheduler 侧**不解析不校验存在性**
  (远端自己的 registry 是唯一 mapping source,C1 轴 C);④文件头注写明 `.smix/nodes.yaml`
  落点 + §9#1(清单只含 sim/emulator)+「本机永不 resolve 远端 ref」不变量

**重构**
- 无

### S2. 节点×设备槽展开 + flow→槽分片(推广 `shard_flows`)

**红(写测试)**
- 文件:`crates/smix-cli/src/federation.rs`(同文件 tests 追加)
- 断言(3 个 test):
  - `slots_flatten_nodes_in_listing_order`:节点 A(2 设备)+ 节点 B(1 设备)→
    `[(0,"a1"),(0,"a2"),(1,"b1")]`,清单序确定性展平
  - `flows_round_robin_over_all_slots_across_nodes`:5 flow × 上述 3 槽 →
    槽 0 得 [0,3]、槽 1 得 [1,4]、槽 2 得 [2](与 `shard_flows(5,3)` 逐字一致 = 复用证明)
  - `single_node_single_device_degenerates_to_the_sequential_order`:1 节点 1 设备 ×
    3 flow → 单槽 [0,1,2](单机顺序路径退化不变式,对手 `one_sim_keeps_the_sequential_order`
    的跨机镜像)
- 跑红:`cargo test -p smix-cli --bin smix federation` 非零退出

**绿(实现)**
- 文件:`crates/smix-cli/src/federation.rs`
- API:
  ```rust
  pub fn expand_slots(nodes: &[NodeSpec]) -> Vec<(usize, String)>          // (node_idx, device_ref)
  pub struct SlotAssignment { pub node: usize, pub device_ref: String, pub flows: Vec<usize> }
  pub fn assign_flows(flow_count: usize, slots: &[(usize, String)]) -> Vec<SlotAssignment>
  ```
- 关键点:①`assign_flows` 内部调 `crate::parallel::shard_flows(flow_count, slots.len())`
  再 zip 槽 —— **复用不重写**,round-robin 语义单点维护;②空槽集返回空(继承
  `shard_flows(_, 0)` 不 panic 语义);③flow→(节点,设备) 是「flow→槽→槽属节点」二段纯映射,
  无任何 IO

**重构**
- 无

### S3. 远端命令组装(`child_argv` 跨机版)+ 255 传输哨兵聚合

**红(写测试)**
- 文件:`crates/smix-cli/src/federation.rs`(同文件 tests 追加)
- 断言(3 个 test):
  - `remote_argv_wraps_child_argv_in_ssh_with_explicit_json_format`:
    `remote_argv(&node, &["a.yaml"], "sim-smix-001", &["--no-launch"])` →
    `["-o", "BatchMode=yes", "mini", "cd <repo-quoted> && target/release/smix run 'a.yaml' --device 'sim-smix-001' --no-launch --format json"]`
    形逐字钉死;远端串含 `--format json`(S12 断段补上);不含 `--parallel`/`--also-device`
  - `shell_quoting_survives_spaces_and_single_quotes`:`shell_quote` 对含空格 / 单引号 /
    空串的输入产 POSIX 安全单引号形(`'\''` escape),裸安全串也稳定加引号(确定性)
  - `transport_failure_255_wins_the_aggregate`:`is_transport_failure(255)` true、对 smix
    码空间 {0,1,2,3,4,5,6,130,143} 全 false;`parallel::aggregate_exit(&[0, 255, 2]) == 255`
    (哨兵 max-胜,worst-of-nodes = 对手函数逐字复用的证明)
- 跑红:`cargo test -p smix-cli --bin smix federation` 非零退出

**绿(实现)**
- 文件:`crates/smix-cli/src/federation.rs`
- API:
  ```rust
  pub const SSH_TRANSPORT_EXIT: u8 = 255;
  pub fn is_transport_failure(code: u8) -> bool
  pub fn shell_quote(s: &str) -> String
  pub fn remote_argv(node: &NodeSpec, flows: &[String], device_ref: &str, passthrough: &[String]) -> Vec<String>
  ```
- 关键点:①返回值 = `ssh` 的参数列表(不含 `"ssh"` 词,与 `child_argv` 不含 exe 同约定;
  `Command::new("ssh")` 归 C3 执行腿);②远端命令串 = `cd <repo> && target/release/smix ` +
  `child_argv(flows, device_ref, passthrough)` 每 token `shell_quote` 后空格 join,再追加
  `--format json` —— **复用 `child_argv` 组 smix 侧 argv,federation 只包 SSH 皮**;
  ③`--format json` 无条件附加(合并回路依赖,非可选;对手 passthrough 恒不含 format,
  C1 S12 已证,无重复风险);④device_ref 原样进 `--device`(远端 alias 透传,本机零解析);
  ⑤`BatchMode=yes` 进 argv(C1 轴 A 实测通道形);⑥聚合不新写函数 —— worst-of-nodes 就是
  `parallel::aggregate_exit`,federation 只补哨兵分类语义

**重构**
- 若三步后 `federation.rs` 内 schema/分片/argv 三块边界模糊,按 `parallel.rs` 的
  doc-comment 工艺补齐模块头注(不改行为,测试保持绿)

## Checkpoint C2 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— 9 个 federation 单测全绿,bin 全量 134(125 基线 + 9)零红 ——
cargo test -p smix-cli --bin smix federation 2>&1 | grep -q 'test result: ok. 9 passed' \
  && cargo test -p smix-cli --bin smix 2>&1 | grep -q 'test result: ok. 134 passed; 0 failed' \
  && echo FEDERATION-TESTS-OK
# —— 调度核心真在 + 复用对手而非重写 + 对手零改动 ——
grep -q 'pub fn parse_nodes' crates/smix-cli/src/federation.rs \
  && grep -q 'pub fn assign_flows' crates/smix-cli/src/federation.rs \
  && grep -q 'pub fn remote_argv' crates/smix-cli/src/federation.rs \
  && grep -q 'parallel::shard_flows' crates/smix-cli/src/federation.rs \
  && grep -q 'parallel::aggregate_exit' crates/smix-cli/src/federation.rs \
  && git diff --quiet crates/smix-cli/src/parallel.rs \
  && echo FEDERATION-SURFACE-OK
```

期望:两行 `FEDERATION-TESTS-OK` + `FEDERATION-SURFACE-OK` 均打印,各命令 exit 0。含义 =
调度核心纯逻辑 + nodes schema 被 9 单测钉死、全量零回归、`shard_flows`/`aggregate_exit`
是复用(grep 到调用点)而非重写、对手 `parallel.rs` 一字未动。

**不在 C2 验收内(诚实划界)**:真 ssh 执行 / spawn(C3);rsync flow 分发 + config 同步 +
artifact 回收(C3/C4);nodes.yaml 的文件 IO 与 `workspace_root` 发现 wiring(C3+ 消费时接);
merged 报告 schema + 合并器(C4);CLI 表面与 `--format json` 在 federation 入口的转发
wiring(C5)。C2 只交纯逻辑 + schema。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.12-c2-hot.md`。
2. `docs/v2.md` 决策日志加两行:①C2 调度核心落位(`smix-cli/src/federation.rs` 钢筋纯逻辑,
   nodes 清单 schema 同文件,`.smix/nodes.yaml` 沿 config.yaml 直读范式不走 store);
   ②config 继承拍板(scheduler config 权威,`.smix/config.yaml` 入 C3 rsync 分发集)。
3. 由用户/上层拍板后热化 C3(单远程节点执行腿:ssh spawn + 准备度 gate + rsync 分发),
   见 CLAUDE.md §6;C3 起需真设备(本机 + mini sim),入口验证含 mini SSH 可达。

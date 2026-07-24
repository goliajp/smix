# plan-hot — v2.12 到 C1:federation 回路可得性研究(scheduler→N 节点→合并报告)

## 目标 checkpoint

C1:**read-only 研究先行**(decomposition-before-attack)。回答 v2.12 的机制不确定性 ——
**「中央 scheduler 把 flow 分片到 N 台机器的 smix(各跑自己的 sim/emulator),结果合并为单一
CI 报告 + 聚合退出码 —— 这条跨机回路无需臆造哪些新机制即可得,诚实的形是什么?」** 通过后世界
变成:`docs/research/c1-federation-loop.md` 存在,含**先于证据钉死**的四轴证伪 rubric
(节点契约 / 调度形 / 跨机设备清单 / 结果合并)+ file:line 级证据 +
`VERDICT: OBTAINABLE|NOT-OBTAINABLE|PARTIAL`。verdict 出后由用户/上层据其热化 C2(建造)或 re-tier。

**为什么是研究而非直接实现**:强 prior 存在 ——「SSH 扇出 + 推广 `--parallel` 编排 + max-exit
聚合」形状看似清楚(单机分片/聚合已被 `parallel.rs` 钉死,v2.8-C4 双 sim e2e 已在 mini 实跑,
SSH/rsync 是既有跨机范式)。但 prior ≠ 已证回路:全 workspace **零** federation/scheduler 代码
(rg 仅命中注释里的 OS scheduler jitter);四个必需机制**无一存在** —— ①节点契约(远端 smix 驱动
+ 准备度 gate;v2.8-C4 实证 stale-binary EXIT=2 陷阱)、②跨机设备清单(registry 严格 per-machine,
resolution 只读本机文件的确定性寻址不变量不可偷改)、③flow 分发(flow 在 scheduler 机盘上)、
④报告/artifact 回传与 merged schema(不存在)。scheduler 形(CLI 编排 / 常驻 daemon / 脚本层
扇出)选错要废多 commit —— 正是 decomposition-before-attack 场景。**全程 read-only**:读源码 /
读 `parallel.rs` 对手 / 对 mini 只做非设备只读探测,**不写任何 scheduler/合并实现代码**。

## 前置条件

```bash
cd /Users/doracawl/workspace/goliajp/smix
git log --oneline | grep -q 'v2.11-C5 close'                          # v2.11 阶段闭合
grep -q 'pub fn shard_flows' crates/smix-cli/src/parallel.rs           # decomp 对手:单机分片
grep -q 'pub fn aggregate_exit' crates/smix-cli/src/parallel.rs        # decomp 对手:聚合语义
grep -q 'run-summary.json' crates/smix-adapter-maestro/src/entry.rs    # 合并对象:run report 面
grep -q 'pub struct RegisteredSim' crates/smix-simctl/src/registry.rs  # per-machine registry 面
! test -f docs/research/c1-federation-loop.md                          # 研究文档净新
ssh -o ConnectTimeout=5 -o BatchMode=yes mini \
  'test -x ~/workspace/goliajp/smix/target/release/smix'               # 第二节点候选在
```

全部 exit 0 = 可开工。任一失败 → 按 §6「何时该拒绝热化」回报,不硬开。

## 已经查清、不必重查的事实(planning 期已探测,C1 直接引用为证据起点)

- **单机编排对手(`smix-cli/src/parallel.rs`,C1 逐段对照)**:`shard_flows` round-robin 纯函数
  (flow i → sim i%M,被 8 个单测钉死)/ `effective_sim_count` clamp / `aggregate_exit` = shard
  max(失败不吞)/ `child_argv` = 子进程 `smix run <flows> --device <UDID> + passthrough`,
  `--parallel`/`--also-device` 不递归。call site `main.rs:1781-1854`:devices = `--device` +
  `--also-device` 链,CLI 源 flag 显式转发,config/env 源 switches 靠子进程同读继承。
  **federation 的跨机版要回答:哪些继承假设在「不同机器的 config/env」下断裂**。
- **合并对象真实形**:`--format json` 每 flow 一行 JSON(`entry.rs:build_summary_json`:flow /
  runOutcome∈{success,failure} / warnings|error / steps`StepDebugRecord[]`);`--debug-output`
  产 `run-summary.json` + per-step JSON + 失败 tree/PNG(在**跑 flow 那台机器**的盘上);退出码
  parse=2 / sdk=3 / unknown=4 / cycle·io=5(`run_error_to_exit`),批 = max。
- **registry 语义(`smix-simctl/src/registry.rs` 头注)**:记录在 `.smix/` smix-store;resolution
  **只读 registry 文件、从不查活 sim 集**(确定性寻址);`RegisteredSim{deviceName,udid,runtime,
  deviceType,locale,runnerPort}`,runnerPort 支撑同机多 runner 并行。**严格 per-machine,无任何
  跨机字段**。
- **mini 现状(2026-07-24 只读探测)**:SSH BatchMode 通;checkout 同路径;`target/release/smix`
  在;`.smix/`(kv/runner/store.lock)在;v2.8-C4 `--parallel 2` 双 sim e2e 曾在其上 EXIT=0,
  且同轮实证 stale-binary(rsync 不完整→EXIT=2)与 SSH 抖动(连上几秒断)两个真风险。
- **零 federation 基建**:rg `federat|schedul|coordinat` 全 workspace 无真命中(仅注释 jitter、
  coord=坐标)。

## 步骤(线性,1 个;研究 checkpoint 的红/绿 = rubric 先于证据)

### S1. read-only decomposition:钉死四轴 rubric → 填证据 → 落 VERDICT

**红(rubric 先于证据)**
- 文件:`docs/research/c1-federation-loop.md`
- 断言:先写下**四轴证伪 rubric**,每轴 `OBTAINABLE`/`NOT-OBTAINABLE` 充分证据条件**此刻钉死**,
  `Evidence:` 槽**留空**,**尚无 `VERDICT` 行**(证明 rubric 非事后合理化,同 c7-zorder 范式):
  - **轴 A(节点契约)**:`OBTAINABLE-A` iff 一台远程节点经现成通道(SSH)可被驱动跑
    `smix run`,且退出码 + `--format json` stdout **无损回传** scheduler 机(只读探测实证:
    `ssh mini 'exit 42'` 回 42、远端 `smix --version`/`--help` stdout 完整捕获),且节点准备度
    (二进制/源新鲜度)存在**可机器判定**的显式 gate 形。`NOT-A` iff 通道有损或准备度只能隐式假设。
  - **轴 B(调度形)**:穷尽枚举 scheduler 候选 —— ①CLI 编排(`--parallel` 推广:child 从本机子
    进程换 SSH 远程进程)②常驻 daemon ③脚本层扇出(smix 不管跨机)。逐候选按「复用现成编排面 /
    新造面数量 / 与 §9#8 三层归位」判 `OBTAINABLE|PARTIAL|NOT`,并回答 flow 分发的诚实形
    (rsync / scp / stdin 候选枚举)。(no-ceiling-words:负向结论须附枚举依据。)
  - **轴 C(跨机设备清单)**:`OBTAINABLE-C` iff「节点×设备」清单有不破坏 per-machine registry
    确定性寻址语义的结构形(候选枚举:scheduler 侧节点清单引用远端 alias / 中央清单镜像 / 每
    节点自决)。`NOT-C` iff 任何形都要改 resolution 语义。§9#1:清单里只有 sim/emulator。
  - **轴 D(结果合并)**:`OBTAINABLE-D` iff N 份现成 run report(`--format json` 行 +
    `run-summary.json`)可无损合并为单一 CI 报告 + worst-of-nodes 退出码,且远端 artifact
    (debug-output bundle)有现成回收通道(rsync/scp)。`NOT-D` iff 合并必须改各节点报告 schema。
  - **Overall VERDICT 判定**:`OBTAINABLE`(四轴皆可 + 一条端到端回路 —— scheduler 机→远端节点
    跑 1 flow→报告+exit 回传合并 —— 各段均有现成机制或明确新造面清单)| `PARTIAL`(部分轴受限,
    附受限枚举)| `NOT-OBTAINABLE`(穷尽枚举后无可得回路)。
- 跑红(须先失败一次,证明 verdict 尚未产):
  ```bash
  test -f docs/research/c1-federation-loop.md \
    && ! grep -qE '^VERDICT: (OBTAINABLE|NOT-OBTAINABLE|PARTIAL)' docs/research/c1-federation-loop.md \
    && echo RUBRIC-FIRST-OK
  ```
  期望:打印 `RUBRIC-FIRST-OK`(rubric 在、verdict 未落 = 红)。

**绿(read-only 填证据 + 落 verdict)**
- 派 **read-only decomposition sub-agent**(Read + Bash + Grep,**无 edit 实现代码权限**;可读
  `parallel.rs`/`entry.rs`/`registry.rs`/`main.rs` call site,可对 mini 做**非设备只读探测**:
  `smix --version`、退出码回传 `ssh mini 'exit 42'`、stdout 捕获、`.smix` store 只读查看 ——
  **不跑 flow、不 boot/shutdown 任何 sim、不写远端盘**)。产出:回填每轴 `Evidence:`(file:line
  / 探测输出级,claim 有出处不脑补)+ 落 `VERDICT:` 行 + Top-N「若 OBTAINABLE 下一步建造的
  attack 候选」(调度核心落哪 crate、节点契约/合并 schema 形,给 C2 起点,不实施)。
- 关键点:①「对手」= `parallel.rs` 单机编排,跨机回路逐段 side-by-side 拆(分片 / 子进程→远程
  进程 / passthrough 继承 / 报告回传 / 聚合);②轴 B/C 负向须穷尽枚举候选,不许「结构性拿不到」
  hand-wave;③verdict 诚实 —— PARTIAL/NOT 都是合法答案,不为「像能做」凑 OBTAINABLE;
  ④§9#1 全程 sim/emulator,真机路径不评;⑤registry 确定性寻址语义按不变量对待,不提出偷改案。
- 跑绿:下方 Checkpoint 验收全绿。

**重构**
- 无(研究文档,无代码结构)。

## Checkpoint C1 验收

```bash
cd /Users/doracawl/workspace/goliajp/smix
# —— 研究文档存在 + 四轴 + verdict ——
test -f docs/research/c1-federation-loop.md \
  && grep -qE '^VERDICT: (OBTAINABLE|NOT-OBTAINABLE|PARTIAL)' docs/research/c1-federation-loop.md \
  && grep -qi '轴 A\|节点契约' docs/research/c1-federation-loop.md \
  && grep -qi '轴 B\|调度形' docs/research/c1-federation-loop.md \
  && grep -qi '轴 C\|设备清单' docs/research/c1-federation-loop.md \
  && grep -qi '轴 D\|结果合并' docs/research/c1-federation-loop.md \
  && echo DOC-VERDICT-OK
# —— 关键 claim 有本机证据支撑(文档引的对手/合并面/registry 面真实存在)——
grep -q 'pub fn shard_flows' crates/smix-cli/src/parallel.rs \
  && grep -q 'pub fn aggregate_exit' crates/smix-cli/src/parallel.rs \
  && grep -q 'run-summary.json' crates/smix-adapter-maestro/src/entry.rs \
  && grep -q 'pub struct RegisteredSim' crates/smix-simctl/src/registry.rs \
  && echo EVIDENCE-ANCHORS-OK
```

期望:两行 `DOC-VERDICT-OK` + `EVIDENCE-ANCHORS-OK` 均打印,各命令 exit 0。含义 = 研究文档
存在、含四轴 rubric + 明确 `VERDICT`,且文档所依赖的编排对手(shard_flows/aggregate_exit)、
合并对象(run-summary.json)、registry 面在本机真实存在(claim 非脑补)。

**不在 C1 验收内(诚实划界)**:任何 scheduler / 节点契约 / 合并 schema 实现代码(verdict =
OBTAINABLE 后归 C2+);跨机真跑 flow 的 e2e(属 C3+,C1 对 mini 只做非设备只读探测);CLI 表面
(属 C5)。C1 只交 verdict 文档。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.12-c1-hot.md`。
2. C1 verdict 写入 `docs/v2.md` 决策日志一行(回路可得性结论 + scheduler 形拍板依据 + 若
   re-tier 的概要调整)。
3. **由用户/上层据 verdict 拍板**:`OBTAINABLE/PARTIAL` → 调 sub-agent 热化 C2(调度核心
   device-free 纯逻辑),见 CLAUDE.md §6;`NOT-OBTAINABLE` → 据 verdict re-tier v2.12 概要
   列表,进决策日志,不硬凑。
4. **v2.12 是折入阶段最后一个 minor**:全 C 过 = v2.8–v2.12 折入阶段全完成,v2.0.0 ship 决策
   交还用户;发布顺延待授权,不自作主张 publish。

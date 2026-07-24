# C1 调研 — 跨机 federation 回路可得性 + 诚实形

> 研究先行 checkpoint(v2.12-C1,`.claude/rule/decomposition-discipline.md` =
> decomposition-before-attack)。回答:
> **「N 台机器各跑自己的 sim/emulator,由一个 scheduler 协调、结果合并进 CI ——
> 这条跨机回路用现有 smix 地基可得吗,诚实的形是什么?」**
>
> 全程 read-only:读源码 / 读单机 `--parallel` 已证编排 / 对 mini 只做非设备只读探测
> (版本 / 退出码回传 / stdout 捕获 / `.smix` store 只读查看 —— 不跑 flow、不 boot/shutdown
> 任何 sim、不写远端盘)。不 edit 实现代码、不 commit。**§9#1:每节点只跑 sim/emulator,
> 真机路径不评。registry 确定性寻址语义按不变量对待,不提偷改案。**
>
> decomposition「对手」= 单机 `--parallel` 编排(`crates/smix-cli/src/parallel.rs` 的
> `shard_flows`/`effective_sim_count`/`aggregate_exit`/`child_argv`/`run_parallel` +
> call site `main.rs:1781-1856` 子进程扇出)。federation = 这条编排的跨机推广;
> 逐段 side-by-side 拆(分片→驱动→执行→收集),明确哪段直接推广、哪段跨机会断。

## Falsification rubric

**先于收证据钉死**。每条判据的 `Evidence:` 槽此刻留空,证据在 `## Evidence` 段回填,
证明 verdict 非事后合理化(同 v2.11-C1 / c7-zorder 范式)。

### 轴 A(节点契约)

- **判 `OBTAINABLE-A` 的充分证据**:一台远程节点经现成通道(SSH)可被驱动跑
  `smix run`,满足全部三条:
  1. **退出码无损回传**:`ssh <node> 'exit N'` 在 scheduler 机拿回 N(只读探测实证,
     含非零值,如 42);
  2. **stdout 无损捕获**:远端 `smix --version` / `smix --help` 的 stdout 在 scheduler
     机完整捕获(`--format json` 每 flow 一行 JSON 依赖 stdout 管道完整性);
  3. **节点准备度存在可机器判定的显式 gate 形**:「二进制在 + 二进制/源新鲜度」可用
     单条(或 2-3 条)只读命令判定(exit 0/非 0),不靠隐式假设远端是新的
     (v2.8-C4 实证 stale-binary EXIT=2 陷阱)。gate 形候选须列出并判定至少一形可得。
  - Evidence: __(空)__
- **判 `NOT-OBTAINABLE-A` 的充分证据**:通道有损(退出码被折叠 / stdout 截断混流),
  或准备度只能隐式假设(无任何机器可判形)。
  - Evidence: __(空)__

### 轴 B(调度形)—— 穷尽枚举 scheduler 候选

候选**穷尽枚举**(no-ceiling-words:负向结论须附枚举依据,不许「结构性拿不到」hand-wave):

- **① CLI 编排推广**:推广 `--parallel` 编排 —— child 从本机 `std::process::Command`
  子进程换成 SSH 远程进程,分片/聚合纯函数推广到「节点×设备」。
- **② 常驻 scheduler daemon**:长驻进程管理节点池 / 队列 / 心跳。
- **③ 脚本层扇出**:smix 不管跨机,用户自己写 shell/CI 矩阵扇出 + 合并。

逐候选按三判据判 `OBTAINABLE|PARTIAL|NOT`:
1. **复用现成编排面**:能否复用 `shard_flows`/`aggregate_exit`/`child_argv` 谱系
   (file:line 指认复用点);
2. **新造面数量**:需净新的机制(传输 / 契约 / 清单 / 合并)各是什么,是否有项目内
   现成范式(SSH/rsync devops 惯例、v2.8-C4 已实跑);
3. **§9#8 三层归位 + steel-cement-stone 归类**:调度核心落哪一类(石头/钢筋/水泥)、
   哪个 crate,是否破三层架构。

另须回答 **flow 分发的诚实形**:flow 文件在 scheduler 机盘上,分发候选穷尽枚举
(rsync / scp / stdin)逐个判,给出推荐 + 依据。

- **判轴 B `OBTAINABLE`**:≥1 候选三判据全过且有明确推荐形 + 落点 crate。
- **判轴 B `NOT-OBTAINABLE`**:穷尽枚举后所有候选都要臆造无现成范式的新机制。
- Evidence(①): __(空)__
- Evidence(②): __(空)__
- Evidence(③): __(空)__
- Evidence(flow 分发): __(空)__

### 轴 C(跨机设备清单)

- **判 `OBTAINABLE-C` 的充分证据**:「节点×设备」清单存在**不破坏 per-machine registry
  确定性寻址语义**(resolution 只读本机 registry 文件、从不查活 sim 集,
  `registry.rs:1-11` 头注不变量)的结构形。候选穷尽枚举逐个判:
  - (a) scheduler 侧节点清单,设备引用**远端 alias**(远端节点自己的 registry 解析);
  - (b) 中央清单镜像(scheduler 机持有全节点设备快照);
  - (c) 每节点自决(scheduler 只指定节点,不指定设备,节点侧自选);
  至少一形满足:不改 `resolve` 语义、不加跨机字段进 `RegisteredSim`、清单里只有
  sim/emulator(§9#1)。
  - Evidence: __(空)__
- **判 `NOT-OBTAINABLE-C` 的充分证据**:枚举后任何形都必须改 resolution 语义
  (让本机 resolve 查远端 / 查活 sim 集)才能工作。
  - Evidence: __(空)__

### 轴 D(结果合并)

- **判 `OBTAINABLE-D` 的充分证据**,全部四条:
  1. N 份现成 run report(`--format json` 每 flow 一行 stdout JSON +
     `run-summary.json`)**无需改各节点报告 schema** 即可合并为单一 CI 报告
     (merged schema = 包裹层,不动叶子);
  2. 退出码聚合有现成语义可推广(`aggregate_exit` = shard max → worst-of-nodes max);
  3. 远端 artifact(`--debug-output` bundle,落跑 flow 那台机器的盘)有现成回收通道
     (rsync/scp 候选枚举判定);
  4. stdout 报告行经 SSH 管道回传不与其他流混淆(stderr/stdout 分流可判)。
  - Evidence: __(空)__
- **判 `NOT-OBTAINABLE-D` 的充分证据**:合并必须改各节点报告 schema,或 artifact
  无任何现成回收通道,或 stdout/stderr 经 SSH 不可分流。
  - Evidence: __(空)__

### Overall VERDICT 判定

- `OBTAINABLE`:四轴皆可 + 一条端到端回路(scheduler 机→远端节点跑 1 flow→报告+exit
  回传合并)各段均有现成机制**或明确的新造面清单**(每个新造面有项目内现成范式可循)。
- `PARTIAL`:部分轴受限,附受限枚举。
- `NOT-OBTAINABLE`:穷尽枚举后无可得回路。

## Evidence

所有 file:line 对本机 crate 静态阅读,read-only。mini 探测全部非设备、只读(SSH BatchMode,
不跑 flow、不 boot/shutdown sim、不写远端盘)。全 workspace
`grep -rn 'federat|scheduler|coordinat' crates/ -i` 真命中为零(仅 `perf_gate.rs:75` /
`bench.rs:11` 注释里的 OS「scheduler jitter」与 `main.rs:1806` 注释「coordination」)——
federation 基建整体是净新建造,与 plan 假设相符。

### decomposition「对手」side-by-side:单机 `--parallel` 编排 vs 跨机 federation

对手 = `crates/smix-cli/src/parallel.rs` + call site `main.rs:1781-1856`。逐段拆,
标注**直接推广 / 形推广 / 跨机断**:

| # | 段 | 单机对手(file:line) | 跨机 federation | 判 |
|---|---|---|---|---|
| S1 | 设备集合组装 | `--device` + `--also-device` 链拼 `all_devices`(`main.rs:1781-1786`),refs 全属本机 registry | 设备分属 N 台机器的 registry;本机链拼形不带节点维度 | **断** → 轴 C(节点×设备清单) |
| S2 | 分片 | `shard_flows(flow_count, device_count)` round-robin 纯函数(`parallel.rs:23-32`,8 单测钉死) | 纯函数、device-free,「device_count」推广为「全节点设备槽总数」即用;flow→槽→(节点,设备) 二段映射是纯逻辑扩展 | **直接推广** |
| S3 | child argv | `child_argv` = `run <flows> --device <UDID> + passthrough`,`--parallel`/`--also-device` 不递归(`parallel.rs:59-66`) | argv 形原样;外面包一层 `ssh <node> 'cd <repo> && ./target/release/smix <argv>'` | **形推广**(SSH 包装是净新薄层) |
| S4 | flow 文件可达 | 子进程与父同盘,直接读同一路径(`flow_strs` = 本机路径,`main.rs:1789-1790`;flow 读入 `fs::read_to_string`,`main.rs:1741`) | flow 在 scheduler 机盘上,远端读不到 | **断** → flow 分发(轴 B 内枚举) |
| S5 | config/env 继承 | 「Config/env-sourced behaviour switches are inherited by the children from the same `.smix/config.yaml` + env」(`parallel.rs:72-74` doc);switches 解析优先级 config > env > default(`main.rs:1693-1702`,`runner.rs:145 load_switches`) | 远端读**自己的** `.smix/config.yaml` + 自己的 env —— 同读继承假设断裂,switches 可静默分叉 | **断** → 节点契约须显式覆盖(config 同步入 prep,或声明 per-node config 权威,C2 拍) |
| S6 | 设备 ref 解析 | scheduler 机 `resolve_device`(`main.rs:1058-1064`)读本机 registry(`registry_path` cwd 向上发现,`main.rs:1072-1085`) | 远端 alias **不得**在 scheduler 机解析;须原样透传给远端 `smix run --device <ref>`,远端用自己 registry 解析 | **形推广**(透传即成;确定性寻址语义不动,见轴 C) |
| S7 | runner 端口 | per-sim `runnerPort`(`registry.rs:71-83`);child 各自解析(`main.rs:1803-1804` 注释「runner_port is per-sim (skipped → each child resolves its own)」) | 远端子进程同样各自解析自己 registry 的 runnerPort | **直接推广** |
| S8 | spawn / 并发 | `std::process::Command::new(exe).spawn()` 先全 spawn 再 join(`parallel.rs:88-95`) | SSH 本身是本机进程 —— `Command::new("ssh")` 落进同一 spawn-all-then-join 形 | **形推广** |
| S9 | 退出码收集 | `c.wait().code()` clamp 0-255,spawn 失败计 1(`parallel.rs:96-105`) | SSH 无损回传远端退出码(实测 42→42);ssh 自身失败 = 255,与 smix 码空间 {0,1,2,3,4,5,6,130,143} 不相交(`entry.rs:691-698` + `main.rs:2003-2011` + 130/143 `entry.rs:544-546`)→ 传输故障机器可辨且 255 天然 max-胜 | **直接推广**(+255 哨兵语义净新一条契约行) |
| S10 | 聚合 | `aggregate_exit` = shard max,空集 0(`parallel.rs:46-48`) | worst-of-nodes = 同一纯函数逐字复用 | **直接推广** |
| S11 | 报告 / artifact | `--format json` 每 flow 一行 stdout(`entry.rs:661-689`);`--debug-output` 落**跑 flow 那台机**的盘,多 flow 时 per-flow 子目录 keyed by basename(`main.rs:1908-1928`);passthrough 转发 `--debug-output`(`main.rs:1844-1847`) | stdout 经 SSH per-node 管道回 scheduler(实测无损、与 stderr 分流);artifact 在远端盘,须回收 | **半断**:stdout 回传现成;artifact 回收 → 轴 D |
| S12 | `--format` 转发 | passthrough 清单 = bundle-id / no-launch / animations / activate / verbose / fail-fast / retry / platform / apps-config / debug-output / env(`main.rs:1808-1851`),**`--format` 不在内**(sed+grep 核实,唯二 "format" 命中是 `format!` 宏)—— 单机 shard 子进程恒 human 输出 | federation 的合并回路**依赖**远端 `--format json`,必须显式转发 | **对手自身 gap**,跨机版须补转发(1 行级) |

**跨机会断的段清单(C2+ 要补)**:S1 设备集合(→节点×设备清单)、S4 flow 分发、
S5 config/env 继承(显式化)、S11 artifact 回收、S12 `--format` 转发。
其余段(S2/S3/S6/S7/S8/S9/S10)直接或形推广,复用 `parallel.rs` 谱系。

### 轴 A(节点契约)→ `OBTAINABLE-A`

**通道保真(mini 只读实测,2026-07-24)**:

1. **退出码无损**:`ssh -o BatchMode=yes mini 'exit 42'` → scheduler 机 `$?` = **42**(实测)。
   复合探测 `'echo STDOUT-LINE; echo STDERR-LINE >&2; exit 7'` → exit **7** 同时保真。
2. **stdout 无损 + 分流**:远端 `target/release/smix --version` → `smix 2.0.0` 完整捕获;
   `--help | head -3` 三行完整;stdout/stderr 分别重定向后 `STDOUT-LINE` 只进 out 文件、
   `STDERR-LINE` 只进 err 文件 —— `--format json` 的 stdout 管道与 `eprintln` 进度行
   (`parallel.rs:87,106` / entry.rs 警告全走 stderr)天然分流,合并输入干净。
3. **节点准备度 gate 形(候选枚举 + 逐判)**:
   - (a) **版本串对比** — 本机与 mini 均报 `smix 2.0.0`(实测),版本串跨多 commit 不变,
     **判不充分**(v2.8-C4 stale-binary 陷阱正是同版本旧码)。
   - (b) **git HEAD 对比** — **mini 上不可用**(实测 exit 128):mini checkout 是 rsync 来的
     worktree 副本,`.git` 是指针文件 `gitdir: /Users/doracawl/workspace/goliajp/smix/.git/worktrees/agent-a7c296e622da6923f`,
     指向 scheduler 机路径,mini 上不存在 → `git rev-parse HEAD` fatal。判**现状不可用**
     (源同步惯例是「只搬源码」,见 memory `build_hosts_mini_lx64`)。
   - (c) **源 mtime vs 二进制 mtime** — `find crates -name '*.rs' -newer target/release/smix`
     在 mini 实测返回 **3 个文件**(`smix-error/tests/sdk_driving_parity.rs`、
     `smix-node/build.rs`、`smix-node/src/lib.rs`)—— gate 现场逮到真实轻度 stale。
     机器可判(非空 = stale,`wc -l` / `grep -q` 出退出码);rsync 保 mtime 使其跨机成立。
     保守方向(会标记不进 CLI 二进制的 .rs,如 napi lib / 测试文件)—— 误报朝 fail-safe 侧,
     诚实。判**现状可用**。
   - (d) **build-stamp 进 `--version`**(编译期嵌 git hash/时间戳)— 最强形,现状**无**
     (`--version` 仅 `smix 2.0.0`),是净新建造候选(C2+)。
   - 另有现成组件:二进制存在性 `ssh mini 'test -x .../smix'`(前置探测实测 exit 0);
     `smix doctor`(simctl 可用性探针,CLI help 实测)与 `smix sim list`/`sim resolve`
     (机器可读)可组成设备侧 ready 判。
   - ≥1 形(c)此刻机器可判 + (d) 明确净新候选 → 准备度 gate **有显式形,非隐式假设**。

→ **轴 A 判 `OBTAINABLE-A`**:三条件全满足(42/7 退出码保真、stdout 完整且与 stderr 分流、
mtime-gate 现状可判 + build-stamp 净新候选)。

### 轴 B(调度形)—— 穷尽枚举 → 推荐 ①CLI 编排推广

- **① CLI 编排推广 → OBTAINABLE(推荐)**:
  1. *复用现成编排面*:`shard_flows`(`parallel.rs:23`,纯函数,槽数抽象即推广)/
     `aggregate_exit`(`parallel.rs:46`,逐字复用)/ `child_argv`(`parallel.rs:59`,argv 形
     原样 + SSH 包装)/ spawn-all-then-join 形(`parallel.rs:76-110`,SSH 是本机进程,同形)。
  2. *新造面数量,各有项目内范式*:SSH 包装薄层(v2.8-C4 已用 SSH 驱动 mini 跑
     `--parallel 2` e2e,范式已实跑)/ flow 分发 rsync(devops 惯例 + mini 源同步现行做法)/
     节点清单(轴 C,净新 config)/ artifact 回收 rsync(轴 D)/ 准备度 gate(轴 A 已判有形)。
     每个净新面都有可循范式,无一臆造。
  3. *三层归位 + 归类*:调度核心 = device-free 纯逻辑 → **钢筋**,落
     `crates/smix-cli/src/federation.rs`(与 `parallel.rs` 同位同工艺:纯函数 + 单测钉死 +
     main.rs 薄 wiring);不触 sense/act core,§9#8 不破。
- **② 常驻 scheduler daemon → PARTIAL(不推荐,枚举依据)**:原理上可建,但逐面皆净新且
  无项目内范式 —— (i) 仓内零 daemon 基建(runner capsule 是设备侧 XCUITest 进程,非编排
  daemon);(ii) CI 消费形是批式「单命令→退出码」(`aggregate_exit` 契约),daemon 需再造
  client↔daemon wire 协议 + 生命周期管理才能回到同一契约;(iii) 无队列需求证据(节点规模
  N=2 起步,flow 批一次性下发,无排队/抢占场景)。三判据两条不过 → 不选。非「结构性做不到」,
  是「每一面都要臆造无范式的新机制」——依据已逐条列。
- **③ 脚本层扇出(smix 不管跨机)→ NOT(作为 smix 答案;枚举依据)**:用户 shell/CI 矩阵
  自扇出当然「能跑」,但 (i) roadmap 明文「Distributed run federation」是 smix 能力,甩给
  用户 = 能力缺位不补,违 §12.2 capability-gap-first;(ii) 合并 schema / 准备度 gate / 聚合
  语义会在每个消费方的水泥里重写一遍,通用能力沉不进钢筋,违 steel-cement-stone;
  (iii) §13 质量 >> 研发成本,「省事」不构成选它的理由。判 NOT。
- **flow 分发候选(穷尽:rsync / scp / stdin)**:
  - **rsync → 推荐**:目录语义 + 增量 + **保 mtime**(直接喂轴 A 的 mtime 新鲜度 gate)+
    现行 mini 源同步惯例同工具。
  - scp → 可用但弱:无增量/目录删除语义,mtime 需 `-p` 且无 delta;无独立优势。
  - stdin → NOT:`smix run` 只收文件路径(flow 经 `fs::read_to_string(flow_path)`,
    `main.rs:1741`;无任何 stdin flow 表面),走 stdin 要净新 CLI 表面,违「复用现成」判据。

→ **轴 B 判 `OBTAINABLE`**:候选 ① 三判据全过;推荐形 = **CLI 编排推广**,调度核心落
`crates/smix-cli/src/federation.rs`(纯逻辑,`parallel.rs` 同工艺);flow 分发 = rsync。

### 轴 C(跨机设备清单)→ `OBTAINABLE-C`(形 = (a) 节点清单 + 远端 alias 透传)

registry 不变量(`registry.rs:1-11` 头注):记录住 `.smix/` smix-store;「Resolution never
consults the live simulator set: the registry file is the only mapping source」。
`RegisteredSim`(`registry.rs:52-84`)字段 = deviceName/udid/runtime/deviceType/locale/
runnerPort,**无任何跨机字段** —— 与 plan 假设相符。

- **(a) scheduler 侧节点清单 + 设备引用远端 alias → OBTAINABLE(推荐)**:净新 config
  (形如 `.smix/nodes.yaml`:node → ssh host + repo 路径 + 该节点设备 ref 列表)只活在
  scheduler 侧;设备 ref **原样透传**进远端 `smix run --device <ref>`,由远端节点用
  **自己的** registry 解析(S6 形推广)。本机 `resolve_device` 不碰远端 ref,远端 resolve
  语义一字不动,`RegisteredSim` 零新字段。清单条目即各节点 registry 里的 sim/emulator
  alias(§9#1 满足)。**确定性保持**:每台机器上「同一 ref → 同一设备」仍只由该机 registry
  文件决定。
- **(b) 中央清单镜像 → PARTIAL(不推荐,枚举依据)**:scheduler 持全节点设备快照并据此
  解析出 UDID 下发 —— 引入**第二 mapping source**,镜像与节点 registry 之间有 staleness
  窗口(镜像旧 → 下发错 UDID),破坏「registry 文件是唯一 mapping source」的精神;要消
  窗口就得查活集或加同步协议(臆造)。不选。
- **(c) 每节点自决(scheduler 只点节点不点设备)→ PARTIAL(不推荐,枚举依据)**:分片
  数学需要每节点设备槽数,scheduler 侧不可知;且与产品「pinned-device model:ambiguity
  is a bug, not a feature」(CLI --help 实测文案)相悖 —— 设备选择变隐式。不选。

→ **轴 C 判 `OBTAINABLE-C`**:形 (a) 不改 resolve 语义、不加跨机字段、清单只含
sim/emulator;负向候选 (b)(c) 枚举依据已列。

### 轴 D(结果合并)→ `OBTAINABLE-D`

1. **报告合并不动叶子 schema**:每 flow 一行 stdout JSON(`build_summary_json`,
   `entry.rs:636-659`:flow / runOutcome∈{success,failure} / warnings|error /
   steps `StepDebugRecord[]`;失败另有 `emit_json_failure` 结构化
   failure{code,message,selector,suggestions,visibleCount},`entry.rs:669-689`)+
   `run-summary.json`(`write_debug_output`,`entry.rs:620-634`;多 flow per-flow 子目录
   keyed by basename,`main.rs:1917-1928`)。merged schema = 纯包裹层
   `{nodes:[{node, exit, flows:[<每行 JSON 原样>]}], aggregateExit}` —— 叶子零改。
2. **退出码聚合现成语义推广**:`aggregate_exit` shard-max(`parallel.rs:46-48`)→
   worst-of-nodes 同函数逐字复用;SSH 保真已实测(轴 A);ssh 自身故障 255 与 smix 码
   空间不相交且天然 max-胜 → 传输故障自动 fail 批,fail-safe 方向正确。
3. **artifact 回收有现成通道**:`--debug-output` bundle 落远端盘(S11);回收候选 rsync
  (增量 + 目录语义,与 flow 分发同工具同惯例,推荐)/ scp(可用,弱);scheduler 侧按
   per-node 子目录归置防 basename 撞名 —— 包裹层解决,不动 bundle 内容。
4. **stdout 回传不混流**:SSH per-node 独立管道 + stdout/stderr 分流实测(轴 A);进度/
   警告全走 stderr(`parallel.rs:87,106` eprintln 谱系)。**且跨机形反优于对手**:单机
   `--parallel` 子进程 stdout 直接继承父进程终端交错;SSH 扇出天然 per-node 捕获。
- **诚实 nuance(不下调)**:(i) 远端多 flow 批的 per-flow 退出码不在 wire 上单列
  (节点只回 max,`main.rs:1911-1912`)—— 与单机 `--parallel` 同限制,per-flow 归因靠
  JSON 行的 runOutcome,足够;(ii) S12:`--format json` 在对手 passthrough 里缺席,
  federation 必须显式转发(1 行级新造,已列断段清单)。

→ **轴 D 判 `OBTAINABLE-D`**:四条件全满足。

### 综合

- **轴 A OBTAINABLE**:SSH 通道退出码/stdout 保真实测;准备度 gate 有机器可判显式形
  (mtime-gate 现状可用 + build-stamp 净新候选;版本串/git 形已判不可用并附依据)。
- **轴 B OBTAINABLE**:三候选穷尽枚举,①CLI 编排推广三判据全过(复用 `parallel.rs`
  四件套 + 每个净新面有实跑范式 + 钢筋归位 `smix-cli/src/federation.rs`);②daemon /
  ③脚本层负向依据已逐条列。flow 分发 = rsync(stdin 无表面,scp 无优势)。
- **轴 C OBTAINABLE**:形 (a) scheduler 节点清单 + 远端 alias 透传 —— resolve 语义
  一字不动、`RegisteredSim` 零新字段、§9#1 满足;(b)(c) 负向枚举已列。
- **轴 D OBTAINABLE**:merged schema 纯包裹不动叶子;`aggregate_exit` 逐字推广;
  255 哨兵 fail-safe;artifact rsync 回收;stdout 分流实测。
- **端到端回路各段**:readiness gate(轴 A 形)→ rsync flow 分发 → `ssh <node> smix run
  --device <远端 alias> --format json --debug-output <dir>` → stdout JSON 行 + 退出码
  回传 → rsync artifact 回收 → 包裹层合并 + `aggregate_exit`。每段 = 现成机制,或已点名
  的净新面(SSH 薄包装 / nodes 清单 / gate / 转发 `--format`)且各有项目内范式。

VERDICT: OBTAINABLE — 轴 A(节点契约)OBTAINABLE(SSH 退出码 42/7 + stdout 分流实测;mtime 新鲜度 gate 现状可判,build-stamp 为最强净新候选)、轴 B(调度形)OBTAINABLE(推荐 ①CLI 编排推广,调度核心落 `crates/smix-cli/src/federation.rs` 钢筋纯逻辑;daemon/脚本层负向枚举已列;flow 分发 = rsync)、轴 C(跨机设备清单)OBTAINABLE(scheduler 侧节点清单 + 远端 alias 透传,registry 确定性寻址语义零改动)、轴 D(结果合并)OBTAINABLE(merged schema 纯包裹层,`aggregate_exit` 逐字推广 worst-of-nodes,artifact rsync 回收)。跨机断段 = S1 设备集合 / S4 flow 分发 / S5 config-env 继承 / S11 artifact 回收 / S12 `--format` 转发,全部有明确新造形 + 项目内范式,无一臆造。

## Top-N「C2+ 建造 attack 候选」(不实施,只给起点)

1. **federation 调度纯逻辑**(C2,`crates/smix-cli/src/federation.rs`,钢筋):
   节点×设备槽展开 + flow→(节点,设备) 分片(推广 `shard_flows`,round-robin over 槽)+
   worst-of-nodes 聚合(`aggregate_exit` 复用)+ 远端 argv 构造(`child_argv` 谱系 +
   SSH 包装形)。纯函数、无网络无设备、单测钉死(对手 8 单测同工艺)。
2. **节点清单 config 形**(C2 定 schema,C3 消费;scheduler 侧净新文件,候选名
   `.smix/nodes.yaml`):node → ssh host / repo 路径 / 设备 ref 列表(远端 alias,
   §9#1 只含 sim/emulator)。不进 `RegisteredSim`、不动 resolve。
3. **节点契约 gate**(C3):`test -x` 二进制在 + mtime 新鲜度(`find crates -name '*.rs'
   -newer <bin>` 非空 = stale,fail-safe)+ `smix doctor` 设备侧探针;最强形 =
   build-stamp 进 `--version`(编译期嵌 git hash,消 (a)(b) 两形的不可判/不可用),
   建议 C3 一并建。**mini 的 `.git` worktree 指针残缺意味着 git 形 gate 现状不可用** ——
   gate 不得依赖远端 git。
4. **远程执行腿**(C3):SSH spawn 包装(落进 `run_parallel` 的 spawn-all-then-join 形)+
   per-node stdout 捕获 + 255 哨兵(ssh 传输故障)归类 + **显式转发 `--format json`**
   (对手 passthrough gap,S12)+ config/env 继承断裂的显式化(config 同步入 prep 或
   声明 per-node 权威,C2 拍板单路径)。
5. **flow 分发 + artifact 回收**(C3/C4):rsync 双向(分发保 mtime 喂 gate;回收按
   per-node 子目录归置)。
6. **merged 报告 schema + 合并器**(C4):包裹层 `{nodes:[{node, exit,
   flows:[...]}], aggregateExit}`,叶子 = 现成每行 JSON + `run-summary.json` 原样;
   fixture 单测(N 份真 shape → merged)+ 双节点(本机 + mini)e2e。
7. **CLI 收口**(C5):federation 表面挂 CLI(具体形 C5 热化时定,本 verdict 只钉
   「CLI 编排推广」路线)。

## 与冷计划 / plan-hot 假设不符处(如实列)

- **mini checkout 的 git 元数据是残缺的**:plan 只记「checkout 同路径」;实测 `.git` 是
  worktree 指针文件(`gitdir: /Users/doracawl/.../.git/worktrees/agent-a7c296e622da6923f`,
  指向 scheduler 机路径),mini 上 `git rev-parse HEAD` fatal(exit 128)。含义:
  **git HEAD 类新鲜度 gate 在现行源同步惯例下不可用**,gate 须走 mtime/build-stamp 形。
  计划未预见此点。
- **mini 二进制此刻真实轻度 stale**:3 个 .rs 源文件 mtime 新于 `target/release/smix`
  (实测)。冷计划的 stale-binary 风险不是历史事件,是现在进行时 —— C3 gate 非可选。
- **版本串无 build-stamp**:本机与 mini 均报 `smix 2.0.0`,版本形不含 git hash/时间戳,
  版本对比无法充当新鲜度 gate(冷计划风险条目成立且比预想更尖:现成表面没有任何
  可判新鲜度的输出,(c)/(d) 是仅有的两形)。
- **对手 passthrough 不转发 `--format`**(`main.rs:1808-1851` 无 format 项):单机
  `--parallel` 的 shard 子进程恒 human 输出 —— plan 描述「CLI 源 flag 显式转发」成立,
  但 `--format` 恰不在转发清单;federation 合并回路依赖它,跨机版必须补转发。
- 其余(`parallel.rs` 四件套 / `run-summary.json` + per-flow 子目录 / registry per-machine
  无跨机字段 / 零 federation 基建 / mini SSH BatchMode 通 + 二进制在)与 plan 假设
  **相符**,已 file:line / 实测证。

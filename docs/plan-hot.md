# plan-hot — v2.8 到 C2：进程内 runner cycle（免 xcodebuild spawn，目标 ~500ms，decomposition-first）

## 目标 checkpoint

C2：`smix runner cycle` 在 **test-host 仍存活** 的可恢复场景下，走「进程内 bounce FlyingFox + rebind XCUIApplication」路径，不再 spawn 一个新的 `xcodebuild test-without-building`，实测 warm cycle 从 ~3s 降到 < 1s（目标 ~500ms）。若 S1 decomposition 判定 XCTest 生命周期结构上不允许在 `test_runForever` 存活期内 bounce FlyingFox，则 C2 的诚实产出是：把这一结构性阻塞写进 ground-truth doc + `docs/v2.md` 决策日志，并把 cycle re-tier（soft-cycle / hard-cycle 分层），不硬塞。**哪条走，由 S1 的 feasibility gate 机器可判地决定，不在实现期临时拍。**

## 前置条件

```bash
git status --short                                              # 期望：空
grep '^version' Cargo.toml | head -1                            # 期望：version = "2.0.0"
cargo test -p smix-cli --test list_sessions_no_nested_runtime  # 期望：exit 0（C14-pre 修复仍绿）
test -f crates/smix-cli/bench/baseline.json                    # 期望：C1 的 bench 基线已在
xcodebuild -version                                            # 记录 Xcode 版本入 doc 顶部
xcrun simctl list runtimes -j | python3 -c 'import json,sys;print([r["identifier"] for r in json.load(sys.stdin)["runtimes"] if r.get("isAvailable")])'
```

- C1（`smix bench` 回归线）必须已落地——没有能看 delta 的测量线，C2 的 perf 工作全是盲猜（冷计划已知风险明列）。
- 设备核实一律走 **mini**（memory `build_hosts_mini_lx64`），不占 studio；动设备前先查
  `pgrep -fl "runner.ts|smix run|supervise|xcodebuild"` 让位活动 batch。

## 步骤（线性，无分叉）

本段是 **PERF checkpoint**，强制走 `~/.claude-shared/global/methodology/perf-decomposition-vs-polish.md`：
S1 = Decomposition（read-only，无 edit 权，只 Read/Bash/Grep）→ ground-truth doc；
S2 = Attack（worktree 隔离，仅当 S1 verdict == feasible 才执行）。
**不允许边读边试**：一边读源码一边「我先改 runForever 试一下」= 自动失败，回 decomposition 模式。

### S1. 拆解 cycle 路径并回答 feasibility（read-only，无 edit）

**红（建立 gap 断言 + 测量 baseline）**
- 参考实现（in-process target reference）：现存的进程内 app-rebind 原语——`resolveApp()`
  （`swift-bridge/SmixRunnerUITests/SmixRunnerUITests.swift:1120`）+ `/session/relaunch-app`
  / `/session/launch-app` / `/session/open`（`:2591`）。这些**已在 `test_runForever` 存活期内 rebind XCUIApplication 并重新驱动 app，不动 `server.run()`、不结束测试**。它是「进程内 cycle 的一半」的现成 ground truth。
- 在 mini 上实测当前 `smix runner cycle`（warm）的 wall time，`median-of-3 + sample stdev`
  （Pre-Phase-A gate：single-run 可能是 noise misread）。同时用带时间戳的探针把这 ~3s 拆到
  stage 级：`down()` 的 SIGINT + 等待（`runner.rs:898`，`kill -INT` 后轮询 `pid_command` 至多 30s）
  / `xcodebuild test-without-building` 重 spawn（`runner.rs:689` + `xcodebuild_argv` `:233`）
  / FlyingFox 重 bind（`SmixRunnerServer.makeServer`）
  / `XCUIApplication.launch()` | `.activate()`（`SmixRunnerUITests.swift:918-921`）
  / `app.frame` 首次快照（`:933`，注释标 ~50-150ms）/ `/health` 首次 200。
- 断言（red）：当前 warm cycle raw time vs ~500ms 目标存在 ≥ 5× gap；bench corpus 里
  **尚无** `runner_cycle.*` 度量（`bench.rs:135` `measure_corpus()` 是纯 device-free 语料，
  不含 cycle-time）——即 gap 当前不可门控。

**绿（产出 ground-truth doc + verdict）**
- 文件（S1 产物）：`docs/perf/v2.8-c2-cycle-decomposition.md`。按 methodology §5 模板，把
  cycle 完整生命周期拆 **≥ 18 stage**，side-by-side 两列：
  「当前 xcodebuild-spawn 路径」`file:line` vs「进程内 target reference 路径」`file:line`，
  每 stage 给 atomic ops 枚举 + µs/ms 估算 + Δ + 原因 + attack 候选。
- **硬质量约束**：18 段之和 ±20% 对账 mini 实测 warm cycle 时间；任一列差 > 20% = 漏段，回去补。
- **runtime 计数验证（methodology §2 luna 教训，source-read 必要不充分）**：doc 若声称
  「FlyingFox bounce 不需要 `test_runForever` 返回」/「XCUIApplication rebind 已在进程内」，
  必须有对应实测证据——例如在 mini 上对现存 `/session/relaunch-app` 打时间戳，证明它确实在
  test 存活期内完成 app rebind 且 `server.run()` 未返回（`runForever` 的 single-shot run /
  stop / return 链是关键证据面，`SmixRunnerServer.swift:1178` 起）。
- **feasibility gate（doc 末尾单行，机器可判）**：写 `Verdict: feasible` 或 `Verdict: infeasible`。
  判据必须落到 `file:line`：
  - `feasible` ⇒ doc 必须给出「不结束 `test_runForever` 的前提下 bounce FlyingFox」的具体机制
    （当前 `server.run()` 是 single-shot，需要在 runForever 外包一层 restart-loop + 一个区别于
    shutdown 信号的 restart 信号；`server.stop()` 现在的语义是 graceful-teardown=end，
    要证明它可被复用为 restart 而非 end），并划清 cycle 的可恢复子集（host 存活但 server 卡死 /
    前台漂移，对应 supervisor 的 `health-unreachable ×3` 触发）与不可恢复子集
    （`** TEST INTERRUPTED **` = host 已死，进程内无能为力，必须 fallback 到 xcodebuild spawn）。
  - `infeasible` ⇒ doc 必须点名 XCTest 生命周期的结构性阻塞行（哪个 API / 哪条 run-loop
    契约禁止进程内 server 重启），并给出 re-tier 建议（cycle 保持 xcodebuild，C2 目标改为
    压缩 down/up 的 SIGINT-wait + respawn 常数）。
- **Top-N attack 清单**（仅 feasible 时有意义）：每项 `file:line` + concrete code change +
  µs/ms 估算 + semantic class + blast radius。**Pre-Phase-B gate**：Top-1 attack target 必须
  在 mini 实测总 cycle self-time 里占 ≥ 双位数 pp，否则该 attack 的估算是 hand-wave，不入清单。

**重构**
- 无（S1 无 edit 权，纯 research）。

### S2. Attack —— 进程内 soft-cycle 落地（worktree 隔离，仅当 S1 verdict == feasible）

> 入口条件（机器可判）：`grep -q '^Verdict: feasible' docs/perf/v2.8-c2-cycle-decomposition.md`。
> 若 verdict 为 `infeasible`，**跳过 S2**，直接走「完成后动作」的 re-tier 分支——这不是失败，是
> decomposition 的诚实产出（methodology §6：做完真没 attack 才 earn 结论，硬塞才是自欺）。

**红（写测试）**
- 文件：`crates/smix-cli/src/runner.rs`（`#[cfg(test)]`）+ 一条设备侧 harness。
- 断言 1（纯逻辑，CI）：新增 cycle 路径分流函数——给定「host 存活 + server 可达/卡死」判定，
  选 in-process soft-cycle；给定 `** TEST INTERRUPTED **` / host 已死，fallback 到现有
  xcodebuild hard-cycle。mock 触发条件，断言选对分支。先跑成红。
- 断言 2（device metric，mini）：measure harness 产出 `runner_cycle.warm_ms` JSON，喂给
  `smix bench --current-file <json>`（`bench.rs:193` 已支持外部 current 注入），断言进程内
  soft-cycle 路径实测 warm < 1000ms（目标带宽 ~500ms）。

**绿（实现）**
- 文件：`swift-bridge/Sources/SmixRunnerCore/SmixRunnerServer.swift`（restart-loop + restart 信号，
  区别于 shutdown 信号）、`swift-bridge/SmixRunnerUITests/SmixRunnerUITests.swift`
  （新增 `POST /soft-cycle`：bounce FlyingFox 路由/状态 + 通过现有 `resolveApp` 机制 rebind
  XCUIApplication，`test_runForever` 不返回）、`crates/smix-cli/src/runner.rs`（`cycle()` 先尝试
  进程内 `/soft-cycle`，失败或 host 死才 fallback 到现有 `down()`+`up()`）。
- API（草签，最终以 S1 doc 为准）：
  - Swift：`POST /soft-cycle` → `{ ok, rebound: bool, wallMs }`；additive 新路由，旧 wire 不改。
  - Rust：`cycle()` 内部先 `try_soft_cycle(port) -> Result<Duration, SoftCycleUnavailable>`，
    `Err` ⇒ 原 xcodebuild 路径（保留为 hard fallback，契约不变）。
- 关键点：
  1. **additive-only**：只加 `/soft-cycle` 路由 + restart 信号，不改 `/shutdown`、不改
     `test_runForever` 的既有 exit-0 graceful 语义。`cargo-semver-checks` 每步守 additive。
  2. **N=1 契约不变**：无 supervisor / host 已死时，`cycle` 行为与今天字节等价（走 hard fallback）。
  3. **Session 存活**：soft-cycle 后 `smix-sessions.json`（`SmixRunnerUITests.swift:1036` 附近）
     与 `Session-Id` 必须继续跨 cycle 存活——进程没重启，这一半天然成立，测试须钉住。
  4. attack 串行全上，`smix bench` **在整批 attack 后跑一次**验证累计过 variance band，不每个单独跑
     （单个 ms 级 gain 常在 noise 内）。

**重构**
- 仅当 `cycle()` 分流逻辑出现明显坏味时抽小函数；重构期测试保持绿，不引入新行为。

## Checkpoint C2 验收

```bash
# ---- 1. Decomposition 门（确定性，CI 可跑）----
test -f docs/perf/v2.8-c2-cycle-decomposition.md
STAGES=$(grep -c '^### S[0-9]' docs/perf/v2.8-c2-cycle-decomposition.md); [ "$STAGES" -ge 18 ]
grep -q '^## Budget validation' docs/perf/v2.8-c2-cycle-decomposition.md
grep -Eq '^Verdict: (feasible|infeasible)$' docs/perf/v2.8-c2-cycle-decomposition.md

# ---- 2. 依 verdict 分流验收（分支在 S1 时已定死，此处只核对已提交后果与 verdict 一致）----
if grep -q '^Verdict: feasible$' docs/perf/v2.8-c2-cycle-decomposition.md; then
  cargo test -p smix-cli --test cycle_softcycle_dispatch      # 分流选择单测，exit 0
  cargo semver-checks check-release -p smix-cli               # additive，无 break
  grep -q 'runner_cycle.warm_ms' crates/smix-cli/bench/baseline.json
  python3 -c 'import json,sys; v=json.load(open("crates/smix-cli/bench/baseline.json"))["metrics"]["runner_cycle.warm_ms"]; sys.exit(0 if v < 1000.0 else 1)'
else
  grep -Eq '2026-.*C2.*(re-tier|结构性阻塞|infeasible)' docs/v2.md
fi
```

期望：整段 `exit 0`。`STAGES ≥ 18`、budget 段存在、verdict 行存在为不变的确定性门；
feasible 分支要求 `runner_cycle.warm_ms < 1000`（mini 实测写入 C1 的 baseline.json）+
`cargo-semver-checks` additive 绿 + soft-cycle 分流单测绿；infeasible 分支要求 `docs/v2.md`
决策日志含 C2 re-tier 记录。**任一分支都无「manually verify / looks correct」，全命令可判。**

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.8-c2-hot.md`（`mv docs/plan-hot.md docs/plan-history/v2.8-c2-hot.md`）。
2. 非微小决策入 `docs/v2.md` 决策日志：C2 的 feasibility verdict、cycle 是否 re-tier 成
   soft/hard 两层、`runner_cycle.warm_ms` 进 bench corpus 的注入路径（外部 `--current-file`，
   避免让纯 device-free corpus 每次 CI 都要设备——这条冷计划假设的订正也在此记）。
3. Checkpoint 验收命令通过 **且** 上层明确说「开始 C3」后，调 sub-agent 生成新 `plan-hot.md`
   （覆盖 C3：截图管线 hoist，`simctl io screenshot` p95 < 100ms，同样 decomposition-first，
   参考实现对照 `simctl` 截图源码路径）。不在主对话里展开。

## 本段预先定死的口径

- **Decomposition-first（不可协商）**：本段是 perf checkpoint，先 S1 拆解（read-only，无 edit 权，
  只 Read/Bash/Grep），产 `docs/perf/v2.8-c2-cycle-decomposition.md`（≥18 stage、side-by-side、
  ±20% 对账、runtime 计数验证、Top-N attack + Pre-Phase-B ≥双位数 pp gate），**再** S2 attack
  （worktree 隔离）。禁止边读边试。
- **§1 自欺触发词一律禁用**：不允许在 doc / 思路 / commit 里出现「architectural ceiling」
  「XCTest 结构性不可达（未拆够）」「within variance」「single run shows」等——用具体
  `file:line` + 实测 median-of-N 替代。REVERT 是诚实答案，不是失败。
- **Feasibility gate 是线性的**：S1 的 `Verdict:` 行**先**决定走哪条，S2 与验收都以它为准，
  不在实现期临时拍（满足 CLAUDE.md §2「if A then B else C → 上一层先决定」）。已定死的判据：
  「XCUIApplication 进程内 rebind」已由现存 `resolveApp` / `/session/*`（`SmixRunnerUITests.swift:1120,2591`）
  证明 feasible——真正待 S1 定夺的只有「FlyingFox 能否在 `test_runForever` 存活期内 bounce」
  这一个未知（当前 `server.run()` single-shot，shutdown = end）。
- **诚实 re-tier 优先于硬塞**：若 S1 判 `infeasible`，正确产出是记录结构性阻塞 + re-tier，
  而非把一个会让 XCUIApplication 处于坏状态或让 XCTest run 提前结束的进程内重启硬推上去——
  那比 xcodebuild spawn 更糟（correctness > 省 spawn 的那 2.5s）。
- **additive 不变量**：任一步 `cargo-semver-checks` 守 pre-fold v2.0 wire/ABI additive；
  `/soft-cycle` 只加不改，`cycle()` 保留 xcodebuild hard fallback 使 N=1 / host-dead 契约字节等价。

# plan-hot — v2.8 到 C3：截图管线 hoist（simctl io screenshot p95 < 100ms，decomposition-first）

## 目标 checkpoint

C3：`smix` 取一帧截图的 p95 从当前 ~230ms 降到 < 100ms。做法由 S1 decomposition 的实测拆解决定 ——
把 sRGB chunk splice + 自适应 pacer 下沉到 C-backed helper / async pool,或砍掉路径上被证明是大头的那一段。
若 S1 判定 ~230ms 的大头是 `xcrun simctl io screenshot` 子进程本身(smix 够不到的 kernel/CoreSimulator 成本),
则诚实 re-tier(目标改为「smix 侧开销 < Xms」而非总 p95 < 100ms),不硬塞。**哪条走由 S1 的实测 verdict 定,不在实现期临时拍。**

## 前置条件

```bash
git status --short                                              # 期望：空
grep '^version' Cargo.toml | head -1                            # 期望：2.0.0
cargo test -p smix-cli --test cycle_softcycle_dispatch          # C2 soft-cycle 仍绿
cargo test -p smix-cli --bin smix bench                         # C1 bench 仍绿
```

- C1（`smix bench`）+ C2（soft-cycle）已 land。C3 的 perf 工作用 C1 的测量纪律。

## 已经查清、不必重查的事实

- **截图管线在 `crates/smix-simctl/`**：`screenshot_pacer.rs`（自适应 pacer：interval floor + slow-path lift + circuit breaker）
  + `lib.rs` 的 `screenshot()`（`:1909` 附近，`xcrun simctl io <udid> screenshot` + sRGB chunk splice + PNG）。
- **当前实测(本 session 早前,mini)**：`simctl io screenshot` ~187–286ms(粗测,非受控)。
  **S1 起手必须受控重量**（median-of-N + sample stdev，Pre-Phase-A gate：粗测可能是 noise misread）。
- **C2 的两条教训直接适用**：① 前提要实测复核（C2 的「~3s」实为 ~36s，错 12×）；
  ② attack 的估算可能错（C2 估 2.9s 实测 8s，错 2.7×）—— **measurement 是唯一裁判，estimate 不回改假装命中**。

## 本段预先定死的口径

- **Decomposition-first（不可协商）**：先 S1 拆解（read-only，无 edit 权，只 Read/Bash/Grep），
  产 `docs/perf/v2.8-c3-screenshot-decomposition.md`（≥18 stage、side-by-side 对照参考路径、±20% 对账、
  runtime 计数验证、Top-N attack + Pre-Phase-B ≥双位数 pp gate），**再** S2 attack（worktree 隔离）。禁止边读边试。
- **参考实现**：对照 `simctl` 截图源码路径 / `CoreSimulator` framebuffer 抓取路径，回答「~230ms 里 smix 侧
  （sRGB splice + pacer + PNG）占多少、simctl/CoreSimulator 子进程占多少」。**这是 feasible / re-tier 的分界**。
- **§1 自欺触发词禁用**：不允许「memory-bandwidth bound」「simctl kernel cost 不可触」等 hand-wave ——
  用具体 `file:line` + 实测 median-of-N 替代。若真是子进程 spawn 主导，读 simctl 源确认对手怎么省的，不推锅。
- **诚实 re-tier 优先于硬塞**：若总 p95 < 100ms 结构上够不到（子进程主导），产出是 re-tier（smix 侧开销目标）+ 记录，
  不把一个牺牲正确性 / 增加 flaky 的 hack 硬推上去（correctness > 省的那几十 ms）。

## 步骤（线性，无分叉）

### S1. 拆解截图路径并回答 feasibility（read-only，无 edit）

**红**：一条 bench 断言当前截图 p95 vs <100ms 目标的 gap；bench corpus 里尚无 `screenshot.*` 度量。

**绿**：产 `docs/perf/v2.8-c3-screenshot-decomposition.md` —— ≥18 stage 拆 `screenshot()` 全路径
（simctl 子进程 spawn / CoreSimulator framebuffer 抓 / PNG encode / sRGB chunk splice / pacer interval 判定 / 落盘）,
side-by-side 对照参考路径，±20% 对账受控实测 p95，runtime 计数验证任何 high-level claim。
**doc 末尾单行机器可判**：`Verdict: feasible`（smix 侧有 ≥双位数 pp 可攻，能把总 p95 压到 <100ms）或
`Verdict: retier`（大头在 simctl/CoreSimulator 子进程，目标改 smix 侧开销）。

### S2. Attack（worktree 隔离，仅当 verdict == feasible）

> 入口：`grep -q '^Verdict: feasible' docs/perf/v2.8-c3-screenshot-decomposition.md`。retier ⇒ 跳过 S2,走 re-tier 分支。

**红**：纯逻辑单测钉住 attack 的可测部分（如 sRGB splice 的 zero-copy 化 / pacer 判定）；device metric harness 产
`screenshot.p95_ms` 喂 `smix bench --current-file`。

**绿**：按 Top-N attack 实施（worktree），additive-only（`cargo-semver-checks` 每步守），attack 串行全上后
`smix bench` 跑一次验证累计过 variance band。

## Checkpoint C3 验收

```bash
test -f docs/perf/v2.8-c3-screenshot-decomposition.md
STAGES=$(grep -c '^### S[0-9]' docs/perf/v2.8-c3-screenshot-decomposition.md); [ "$STAGES" -ge 18 ]
grep -q '^## Budget validation' docs/perf/v2.8-c3-screenshot-decomposition.md
grep -Eq '^Verdict: (feasible|retier)$' docs/perf/v2.8-c3-screenshot-decomposition.md
if grep -q '^Verdict: feasible$' docs/perf/v2.8-c3-screenshot-decomposition.md; then
  cargo semver-checks check-release -p smix-simctl        # additive
  # mini 受控实测 p95 < 100ms(记入决策日志,同 C2:设备量不进 committed baseline)
else
  grep -Eq '2026-.*C3.*(re-tier|retier|子进程主导)' docs/v2.md
fi
```

期望：确定性门（stage≥18 / budget / verdict 行）+ 依 verdict 的分支后果一致。设备 p95 记决策日志(跨机会变,不进 baseline)。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.8-c3-hot.md`。
2. 决策日志记 C3 的 verdict + 实测 p95 + 是否 re-tier。
3. 验收通过后调 sub-agent 生成 C4 热计划（多 sim 编排 `--parallel N`，能力 checkpoint 非 perf）。

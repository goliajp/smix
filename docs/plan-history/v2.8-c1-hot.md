# plan-hot — v2.8 到 C1:bench 回归检测(perf 工作的测量前置)

## 目标 checkpoint

**C1**:`smix bench` 子命令跑 perf 语料、对比 committed baseline,任一指标退化 > 5% 即非零退出。
通过后,C2/C3 的 perf 工作**有 delta 可看** —— 没有它,进程内 cycle / 截图 hoist 的优化是盲猜。

今天的 perf 模型是 26 个 `perf_gate.rs` 各自硬编码**绝对 ns 上限**(`< 15 ns`)。
绝对天花板挡得住暴涨,挡不住天花板下的**缓慢漂移** —— 一次 +4% 不触顶,十次就 +40%。
C1 补 baseline 相对层:比对 committed 基线,漂移超 5% 就红。

## 前置条件

```bash
git status --short                         # 期望:空
grep '^version' Cargo.toml | head -1       # 期望:2.0.0
cargo test -p smix-cli --test list_sessions_no_nested_runtime  # 折入前修复仍绿
```

## 已经查清、不必重查的事实

- **perf 语料已存在**:26 个 `perf_gate.rs`(`crates/*/tests/` 与 `*/benches/`),
  各测跑 N 次取中位,断言硬编码绝对 ns 上限。C1 **复用**它们的测量,不另造语料。
- **没有 committed baseline + delta 比对** —— 这正是 C1 要补的缺口。
- **测量机器敏感**(M-series / CI scheduler jitter),所以**回归判据本身(纯比较)必须与测量隔离**:
  给定 baseline 数 + current 数 + 阈值,是否退化是**确定性纯函数**,能单测钉死;
  测量的抖动不该污染判据的可测性。

## 本段预先定死的口径

### 口径 — 纯判据与机器测量分离

C1 的红/绿钉在**纯比较引擎**上,不钉在「实测某数 < 某 ns」上(那是机器敏感、会 flaky):

- 纯引擎:`compare(baseline, current, tolerance) -> Vec<Regression>` —— 完全确定性,fixtures 单测。
- 语料运行:`smix bench` 跑 perf 语料产 current、读 committed baseline、喂引擎、报告 + 退出码。
  这一层的测试只验「退出码随引擎结果」,不验具体 ns。

绝对 ns 上限(既有 26 gate)**保留** —— 它挡暴涨,baseline 层挡漂移,两者叠加不替换。

## 步骤(线性,2 个)

### S1. 纯回归比较引擎

**红(写测试)**
- 文件:`crates/smix-cli/src/bench.rs`(新)+ 同文件 `#[cfg(test)]`
- 断言:
  - baseline `100ns` vs current `104ns`,tolerance 5% → **无** regression(4% < 5%)。
  - baseline `100ns` vs current `106ns` → **一条** regression,含指标名 + 旧值 + 新值 + 百分比。
  - current 缺某 baseline 里有的指标 → 明确报「指标消失」,不静默当通过。
  - baseline 缺某 current 新增的指标 → 不算 regression(新指标无基线),但列出待 `--update-baseline`。

**绿(实现)**
- 文件:`crates/smix-cli/src/bench.rs`
- API:`pub fn compare(baseline: &BenchSet, current: &BenchSet, tol_pct: f64) -> Vec<Regression>`
  + `BenchSet`(指标名→中位 ns 的有序 map,serde)+ `Regression { metric, baseline, current, pct }`。
- 关键点:百分比按 `(current-baseline)/baseline*100`;只报**变慢**方向;缺失指标是独立 variant 不混入。

### S2. `smix bench` 子命令 + committed baseline

**红(写测试)**
- 文件:`crates/smix-cli/tests/bench_gate.rs`(新)
- 断言:`smix bench` 对一个「current 比 baseline fixture 慢 10%」的注入场景**非零退出**且 stdout 列出该指标;
  对「current 在 5% 内」的场景**零退出**。(注入走 env 或 `--current-file` 喂假 current,不依赖真实测量避免 flaky。)

**绿(实现)**
- 文件:`crates/smix-cli/src/main.rs`(加 `Cmd::Bench`)+ `bench.rs`
- API:`smix bench`(跑语料→current→比 committed baseline→>5% 非零)
  + `smix bench --update-baseline`(把 current 写回 committed baseline 文件)
  + `--current-file <path>`(测试注入用,喂预制 current 跳过测量)。
- baseline 文件:`crates/smix-cli/bench/baseline.json`(committed,`--update-baseline` 重写)。
- 关键点:退出码由 S1 引擎结果决定;报告人类可读 + 机器可 grep;不吞错误。

**重构**
- 无(除非 `BenchSet` 与既有 perf_gate 的测量辅助有明显可共用点,且不牵动 26 gate)。

## Checkpoint C1 验收

```bash
cargo test -p smix-cli --lib bench                      # 期望:引擎单测绿
cargo test -p smix-cli --test bench_gate                # 期望:注入回归→非零,注入无回归→零
cargo run -p smix-cli -- bench --help                   # 期望:列出 --update-baseline / --current-file
cargo-semver-checks check-release -p smix-cli 2>&1 | tail -1  # 期望:additive(bench 是新增,无 break)
```

期望:前两条 exit 0;`bench --help` 含两个 flag;semver 确认 additive。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.8-c1-hot.md`
2. 调 sub-agent 生成 C2 热计划(进程内 runner cycle,**decomposition-first**:S1 read-only 拆解、S2 worktree attack),见 CLAUDE.md §6 + `plan-cold/v2.8-faster-and-wider.md` 追加口径

# plan-hot — v2.8 到 C4：多 sim 编排（`smix run --parallel N`，N=1 保持现契约）

## 目标 checkpoint

C4：`smix run --parallel N <flows...>` 把 N 条 flow 分片到 M 台 sim 并发跑，每台 sim 各自一对
runner + supervisor；`--parallel 1`（默认）与今天的单-sim 行为**字节等价**。这是能力 checkpoint
（非 perf，不走 decomposition-first）。

## 前置条件

```bash
git status --short                                    # 期望：空
grep '^version' Cargo.toml | head -1                  # 期望：2.0.0
cargo test -p smix-simctl --test types 2>/dev/null || cargo build -p smix-simctl  # C3 capture 仍编译
```

## 已经查清、不必重查的事实

- **单-sim run 入口**：`crates/smix-cli/src/main.rs` `Cmd::Run`（`--device <UDID>` 单台）+ 底层
  runner 生命周期 `crates/smix-cli/src/runner.rs`（`up` / `down` / `cycle`，含 C2 的 soft-cycle）。
- **sim 注册表**：`.smix/sims.json`（alias → udid），`sim-guard` 强制显式 UDID。多 sim 分片要从
  注册表/显式列表取 M 台 UDID，每台 `runner up`（各自 port，`runner.rs` 已支持 per-request port）。
- **supervisor 已 per-runner**：`smix runner supervise` 附一个 runner（`state.json` 记 pid）。
  N 台 sim = N 对 runner+supervisor，互不干扰（各自 port + state）。
- **N=1 不变**：`--parallel` 缺省 = 1 = 现有单-sim 路径，测试须钉住字节等价。

## 本段预先定死的口径

- **additive**：`--parallel` 是新 flag；不改 `smix run` 现有单-sim 语义 / wire / SDK 面。
  `cargo-semver-checks` 守（若动 library crate）。分片编排在 CLI 层（bin，无 library API）。
- **sim-guard 纪律**：每台 sim 显式 UDID；分片器从 `--device` 列表或注册表取 M 台，**不** booted/all。
- **失败隔离**：一台 sim 的 flow 失败不杀其它分片；汇总每台的 pass/fail，退出码反映整体。
- **资源礼让**：动多 sim 前查 `pgrep -fl "runner.ts|smix run|supervise"`（活动 batch 是绝对边界）。

## 步骤（线性，2 个）

### S1. 分片器 + 每-sim runner 对（纯逻辑先行）

**红**
- 文件：`crates/smix-cli/src/`（新分片模块）+ `#[cfg(test)]`
- 断言：给定 K 条 flow + M 台 UDID + `parallel=N`，分片函数产出「哪条 flow 上哪台 sim」的确定性
  分配（round-robin 或 least-loaded），且 N=1 时全部落单台（= 现契约）；一台的失败不影响其它分配。

**绿**
- 最小实现分片分配 + 每台 sim `runner up`（各自 port）/ `down` 生命周期编排（复用 `runner.rs`）。
- 关键点：并发用 tokio join；每分片独立 `App` + runner client；N=1 走原路径不新建编排。

### S2. `--parallel` flag + 汇总退出码

**红**
- 文件：`crates/smix-cli/tests/`（新 bin 测试，mock 或 dry-run 层）
- 断言：`smix run --parallel 2 a.yaml b.yaml --device <U1> --device <U2>` 分片到两台；
  一台失败 → 整体非零退出 + 汇总每台结果；`--parallel 1` 与无 flag 字节等价。

**绿**
- `Cmd::Run` 加 `--parallel <N>`（默认 1）+ 多 `--device` 收集；N>1 走分片编排，N=1 走原路径。
- 关键点：汇总人类可读 + 机器可 grep；退出码 = 任一分片失败即非零；不吞任何分片的错误。

## Checkpoint C4 验收

```bash
cargo test -p smix-cli --bin smix parallel        # 分片器单测 exit 0
cargo test -p smix-cli --test <parallel_bin_test> # --parallel flag + 汇总退出码 exit 0
cargo run -p smix-cli -- run --help | grep -q parallel   # flag 文档化
cargo semver-checks check-release -p smix-cli 2>&1 | tail -1  # additive（bin 天生 additive）
# 设备 e2e(mini,让位活动 batch)：一条 flow 在 --parallel 2 跑两台 sim 各自过(记决策日志)
```

期望：单测 + bin 测试 exit 0；`--parallel` 进 `run --help`；e2e 两台各自过。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.8-c4-hot.md`。
2. 决策日志记 C4 的分片策略（round-robin / least-loaded）+ e2e 实测。
3. 验收通过后热化 C5（real-sim 压测台，20 流夜间 CI）。

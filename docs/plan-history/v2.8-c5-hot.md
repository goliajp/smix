# plan-hot — v2.8 到 C5：real-sim 分级压测台（20 流夜间 CI）

## 目标 checkpoint

C5：一个 20 流 bootstrap corpus 在夜间 CI 对一台 real sim 跑，取代当前 ad-hoc 的单 smoke，
成为**分级 stress+smoke** 管线 —— smoke（少数关键流，每 PR）+ stress（20 流全量，夜间）。
输出机器可判的 pass/fail + 每流耗时,退化可追。能力 checkpoint（非 perf）。

## 前置条件

```bash
git status --short                                 # 期望：空
grep '^version' Cargo.toml | head -1               # 期望：2.0.0
cargo test -p smix-cli --bin smix parallel         # C4 分片器仍绿
ls scripts/release/smoke-v1.smoke.sh               # 现有 smoke gate 在
```

## 已经查清、不必重查的事实

- **现有 smoke**：`scripts/release/smoke-v1.smoke.sh`（runner up → 一条 yaml → cycle → 验 session → down），
  ship.sh 的硬 gate。**是 ad-hoc 单流**，不分级。
- **现有 corpus gate**：`scripts/release/corpus-gate.sh`（跑 bootstrap corpus，每流非零即挂）。C5 扩它到 20 流 + 分级。
- **C4 的 `--parallel`**：20 流可用 `smix run --parallel N` 分片到 M 台 sim 加速（如 CI 有多 sim）。
- **CI**：`.github/workflows/ci.yml`（现无 nightly；C5 加一个 nightly job）。
- **bench（C1）**:`smix bench` 已能对 corpus 记 baseline —— stress 的每流耗时可喂它做回归线。

## 本段预先定死的口径

- **分级不是两套代码**：smoke = corpus 的一个标了 `tier: smoke` 的子集；stress = 全量。一个 corpus，两个 tier 选择器。
- **机器可判**：每流 pass/fail + 耗时进结构化输出（JSON），CI 判据读它,不靠人看日志。
- **设备礼让 + 显式 UDID**：nightly job 用 pinned sim（sim-guard 显式 UDID）；动设备前 pgrep 活动 batch。
- **corpus 是真流不是 mock**：20 流覆盖 launch / tap / fill / scroll / assert / session / 跨屏导航等真实表面（复用 examples/ 黄金路径 + fixture）。

## 步骤（线性，2 个）

### S1. 20 流 corpus + tier 选择器（纯结构先行）

**红**
- 文件：`scripts/release/`（corpus 清单，如 `stress-corpus.yaml` 列 20 流 + 每流 `tier`）+ 一个纯选择器测试（Rust 或 py）
- 断言：给定 corpus 清单,选 `tier=smoke` 得关键子集、`tier=all` 得 20 流;清单每流指向存在的 yaml。

**绿**
- corpus 清单（20 真流,复用/新增 examples + fixture）+ 选择器（tier → 流列表）。
- 关键点：清单是单一真源;smoke ⊆ stress;每流 yaml 真能 parse（过 guide_gate 同款标准）。

### S2. 分级压测脚本 + 结构化输出 + nightly CI

**红**
- 文件：`scripts/release/stress-gate.sh`（新）+ 其自测（注入一个失败流断言非零 + JSON 记该流 fail）
- 断言：跑 corpus(可选 `--parallel`)→ 每流结构化结果(JSON: 流名/pass-fail/耗时)→ 任一 fail 非零退出。

**绿**
- `stress-gate.sh`：选 tier → 跑（`smix run` 或 `--parallel`）→ 聚合 JSON → 退出码。
- `.github/workflows/ci.yml` 加 nightly job 跑 `tier=all`;PR job 跑 `tier=smoke`(或保留现 smoke)。
- 关键点：耗时可喂 `smix bench --current-file` 做回归(同 C1/C2/C3 设备量不进 committed baseline 的口径)。

## Checkpoint C5 验收

```bash
test -f scripts/release/stress-corpus.yaml
COUNT=$(grep -c "tier:" scripts/release/stress-corpus.yaml); [ "$COUNT" -ge 20 ]
bash scripts/release/stress-gate.sh --tier smoke --dry-run   # 选 smoke 子集,解析全过(无设备)
cargo test <tier-selector-test>                              # 选择器单测 exit 0
grep -q "stress-gate\|stress-corpus" .github/workflows/ci.yml # nightly job 接线
# 设备 e2e(mini,让位活动 batch)：stress-gate --tier all 对一台 sim 全绿(记决策日志)
```

期望：corpus ≥20 流;选择器 + dry-run + CI 接线 exit 0;设备 stress 全绿。

## 完成后动作

1. 归档此文件到 `docs/plan-history/v2.8-c5-hot.md`。
2. 决策日志记 corpus 组成 + 分级判据 + 设备 stress 实测。
3. 验收通过后热化 C6（Android 运行时 parity，UiAutomator 补 rate-limit pacer + app-alive cache）。

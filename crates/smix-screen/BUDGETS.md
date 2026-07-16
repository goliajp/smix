# smix-screen performance budgets

Regression-catch budgets enforced by `tests/perf_gate.rs`. Each budget
is set with 3-10× headroom over the observed P50 on a dev machine so
CI fails on order-of-magnitude regressions, not micro-noise.

Run `cargo test -p smix-screen --release --test perf_gate` to check.
Run `cargo bench -p smix-screen --bench visibility` for the full
criterion baseline.

## Path taxonomy

`is_visible_enough` + `visible_area` are **per-candidate hot** paths —
called once per node during every selector resolve DFS. A 100-node
tree is the typical tap target; 500-node trees occur in dense scroll
lists. Per the smix perf mandate ("borrow the opportunity to greatly
improve performance" — user 2026-05-25), these primitives must stay
sub-microsecond.

## Budgets

| Path | Budget | Observed P50 (M-series, release) | Headroom |
|---|---:|---:|---:|
| `is_visible_enough` — in-view (happy path) | < 10 ns | ~1.0 ns | ~10× |
| `is_visible_enough` — zero-bounds early reject | < 5 ns | ~0.45 ns | ~11× |
| `is_visible_enough` — unknown-root conservative pass | < 10 ns | ~0.58 ns | ~17× |
| `is_visible_enough` — offscreen (intersection empty) | < 10 ns | ~0.81 ns | ~12× |
| `visible_area` — intersection arithmetic | < 10 ns | ~1.11 ns | ~9× |
| `dfs_collect 100-node` composite filter | < 500 ns | ~233 ns | ~2.1× |

## Comparative numbers (vs TS V8 baseline)

Real measured medians on M-series Mac, release profile, ≥100-sample.
**These are the numbers to quote.**

| Operation | TS V8 baseline | Rust release | Speedup |
|---|---:|---:|---:|
| `is_visible_enough` in-view | 21.2 ns | **1.0 ns** | **21×** |
| `is_visible_enough` zero-bounds reject | 20.5 ns | **0.45 ns** | **46×** |
| `is_visible_enough` unknown-root pass | 20.6 ns | **0.58 ns** | **35×** |
| `is_visible_enough` offscreen | 20.7 ns | **0.81 ns** | **26×** |
| `visible_area` intersection arith | 20.9 ns | **1.11 ns** | **19×** |
| `dfs_collect` 100-node composite | 788 ns | **233 ns** | **3.4×** |

Primitives sit at single-cycle territory because they inline to
zero-allocation arithmetic on three f64 pairs; `dfs_collect` is
3.4× because allocation + Vec push dominates over the predicate work.

## Memory

Measured via `examples/dhat_profile.rs` (10,000-iter loop calling
`collect_visible_summaries(tree_100, 1000)`):

| Metric | Total (10K iter) | Per call |
|---|---:|---:|
| bytes alloc'd | 286 MB | **28.6 KB / call** |
| heap blocks | 1,070,000 | **107 blocks / call** |
| peak resident | 14.7 KB | 102 blocks |

Reason it's heavy: `ElementSummary` carries owned `Option<String>` for
`name` / `id` / `text`, and `collect_visible_summaries` clones each
visible node's name string. 100-node tree → ~100 String clones per
call. Callers that pre-filter to a smaller candidate set (or replace
`String` with `&str` borrows) cut this proportionally.

Re-derive: `cargo run --release --example dhat_profile -p smix-screen`,
then open `dhat-heap.json` with the dhat viewer.

## Methodology

- Each test runs the path 100+ times under criterion's harness.
- The **median** sample is asserted under the budget, not the mean —
  median is robust to occasional GC / context-switch noise.
- Budgets are **wall-clock**, not CPU time.
- Profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`,
  `debug = true`.

## When to re-measure

- Touching `is_visible_enough` / `visible_area` / `collect_visible_summaries` body.
- After Rust toolchain bumps (LLVM version change can shift inline thresholds).
- CI runner class change.

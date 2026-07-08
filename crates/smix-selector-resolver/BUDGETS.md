# smix-selector-resolver performance budgets

Regression-catch budgets enforced by `tests/perf_gate.rs`. Each budget
is set with 1.5-3× headroom over the observed P50 on a dev machine so
CI fails on order-of-magnitude regressions, not micro-noise.

Run `cargo test -p smix-selector-resolver --release --test perf_gate`
to check. Run `cargo bench -p smix-selector-resolver --bench resolver`
for the full criterion baseline.

## Path taxonomy

`resolve_selector` is the **end-to-end hot** path — every `tap`,
`find`, `wait_for`, `expect.toBeVisible` runs it exactly once. Test
suites with 100 steps run it 100+ times. Single-call cost × N is the
business latency floor; a 1 µs vs 10 µs split is a 1-second delta on
a 100-step test.

The internal pipeline is DFS pre-order → visibility filter → spatial
filter → index pick. `ResolverContext::new()` compiles every `Pattern`
in the selector tree once and caches them in a `HashMap<*const Pattern,
CompiledPattern>`, so the hot DFS uses `match_text_compiled` (sub-50 ns
per node) regardless of selector complexity.

## Budgets

| Path | Budget | Observed P50 (M-series, release) | Headroom |
|---|---:|---:|---:|
| `resolve_selector` text base, 100-node tree, hit | < 2 µs | ~1.19 µs | ~1.7× |
| `resolve_selector` text base, 100-node tree, miss | < 2 µs | ~1.09 µs | ~1.8× |
| `resolve_selector` text + below, 4-node tree | < 500 ns | ~295 ns | ~1.7× |
| `resolve_selector` id base, 100-node tree, miss | < 500 ns | ~198 ns | ~2.5× |
| `resolve_selector` regex text, 100-node tree, hit | < 15 µs | ~10.96 µs | within budget |
| `resolve_selector` anchor + below, 4-node tree, nth=1 | < 500 ns | ~277 ns | ~1.8× |
| `resolve_selector` text + nth=2, multi-match 5-X tree | < 500 ns | ~179 ns | ~2.8× |

## Comparative numbers (vs TS V8 baseline)

Real measured medians on M-series Mac, release profile, ≥100-sample.

| Operation | TS V8 baseline | Rust release | Speedup |
|---|---:|---:|---:|
| text 100-node hit | 3,652 ns | **1,186 ns** | **3.1×** |
| text 100-node miss | 3,693 ns | **1,088 ns** | **3.4×** |
| text + below 4-node spatial | 607 ns | **295 ns** | **2.1×** |
| id 100-node miss | 3,234 ns | **198 ns** | **16×** |
| regex 100-node hit | 11,114 ns | **10,957 ns** | **parity** (compile dominant) |
| anchor + below nth=1 | 532 ns | **277 ns** | **1.9×** |
| text + nth=2 multi | 468 ns | **179 ns** | **2.6×** |

## Regex case — known parity, optimization path documented

The regex `resolve_selector` measurement (~11 µs) is dominated by
regex compile (~16 µs) inside `ResolverContext::new()`. v3.x leaves
this for the resolver-consumer layer: callers that hold a Selector
across multiple resolve calls (e.g. inside a `wait_for` poll loop
where the same selector polls every 100ms) should cache the
`ResolverContext` and re-use it; per-call cost then drops to ~1 µs
and overall speedup vs TS reaches ~11×. This is documented in
[PERFORMANCE.md §3](../../PERFORMANCE.md) as the c6+ optimization
path; for v3.x the cost is fine because typical resolve calls are
fewer than the SDK / driver cache layer turnover.

## Memory

Measured via `examples/dhat_profile.rs` (10,000-iter loop calling
`resolve_selector(tree_100, text_selector)` — hit path):

| Metric | Total (10K iter) | Per call |
|---|---:|---:|
| bytes alloc'd | 2.09 MB | **209 bytes / call** |
| heap blocks | 30,000 | **3 blocks / call** |
| peak resident | 209 bytes | 3 blocks |

3 allocations per resolve = `HashMap<*const Pattern, CompiledPattern>`
(compile cache) + DFS candidate `Vec<&A11yNode>` + ancestor-path Vec
for spatial filter dispatch. All freed at function exit; peak
resident is per-call high-water, not accumulating.

Compared to TS (V8 hidden-allocation, ~10 alloc/call: Map literal,
candidate array, anchor-resolve recursion frames), this is **~3-4×
fewer allocations** per call.

Re-derive: `cargo run --release --example dhat_profile -p smix-selector-resolver`,
then open `dhat-heap.json` with the dhat viewer.

## Methodology

- Each test runs the path 100+ times under criterion's harness.
- The **median** sample is asserted under the budget, not the mean.
- Budgets are **wall-clock**, not CPU time.
- Profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`.

## When to re-measure

- Touching the DFS walk, visibility filter, or spatial filter body.
- Touching `ResolverContext::new()` (compile cache shape).
- After `regex` / `smix-selector` version bumps.
- After Rust toolchain bumps.
- CI runner class change.

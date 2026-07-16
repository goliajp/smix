# smix-selector performance budgets

Regression-catch budgets enforced by `tests/perf_gate.rs`. Each budget
is set with 3-6× headroom over the observed P50 on a dev machine so
CI fails on order-of-magnitude regressions, not micro-noise.

Run `cargo test -p smix-selector --release --test perf_gate` to check.
Run `cargo bench -p smix-selector --bench matchtext` for the full
criterion baseline.

## Path taxonomy

`match_text_compiled` is the **per-candidate hot** path — called once
per node × once per selector during every DFS resolve. A 100-node tree
with a single text selector is the baseline cost driver. The
deliberate `Pattern` / `CompiledPattern` split exists so callers can
pay the regex compile cost (~16 µs) once and amortize across many
candidates.

| Path | Hot? | Budget rationale |
|---|---|---|
| `match_text_compiled` | **yes** | hot loop, must stay sub-50 ns |
| `match_text` (compile-on-call SDK convenience) | no | one-shot SDK path; compile cost is the budget |
| `Pattern::compile` (text) | text: yes | reused in resolver context cache; near-free |
| `Pattern::compile` (regex) | one-shot | amortized over many `match_text_compiled` calls |

## Budgets

| Path | Budget | Observed P50 (M-series, release) | Headroom |
|---|---:|---:|---:|
| `match_text_compiled` string label hit (field 1) | < 20 ns | ~4.9 ns | ~4× |
| `match_text_compiled` string miss (6-field scan) | < 20 ns | ~3.5 ns | ~6× |
| `match_text_compiled` string identifier hit (field 5) | < 20 ns | ~7.6 ns | ~3× |
| `match_text_compiled` regex default (pre-compiled) | < 50 ns | ~9.4 ns | ~5× |
| `match_text_compiled` regex /i explicit (pre-compiled) | < 50 ns | ~9.3 ns | ~5× |
| `match_text_compiled` regex on 100-char label | < 100 ns | ~32 ns | ~3× |
| `Pattern::compile` text (one-time) | < 50 ns | ~12 ns | ~4× |
| `Pattern::compile` regex short (one-time) | < 50 µs | ~16 µs | ~3× |
| `match_text` regex (compile-on-call SDK path) | < 30 µs | ~17 µs | within budget |

## Comparative numbers (vs TS V8 baseline)

Real measured medians on M-series Mac, release profile, ≥100-sample.
**These are the numbers to quote.**

| Operation | TS V8 baseline | Rust (compiled) | Speedup |
|---|---:|---:|---:|
| string label hit | 36.8 ns | **4.9 ns** | **7.5×** |
| string miss (6-field) | 41.4 ns | **3.5 ns** | **12×** |
| string identifier hit | 38.9 ns | **7.6 ns** | **5.1×** |
| regex default (/i auto) | 105.5 ns | **9.4 ns** | **11×** |
| regex /i explicit | 47.0 ns | **9.3 ns** | **5.0×** |
| regex on 100-char label | 52.2 ns | **31.5 ns** | **1.7×** |

## Memory

Measured via `examples/dhat_profile.rs` (1 × `Pattern::compile` regex
+ 10,000-iter loop of `match_text_compiled`):

| Metric | Total (1 compile + 10K matches) | Per match |
|---|---:|---:|
| bytes alloc'd | 24 KB | **~0 bytes / call** |
| heap blocks | 191 | **~0 blocks / call** |
| peak resident | 10.5 KB | 65 blocks |

Hot-loop `match_text_compiled` is **zero-alloc** — borrows the
compiled `regex::Regex` and the node's six `&Option<String>` field
references via `.as_deref()`. The 24 KB / 191 blocks come from the
one-time `Pattern::compile` (`regex::Regex` builds its DFA tables).

Per-call cost on a hot loop is amortized to single-digit bytes, which
is what the perf budget (5-30 ns/call) reflects.

Re-derive: `cargo run --release --example dhat_profile -p smix-selector`,
then open `dhat-heap.json` with the dhat viewer.

## Methodology

- Each test runs the path 100+ times under criterion's harness.
- The **median** sample is asserted under the budget, not the mean —
  median is robust to occasional GC / context-switch noise.
- Budgets are **wall-clock**, not CPU time.
- Profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`.

## Design note: Pattern / CompiledPattern split

Mirrors how TS-era code worked: a TS `RegExp` is a stateful object —
once you've constructed it, `.test()` is cheap; V8 caches the compiled
form on the RegExp instance. Rust's `regex::Regex` is the equivalent,
but `Pattern::Regex` is just data (string + flags) so `Pattern` can be
sent over the HTTP wire. `CompiledPattern` is the runtime cache shape;
consumers that loop over many nodes should call `Pattern::compile()`
once and reuse the resulting `CompiledPattern`. The resolver crate
([`smix-selector-resolver`](https://crates.io/crates/smix-selector-resolver))
does this via `ResolverContext`'s per-call HashMap cache keyed by
`Pattern` raw pointer.

## When to re-measure

- Touching `match_text_compiled` body or any of its six field accessors.
- Touching `Pattern::compile` (regex flag injection logic).
- After `regex` crate version bumps.
- After Rust toolchain bumps.
- CI runner class change.

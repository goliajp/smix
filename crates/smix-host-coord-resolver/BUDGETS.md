# smix-host-coord-resolver performance budgets

Regression-catch budgets enforced by `tests/perf_gate.rs`.

`resolve_to_norm_coord` is **hot path** — every `driver.tap` and
every host-side mode=resolve flow calls it once before injecting the
tap event. Sits on top of `smix-selector-resolver` (~1 µs
text-100-node-hit) + 1 centroid arithmetic + 1 normalize step — total
budget ~3 µs hit / ~3 µs miss / ~1 µs id-hit.

Run `cargo test -p smix-host-coord-resolver --release --test perf_gate`
to check. Run `cargo bench -p smix-host-coord-resolver --bench host_coord`
for the full criterion baseline.

## Budgets

| Path | Budget | Observed P50 (M-series, release) | Headroom |
|---|---:|---:|---:|
| `resolve_to_norm_coord` text 100-node hit | < 3 µs | ~1.2 µs | ~2.5× |
| `resolve_to_norm_coord` text 100-node miss | < 3 µs | ~1.1 µs | ~2.7× |
| `resolve_to_norm_coord` id 100-node hit | < 1 µs | ~250 ns | ~4× |

## Memory

- Pure pipeline. No standalone allocations — all heap activity is
  delegated to [`smix-selector-resolver`](https://crates.io/crates/smix-selector-resolver)
  (`ResolverContext::new()` + the DFS candidate `Vec`). The centroid +
  normalize math is **zero-alloc** arithmetic on six `f64`s.
- Returned `Result<(f64, f64), HostResolveError>` is stack-resident
  (HostResolveError is the 24-byte enum; `CentroidOutOfFrame` variant
  carries two `f64`).

## Methodology

- Each test runs 20,000 iterations under the in-house `measure_ns`
  helper.
- Profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`.

## When to re-measure

- Touching the centroid / normalize body in `resolve_to_norm_coord`.
- After `smix-selector-resolver` bumps (this stone's budget is layered
  on top of selector-resolver's; a regression there bubbles up here).
- CI runner class change.

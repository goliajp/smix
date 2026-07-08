# smix-error performance budgets

Regression-catch budgets enforced by `tests/perf_gate.rs`.

`smix-error` is **cold path** — only fires on assertion / driver
failure. But once it fires, the surface area is exactly what an AI
agent or developer reads, so we keep the cost low enough that failure
rendering doesn't dominate the failure window (e.g. a poll loop that
keeps catching `ElementNotFound` and rebuilding the failure prompt).

Run `cargo test -p smix-error --release --test perf_gate` to check.
Run `cargo bench -p smix-error --bench error` for the full criterion
baseline.

## Budgets

| Path | Budget | Observed P50 (M-series, release) | Headroom |
|---|---:|---:|---:|
| `edit_distance("Login", "Logon")` short | < 300 ns | ~80 ns | ~3.5× |
| `similarity("Login", "Logon")` short | < 400 ns | ~110 ns | ~3.5× |
| `build_suggestions(target, 10-elements)` | < 100 µs | ~5 µs | ~20× |
| `to_prompt` full failure (10 elements) | < 150 µs | ~10 µs | ~15× |

## Memory

- `edit_distance` allocates two `Vec<usize>` of width `min(a, b) + 1`
  (Wagner-Fischer two-row trick). For 5-char strings that's ~48 bytes
  total, dropped at function exit.
- `similarity` calls `edit_distance` plus zero extra alloc.
- `build_suggestions` allocates one `Vec<(f64, &'static str, String,
  usize)>` capped at the visible-element count, plus one `String` per
  surviving suggestion (≤ `SUGGESTION_TOP_N = 3` clones of
  `el.name` / `el.text`).
- `to_prompt` allocates one `String` for the rendered output
  (~1 KB typical) plus per-line `format!` intermediates that are
  freed as it walks `visible_elements` + `device_log` + `suggestions`.

### Measured (`examples/dhat_profile.rs`, 10,000-iter loop of
`build_suggestions` + `to_prompt` against a 10-element fixture)

| Metric | Total (10K iter) | Per call (build+render) |
|---|---:|---:|
| bytes alloc'd | 75.2 MB | **7.5 KB / call** |
| heap blocks | 1,660,000 | **166 blocks / call** |
| peak resident | 3.5 KB | 36 blocks |

7.5 KB / 166 blocks per render is heavy — dominated by per-element
string clones during candidate scoring + per-line `format!`
intermediates in `to_prompt`. This is acceptable because the path is
cold (only fires on failure); but a future opt path is to swap the
`Vec<(f64, &'static str, String, usize)>` candidate storage for
`SmallVec<[…; 8]>` + use `compact_str::CompactString` for the cloned
element names, cutting alloc count roughly in half.

Re-derive: `cargo run --release --example dhat_profile -p smix-error`,
then open `dhat-heap.json` with the dhat viewer.

## Methodology

- Each test runs 100,000 iterations (or /10 for the heavier paths).
- Profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`.

## When to re-measure

- Touching `edit_distance` (Wagner-Fischer body) or `similarity` normalization.
- Touching `build_suggestions` sort comparator or threshold logic.
- Touching `to_prompt` rendering format.
- After Rust toolchain bumps.

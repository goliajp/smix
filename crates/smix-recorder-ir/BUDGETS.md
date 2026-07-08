# smix-recorder-ir performance budgets

Regression-catch budgets enforced by `tests/perf_gate.rs`.

Per-action this is **warm** (one append per user-recorded action — but
generators serialize the full session, and the sort merge runs every
flush). Single-digit ns accessors keep generator scan cost negligible.

Run `cargo test -p smix-recorder-ir --release --test perf_gate` to
check. Run `cargo bench -p smix-recorder-ir --bench ir` for the full
criterion baseline.

## Budgets

| Path | Budget | Observed P50 (M-series, release) | Headroom |
|---|---:|---:|---:|
| `IRAction::kind` (8 variants) | < 5 ns | ~0.5 ns | ~10× |
| `IRAction::timestamp_ms` (8 variants) | < 5 ns | ~0.5 ns | ~10× |
| `sort_by_timestamp` (100 actions) | < 15 µs | ~3 µs | ~5× |
| `serde_json::to_string(&IRAction::Tap)` | < 2 µs | ~600 ns | ~3× |
| `serde_json::from_str::<IRAction>(json)` | < 10 µs | ~4 µs | ~2.5× |

## Memory

- `IRAction::kind` / `IRAction::timestamp_ms` are **zero-alloc**
  pattern matches returning a `&'static str` / `f64` respectively.
- `sort_by_timestamp` allocates one `Vec<IRAction>` of length `n`
  (idempotent merge — does not mutate the input).
- serde encode allocates one `String` per call; serde decode allocates
  the embedded `Selector` / `KeyName` / `SwipeDirection` tree.

## Methodology

- Each test runs 200,000 iterations for accessors, /10 or /20 for
  heavier paths.
- Profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`.

## When to re-measure

- Adding a new variant to `IRAction`.
- Touching `sort_by_timestamp` comparator.
- After `serde` / `serde_json` major-version bumps.

# smix-input performance budgets

Regression-catch budgets enforced by `tests/perf_gate.rs`. enum →
`&'static str` is a hot wire-encoder path (every recorder JSON emit,
every runner-wire serde encode hits it); serde round-trip is cool but
must still stay sub-microsecond.

Run `cargo test -p smix-input --release --test perf_gate` to check.
Run `cargo bench -p smix-input --bench input` for the full criterion
baseline.

## Budgets

| Path | Budget | Observed P50 (M-series, release) | Headroom |
|---|---:|---:|---:|
| `SwipeDirection::as_str` | < 5 ns | ~0.5 ns | ~10× |
| `KeyName::as_str` | < 5 ns | ~0.5 ns | ~10× |
| `serde_json::to_string(&KeyName::Return)` | < 200 ns | ~80 ns | ~2.5× |
| `serde_json::from_str::<SwipeDirection>("\"left\"")` | < 300 ns | ~120 ns | ~2.5× |

## Memory

- `SwipeDirection`, `KeyName` are `Copy + repr-default` enums — 1 byte
  each. `as_str()` returns `&'static str` — **zero alloc** on the hot
  path.
- `serde_json::to_string` allocates one `String` per call (~8 bytes for
  short enum names). Callers that loop should reuse a `Vec<u8>` writer
  via `serde_json::to_writer`.
- `serde_json::from_str` allocates **zero** for these enums (no
  String / Vec fields) — the result is a stack-resident enum value.

## Methodology

- Each test runs the path 1,000,000 iterations (or /10 for serde) under
  the in-house `measure_ns` helper.
- Profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`.

## When to re-measure

- Adding a new variant to either enum.
- After `serde` / `serde_json` major-version bumps.
- CI runner class change.

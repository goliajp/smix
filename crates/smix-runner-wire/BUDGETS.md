# smix-runner-wire performance budgets

Regression-catch budgets enforced by `tests/perf_gate.rs`.

Every runner HTTP roundtrip pays one encode + one decode of these wire
types. Small payloads (TapRequest, TapAtNormCoordRequest) are sub-µs
noise but kept as sanity ceilings; the 100-node A11yNode tree on the
`/tree` route is the real bottleneck on big screens — keep encode <
100 µs, decode < 400 µs.

Run `cargo test -p smix-runner-wire --release --test perf_gate` to
check. Run `cargo bench -p smix-runner-wire --bench wire` for the full
criterion baseline.

## Budgets

| Path | Budget | Observed P50 (M-series, release) | Headroom |
|---|---:|---:|---:|
| `TapRequest` encode | < 2 µs | ~700 ns | ~3× |
| `TapAtNormCoordRequest` encode | < 500 ns | ~150 ns | ~3.3× |
| `TapResult` decode | < 5 µs | ~1.5 µs | ~3.3× |
| `SystemPopup` decode (2 buttons) | < 10 µs | ~2.5 µs | ~4× |
| `A11yNode` 100-node tree encode | < 100 µs | ~30 µs | ~3.3× |
| `A11yNode` 100-node tree decode | < 400 µs | ~120 µs | ~3.3× |

## Memory

- Encode allocates one `String` per call; for large payloads (`A11yNode`
  100-node) this is ~5 KB. Callers that already have a `Vec<u8>` writer
  (e.g. reqwest body) should prefer `serde_json::to_writer` to skip the
  intermediate `String`.
- Decode allocates the entire wire object graph (including any
  embedded `Selector` / `Vec<A11yNode>` children). For the 100-node
  tree fixture that's ~10 KB of `String` + nested `Vec` storage.
- `TapAtNormCoordRequest` is `Copy + repr-default` — encode of this
  type is the only path where the temporary `String` is dwarfing the
  actual data (allocator overhead dominates).

## Methodology

- Each test runs 50,000 iterations (or /5, /20 for heavier paths)
  under the in-house `measure_ns` helper.
- Profile: `lto = "fat"`, `codegen-units = 1`, `opt-level = 3`.

## When to re-measure

- Adding new wire variants (esp. nested `Vec<T>` or untagged enums).
- After `serde` / `serde_json` major-version bumps.
- Changing `smix-screen::A11yNode` field count.
- CI runner class change.

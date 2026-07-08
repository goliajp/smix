# smix-recorder performance budgets

Regression-catch budgets for the host-side recorder. The recorder is
**cold path** at IRAction capture time (called once per SDK
side-effecting call) and again at session-end emit (one-shot per
session). Budget pressure is on the emit side, where a 1000-event
session is the realistic upper end of an unattended session.

Run `cargo bench -p smix-recorder --bench perf_gate` for the criterion
baseline.

## Path taxonomy

`RecordSession::record` is per-step (capture) — invoked once per SDK
method on `RecordingApp`. `snapshot` / `stop` are one-shot per session.
`generate_maestro_yaml` / `generate_rust` are one-shot per session and
walk the full event list once.

| Path | Hot? | Budget rationale |
|---|---|---|
| `RecordSession::record` (per-IRAction push) | per-step | < 1 ms (mutex + Vec push) |
| `RecordSession::snapshot` (clone events) | one-shot | < 5 ms for 1000-event session |
| `generate_maestro_yaml` (full emit) | one-shot | < 50 ms for 1000-event session |
| `generate_rust` (full emit) | one-shot | < 50 ms for 1000-event session |
| `cleanup` (shell out to `claude` CLI) | one-shot | external; dominated by CLI latency |

## Budgets

| Path | Budget | Observed P50 | Headroom |
|---|---:|---:|---:|
| `record(IRAction::Tap)` | < 1 ms | TBD | TBD |
| `generate_maestro_yaml` (1000 events) | < 50 ms | TBD | TBD |
| `generate_rust` (1000 events) | < 50 ms | TBD | TBD |

## Memory

- `RecordSession` owns a single `Mutex<Vec<IRAction>>`. Per-IRAction
  footprint is dominated by the `Selector` it carries (typically
  64-256 bytes; anchor-box variants reach ~1 KB).
- `generate_maestro_yaml` / `generate_rust` each allocate one
  `String` for the rendered output plus per-line `format!` /
  `serde_norway::to_string` intermediates. 1000-event sessions land
  in the ~100 KB range.

## When to re-measure

- Touching the `RecordingApp` delegation layer (each new wrapped SDK
  method adds one IRAction enum variant + one `record` call site).
- Touching either generator emit (yaml or Rust) format.
- After upgrading `serde_norway` or the `claude` CLI cleanup wire
  protocol.


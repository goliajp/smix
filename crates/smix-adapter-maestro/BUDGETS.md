# smix-adapter-maestro performance budgets

Regression-catch budgets enforced by `tests/perf_gate.rs` (added in c2 once
the yaml parser lands). Each budget will be set with 3-6× headroom over
the observed P50 on a dev machine.

Run `cargo test -p smix-adapter-maestro --release --test perf_gate` to
check (c2+). Run `cargo bench -p smix-adapter-maestro --bench parse` for
the full criterion baseline (c2+).

## Path taxonomy

`yaml_parse` is the **per-yaml hot path** — called once per yaml file at
adapter run start. Caller (CLI) calls it once per shell-out invocation; the
v3.17-ported 38 yaml all sit in the 1-3 KB range so per-yaml parse cost is
amortized over the 5-50 step run.

| Path | Hot? | Budget rationale (filled c2+) |
|---|---|---|
| `parse_flow_yaml` | startup | one-shot per shell-out, budget TBD |
| `Step::run` (per-step dispatch) | per-step | hot inside flow, budget TBD |

## Budgets

| Path | Budget | Observed P50 | Headroom |
|---|---:|---:|---:|
| `parse_flow_yaml` (3 KB yaml) | TBD (c2) | TBD | TBD |
| `Step::run` dispatch | TBD (c3) | TBD | TBD |

Note: c1 scaffolding only; budgets land alongside the parser (c2) and the
mapping layer (c3). perf_gate.rs is stubbed but skipped until c2.

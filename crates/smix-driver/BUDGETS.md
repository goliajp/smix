# smix-driver performance budgets

Regression-catch budgets for the decide-layer wrapper around
`HttpRunnerClient`. The driver sits on the SDK hot path (`app.tap` /
`app.find` / `app.wait_for` all flow through it), so latency overhead on
top of the runner round-trip is observable end-to-end.

Run `cargo bench -p smix-driver --bench perf_gate` for the criterion
baseline.

## Path taxonomy

`tap` / `find` / `wait_for` are the **per-step hot path** — called once
per user-visible step. `tree` is the **per-snapshot hot path** — invoked
inside the 5s implicit-wait poll loop. `describe` aggregates `tree` plus
a visibility filter; cold-path for failure rendering.

| Path | Hot? | Budget rationale |
|---|---|---|
| `SimctlDriver::tap` (resolve + Apple native event) | per-step | < 5 s total budget (POLL_INTERVAL_MS = 250, TOTAL_TIMEOUT_MS = 5000) |
| `SimctlDriver::find` (existence quick-probe) | per-step | < 100 ms when `/find` route applies; else `tree` cost |
| `SimctlDriver::tree` (full a11y snapshot) | per-poll | dominated by runner round-trip; driver overhead < 5 ms |
| `SimctlDriver::wait_for` (poll loop) | per-step | 5 s total budget, 250 ms poll cadence |
| `SimctlDriver::describe` (visible summaries) | cold | < 50 ms past `tree` |

## Budgets

| Path | Budget | Observed P50 | Headroom |
|---|---:|---:|---:|
| `tap` resolve + dispatch (host overhead, runner mocked) | TBD | TBD | TBD |
| `find` route selector (host overhead, runner mocked) | TBD | TBD | TBD |
| `wait_for` first-hit settle (host overhead, runner mocked) | TBD | TBD | TBD |

## Memory

Driver itself is allocation-light — most cost is downstream: `tree()`
clones the runner response and `find_all` collects matching `&A11yNode`
references into a `Vec<A11yNode>`. Per-find footprint scales with the
size of the matched node sub-tree (typically < 10 KB for a non-anchor
selector that hits a leaf cell).

## When to re-measure

- Touching the `tap` / `find` poll loop cadence or timeout constants.
- Touching `can_use_find_route` (changes which selectors hit the runner
  `/find` fast path vs the full-tree fallback).
- After upgrading the `smix-runner-client` dependency.


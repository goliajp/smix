# smix-simctl performance budgets

`smix-simctl` is **IO-bound**. Every public method spawns
`xcrun simctl <subcommand>` as a child process and awaits its exit.
Process-spawn latency on macOS dominates (10-100 ms typical, dwarfing
any inline cost), so there is no useful inline budget to track here.

Protocol stones ship dense in-house `perf_gate.rs` ceilings; IO-bound
stones use integration tests + real-world latency baselines kept in
`docs/PERFORMANCE.md`.

## Per-call wall-clock baseline (host: macOS 26.x, M-series, dev laptop)

These are **observed** medians, not enforced ceilings — `xcrun simctl`
latency varies with system load, simulator state, and Xcode version.

| Operation | Observed median | Notes |
|---|---:|---|
| `list_runtimes` | ~50 ms | JSON parse trivial; spawn + JSON output dominates |
| `list_devices` | ~80 ms | JSON output ~50 KB on a typical multi-runtime host |
| `boot` (cached) | ~150 ms | Idempotent; no-op on already-booted device |
| `boot_and_wait` (cold) | ~5-15 s | First boot of a runtime, then SpringBoard |
| `shutdown` | ~200 ms | |
| `screenshot` | ~250 ms | PNG ~500 KB-1 MB at native scale |
| `launch` | ~200 ms | Parses `bundle: pid` from stdout |
| `install` | ~2-5 s | Depends on `.app` bundle size |
| `terminate` | ~100 ms | |

## Memory

- All public methods allocate at minimum: a `Command` builder, an
  `Output` buffer for stdout / stderr, and the parsed result. For
  `screenshot` the PNG bytes buffer is the dominant allocation
  (~1 MB transient).
- `list_devices` builds a `HashMap<String, Vec<SimctlDevice>>` and
  flattens to `Vec<SimctlDevice>`, dropped at function exit.

## Methodology

No `tests/perf_gate.rs` for inline-able paths — see also c5 (TEST
augmentation) for integration tests that exercise the real `xcrun
simctl` binary end-to-end.

## When to re-measure

- After macOS / Xcode major-version updates (`xcrun simctl` itself can
  shift baseline latencies).
- After adding new public methods.
- After CI runner class change (CI is typically slower than dev laptop).

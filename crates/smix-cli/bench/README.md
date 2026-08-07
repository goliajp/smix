# smix bench baseline

`baseline.json` is the perf-regression reference for `smix bench`. It holds
median nanoseconds per in-process corpus metric, captured by
`smix bench --update-baseline`.

**Provenance matters.** These are absolute times from the machine that last
ran `--update-baseline` (studio, Apple M-series). Comparing them at 5%
tolerance on a *different* machine will false-positive, because CPUs differ by
more than 5%. So:

- On a given machine, `smix bench` catches slow drift between commits — its
  intended use.
- Cross-machine CI needs its own baseline (re-run `--update-baseline` on the
  nightly host) **or** ratio-normalisation so the metrics are machine-
  independent. That normalisation is the tracked open piece for this gate,
  and until it lands, do not
  wire `smix bench` (no args) into cross-machine CI — the fixture-driven
  `bench_gate.rs` test is the CI-safe part.

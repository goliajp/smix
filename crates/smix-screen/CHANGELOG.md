# Changelog

All notable changes to `smix-screen` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- `Rect` / `Bounds` geometric types with intersection arithmetic.
- `Role` enum with 29 a11y role variants matching the runner's
  schema (camelCase serde).
- `A11yNode` tree type — 13 fields, camelCase serde, used by every
  `/tree` and `/find` runner response.
- `ElementSummary` + `ScreenDescription` AI-readable failure context
  shapes (used by `ExpectationFailure::to_prompt()` in
  [`smix-error`](https://crates.io/crates/smix-error)).
- `is_visible_enough` / `visible_area` predicate primitives.
- `collect_visible_summaries` + `summarize_node` cold-path helpers.
- `tests/perf_gate.rs` regression budgets for the six hot paths.
- `benches/visibility.rs` criterion baseline.
- Fuzz target `fuzz/fuzz_targets/a11y_node_parse.rs`.

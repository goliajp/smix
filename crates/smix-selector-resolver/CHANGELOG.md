# Changelog

All notable changes to `smix-selector-resolver` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- `resolve_selector` (returns the first matched node) and
  `resolve_selector_all` (returns every matched node) end-to-end
  pipeline: DFS pre-order → visibility filter → spatial filter
  (six recursive anchor keys, AND semantics) → index pick.
- `ResolverContext` per-call regex compile cache (one
  `HashMap<*const Pattern, CompiledPattern>` per `resolve_selector`
  call; amortizes compile across the entire candidate walk).
- Visibility filter "drop nothing when no candidate is visible"
  preservation so failure-message paths still get a meaningful
  nearest-miss.
- Anchor null short-circuit: a spatial anchor sub-selector that
  resolves to nothing forces the overall result to `None`.
- `tests/perf_gate.rs` regression budgets for the seven hot paths.
- `benches/resolver.rs` criterion baseline.

# Changelog

All notable changes to `smix-host-coord-resolver` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- `resolve_to_norm_coord(tree, selector) -> Result<(f64, f64), HostResolveError>`
  pure pipeline — no async, no I/O, no injector awareness.
- `HostResolveError` with four discriminated variants:
  `NotFound` / `EmptyMatchedFrame` / `UnknownAppFrame` /
  `CentroidOutOfFrame { nx, ny }`.
- Consumed by [`smix-driver`](https://crates.io/crates/smix-driver)
  for the `tap` implementation (which adds implicit wait + retry
  semantics on top).

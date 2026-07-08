# Changelog

All notable changes to `smix-recorder` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- `RecordSession`: `Mutex<Vec<IRAction>>` buffer with `record` /
  `snapshot` / `stop` / `len` / `is_empty`.
- `RecordingApp<'a>`: thin wrapper over `&'a smix_sdk::App` that
  delegates every side-effecting SDK method (tap / fill / clear /
  press_key / swipe / go_back / wait_for / hide_keyboard) and pushes
  the matching `IRAction` into the session buffer.
- `generate_rust`: emits a `#[tokio::test]`-style Rust source file
  targeting the `smix-sdk` public surface for the captured session.
- `generate_maestro_yaml`: emits a maestro-compatible yaml flow for
  cross-tool playback. Uses workspace `serde_norway` (RUSTSEC-2025-0068
  maintained fork) for YAML emission.
- `cleanup::cleanup`: optional `claude` CLI shell-out for constrained
  AI post-processing (naming, `wait_for` injection between phases).
  Callers fall back to the raw output on cleanup failure.

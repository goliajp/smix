# smix-recorder

smix-recorder — host-side recorder for smix test sessions
(`RecordSession` + `RecordingApp` + Rust / maestro yaml generators +
`claude` CLI cleanup).

Part of the [smix](https://github.com/goliajp/smix) workspace. See the
top-level [README.md](../../README.md) for the project overview.

## Layout

- `session::{RecordSession, RecordingApp}` — the host-side capture
  surface a user wires between their test code and the underlying
  `smix_sdk::App`.
- `generator_rust::generate_rust` — emit a Rust `#[tokio::test]`-style
  source file targeting the smix-sdk public surface.
- `generator_maestro_yaml::generate_maestro_yaml` — emit a
  maestro-compatible yaml flow (cross-tool playback).
- `cleanup::cleanup` — optional `claude` CLI shell-out for constrained
  AI cleanup (naming, `wait_for` injection between phases). Callers
  fall back to the raw output on cleanup failure.

## License

Apache-2.0 OR MIT.

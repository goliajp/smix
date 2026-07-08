# Changelog

All notable changes to `smix-runner-wire` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- Pure serde wire request/response types for the 18-endpoint
  SmixRunnerCore HTTP IPC surface.
- Common types: `RunnerIncludeOpts` / `IncludeScope`.
- `/tap` family: `TapMode` / `TapStages` / `TapResult` / `TapRequest` /
  `TapAtNormCoordRequest`.
- Keyboard family: `KeyboardStages` / `RunnerKeyboardResult`.
- `/find` family: `FindRequest` / `FindResponse`.
- `/scroll` family: `RunnerScrollSelector` / `ScrollResponse`.
- `/system-popups` family: `SystemPopup` / `SystemPopupButton` /
  `SystemPopupsResponse`.
- `/record/*` family: `RecordedEvent` / `RecordEventsResponse`.
- `RunnerTransportErrorKind` discriminator enum (concrete `reqwest`
  source lives in [`smix-runner-client`](https://crates.io/crates/smix-runner-client)).
- camelCase wire compatibility with the SmixRunnerCore contract.
- Zero HTTP / async / I/O / schemars dependencies — `schemars` is
  intentionally excluded to keep the dep surface minimal; downstream
  consumers needing JSON Schema can derive on their own newtype.

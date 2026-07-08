# Changelog

All notable changes to `smix-error` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- `FailureCode` enum with 9 SCREAMING_SNAKE_CASE variants:
  `ELEMENT_NOT_FOUND` / `ELEMENT_NOT_ENABLED` / `WRONG_TEXT` /
  `EXPECTED_HIDDEN_BUT_VISIBLE` / `TIMEOUT` / `DRIVER_ERROR` /
  `RUNNER_TRANSPORT` / `INVALID_SELECTOR` / `RECORDER_FAILED`.
- `ExpectationFailure` rich AI-readable failure struct: message +
  failure code + visible-element summary list + suggestions + hint +
  optional device log tail.
- `to_prompt()` renderer: emits a self-contained block a coding agent
  can paste verbatim into a follow-up prompt to a teammate or LLM.
- `build_suggestions` + `similarity` + `edit_distance` cold-path
  helpers used to rank near-miss candidates by edit distance.
- `False` newtype (zero-sized type for serde-renders-as-`false` shape).
- `FailureInit` builder pattern for `ExpectationFailure` construction.
- `thiserror`-derived `Display` + `Error` impls.

# Changelog

All notable changes to `smix-recorder-ir` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- `IRAction` enum with 8 variants — `Tap` / `Fill` / `Clear` /
  `PressKey` / `Swipe` / `GoBack` / `WaitFor` / `HideKeyboard`. All
  variants carry `timestamp_ms: f64`. Wire shape: `tag = "kind"`,
  camelCase.
- `IRAction::kind()` + `IRAction::timestamp_ms()` stable accessors
  across variants.
- `RecorderErrorReason` kebab-case-serialized enum with 5 variants:
  `EmptySession` / `MalformedAction` / `CleanupFailed` /
  `CleanupEmptyOutput` / `CleanupInvalidOutput`.
- `RecorderError` struct with reason + message + `Display` + `Error`
  impls.
- `sort_by_timestamp` ascending merge helper (idempotent).
- Fuzz target `fuzz/fuzz_targets/ir_action_parse.rs`.

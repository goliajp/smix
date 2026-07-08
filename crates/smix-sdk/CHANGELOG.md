# Changelog

All notable changes to `smix-sdk` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- `App`: user-facing handle wrapping `SimctlDriver` + `SimctlClient` +
  `HttpRunnerClient`. Methods: `connect_to_runner` / `launch` /
  `terminate` / `tap` / `fill` / `clear` / `press_key` / `swipe` /
  `scroll_until_visible` / `go_back` / `wait_for` / `find` / `find_one`
  / `find_all` / `tree` / `describe` / `system_popups`.
- Ergonomic selector helpers: `text` / `text_regex` / `id` / `label` /
  `role` / `role_named` / `focused` / `anchor_box`.
- Re-exports across the dependency wall: `Selector` / `Modifiers` /
  `Pattern` / `A11yNode` / `Rect` / `Role` / `KeyName` /
  `SwipeDirection` / `ExpectationFailure` / `FailureCode` /
  `IncludeScope` / `SimctlClient` / `SimctlError` /
  `SimctlPermission` / `Appearance` / `LaunchResult`.

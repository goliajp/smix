# Changelog

All notable changes to `smix-driver` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release. `SimctlDriver` wraps `HttpRunnerClient` with
host-side resolve dispatch (SDK call → `driver.tap` → `tree()` →
`resolve_selector()` → centroid → `runner.tap_at_norm_coord()`).

- Sense methods: `tree` / `describe` / `find_one` / `find_all` /
  `find` (with `/find` runner route fast-path dispatch via
  `can_use_find_route`) / `system_popups`.
- Act methods: `tap` (5 s implicit-wait + retry, 250 ms poll cadence) /
  `fill` / `clear` / `press_key` / `swipe` / `scroll_until_visible` /
  `go_back` / `wait_for`.
- All methods return `Result<T, ExpectationFailure>` with the
  AI-readable rendering from `smix-error`.

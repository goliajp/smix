# Changelog

All notable changes to `smix-selector` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- `Selector` enum with 6 base forms: `Text` / `Id` / `Label` /
  `Role { role, name? }` / `Focused` / `Anchor`.
- `Pattern` enum with string-or-regex JSON wire form
  (untagged string ↔ `Pattern::Text`, tagged `{regex, flags}` ↔
  `Pattern::Regex`).
- `CompiledPattern` runtime cache shape exposed as a separate type
  so callers can amortize the ~16 µs regex compile across many
  candidates.
- `Modifiers` (6 spatial keys + 3 index keys) + `IndexModifiers`
  for anchor-only base + `AnchorBox` for recursive nested anchors.
- `match_text` (SDK convenience, compiles on every call) +
  `match_text_compiled` (hot-loop API, pre-compiled).
- `describe_selector` cold-path renderer used in failure messages.
- `True` newtype (zero-cost zero-sized type for `focused: true` /
  `anchor.inside: true` JSON wire shape).
- `tests/perf_gate.rs` regression budgets for the nine hot paths.
- `benches/matchtext.rs` criterion baseline.
- Fuzz targets `selector_parse` + `pattern_compile`.

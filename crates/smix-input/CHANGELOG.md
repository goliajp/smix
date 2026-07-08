# Changelog

All notable changes to `smix-input` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- `SwipeDirection` enum: `Up` / `Down` / `Left` / `Right`. camelCase
  serde wire compatibility with the SmixRunnerCore HTTP wire.
- `KeyName` enum: `Return` / `Delete` / `Tab` / `Space` / `Escape`
  + four arrow keys (`ArrowUp` / `ArrowDown` / `ArrowLeft` /
  `ArrowRight`).
- `as_str()` accessor + `Display` impl on both enums.
- Zero dependencies beyond serde.

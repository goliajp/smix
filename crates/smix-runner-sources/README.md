# smix-runner-sources

Embedded Swift runner project sources for the smix CLI.

Solves a distribution gap: `cargo install smix` ships only the Rust
binary, but the smix runner is an XCTest bundle that needs
`SmixRunner.xcodeproj` + Swift sources on disk to compile. Before this
crate existed, consumers acquired the runner project out-of-band, and
`smix runner up` never verified the on-disk sources matched the
installed CLI version — so CLI upgrades that added new HTTP routes,
handler types, or wire fields silently no-op'd until the consumer
manually re-copied the runner project (which most never did).

## Contents

`data/swift-runner-sources.tar.gz` is a gzipped tar of the smix runner
project as of the crate's version:

- `Package.swift`, `project.yml`
- `SmixRunner/` — bootstrap target
- `SmixRunnerUITests/` — test bundle that runs `test_runForever`
- `Sources/SmixRunnerCore/` — HTTP server, session handlers, routes
- `Tests/SmixRunnerCoreTests/` — Swift unit tests
- `SmixRunner.xcodeproj/` — Xcode project + shared scheme

Excluded: `SmixCoreFFI.xcframework/` (13.3 MB binary — fetched
separately at extract time from GitHub Release pinned by version).

## API

```rust
use smix_runner_sources::{ensure_tree, RunnerPlatform};
use std::path::Path;

// The tree for these sources under a machine directory — one per set of
// sources, `<root>/runner-sources/ios/<version>-<digest>/`, so two smix
// builds on one machine never replace each other's (called from
// smix-capsule when a runner comes up).
let ensured = ensure_tree(Path::new("/home/me/.local/share/smix"), RunnerPlatform::Ios)?;
println!("{} (extracted now: {})", ensured.dir.display(), ensured.extracted);
```

`extract_to(dir, force)` remains for an explicit destination
(`smix runner install --path <dir>`): that one directory is replaced
whole and its previous tree kept beside it.

## Regenerating the tarball

The tarball is checked in and MUST be regenerated whenever anything
under `swift-bridge/` changes:

```bash
scripts/release/build-runner-tarball.sh
```

The pre-publish CI gate refuses to publish if the checked-in tarball
does not match the current `swift-bridge/` state.

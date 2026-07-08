# ABI stability — smix v1.0.0

> The following crates expose smix's public, business-agnostic
> primitives. Their public API is frozen at v1.0.0; breaking
> changes require a major bump.

## Frozen crates

| crate | primary types | wire? | Notes |
|---|---|---|---|
| `smix-error` | `ExpectationFailure`, `FailureCode`, `FailureInit` | yes (RunReport JSON) | AI-readable failure surface |
| `smix-selector` | `Selector`, `Modifiers`, `Anchor` | yes (all routes) | Central selector algebra |
| `smix-screen` | `A11yNode`, `Rect`, `Bounds`, `Role`, `ElementSummary` | yes (`/tree` route) | A11y tree wire shape |
| `smix-runner-wire` | HTTP schemas | yes | Wire schema types |
| `smix-input` | `SwipeDirection`, `KeyName` | yes (input routes) | Input primitives |
| `smix-verbs` | `VerbEntry`, `VerbCategory`, `ArgShape`, `VERB_TABLE` | yes (indirectly via parser + migrate) | Canonical verb table |
| `smix-metro-log` | `MetroLogTail`, `SignalMatcher`, `Window`, `AwaitError` | no | Signal await surface |
| `smix-fixture` | `FixtureRegistry`, `FixtureDecl`, `SignalMatcher` | no | Fixture registry |
| `smix-annotate` | `Annotator`, `Annotation`, `Color`, `Position`, `Compression` | no | Annotation primitives |
| `smix-migrate` | `Migrator`, `MigrateReport`, `MigrateError`, `Rename` | no | YAML codemod |

## Non-frozen crates

- `smix-driver` — internal driver dispatch; can add methods
- `smix-sdk` — SDK app builder; can add builder methods
- `smix-adapter-maestro` — adapter runtime; can add `Step` variants
- `smix-cli` — command surface; can add subcommands + flags
- `smix-simctl` — `simctl` wrapper; can add commands

## ABI compatibility rules

The following changes are **compatible** within v1.x:

- Adding new methods to public traits (with default impls)
- Adding new variants to `#[non_exhaustive]` enums
- Adding new fields to `#[non_exhaustive]` structs
- Adding new items to modules
- Widening trait bounds on generic methods
- Adding new re-exports

The following are **breaking** (require v2.0):

- Removing or renaming any public item
- Adding new required trait methods
- Adding new required struct fields
- Narrowing trait bounds
- Changing existing method signatures

## Semver verification

Use `cargo-semver-checks` to gate CI on the frozen crates' ABI:

```bash
for c in smix-error smix-selector smix-screen smix-runner-wire \
         smix-input smix-verbs smix-metro-log smix-fixture \
         smix-annotate smix-migrate; do
    cargo semver-checks check-release --package $c
done
```

Any breaking change surfaced in CI blocks a v1.x release; it must bump to v2.0.

## `#[non_exhaustive]` enums

All public enums are marked `#[non_exhaustive]` at the v1.0 freeze:

```rust
#[non_exhaustive]
pub enum ExpectationFailureCode { ... }

#[non_exhaustive]
pub enum Selector { ... }

// ...
```

This allows adding new variants in v1.x without breaking downstream
consumers who exhaustively match.

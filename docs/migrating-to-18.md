# Migrating to smix 18.0.0

18.0.0 is a major release because two public Rust structs became
`#[non_exhaustive]` and gained a field. Flows and the CLI need no change to
keep working: a `pinch` without `runUp:` does what it did in 17.0.0.

If you only write YAML or drive smix from the CLI, read **What answers
differently** and stop. **Rust API** is for code that calls the crates.

---

## What answers differently

- **Android: a View an app lays over its Compose screen is in the tree** —
  `smix tree`, `find` and every selector see it, where before only the
  accessibility dump did. This needs the app built against this release's
  probe: raise `debugImplementation("jp.golia.smix:smix-probe:19.0.0")` and
  rebuild. A flow that read such a View with `ocrText:` to get around it can
  name it by text or label again.

## Rust API

- `smix_input::Pinch` is `#[non_exhaustive]` and has a new field,
  `run_up: RunUp`. Build one with
  `Pinch::new(centre, from_span, to_span)` and `.with_duration_ms(…)`,
  `.with_pan(…)`, `.with_run_up(…)`; reading its fields is unchanged.
- `smix_input::Pan` is `#[non_exhaustive]`; build one with
  `Pan::new(dx, dy, ms)`.
- New in `smix_input`: `RunUp` (`Spread`, `Slide`, `None`;
  `#[non_exhaustive]`), with `as_wire` and `from_name`.
- `smix_adapter_maestro::Step::Pinch` has a new field, `run_up`. A pattern
  that names every field adds it, or ends in `..`.

## Runners

`POST /pinch-at-norm-coord` reads an optional `runUp` (`spread`, `slide`,
`none`; `spread` when absent). A 17.0.0 runner ignores it and spreads; bring
the runner up from 18.0.0 for `slide` and `none`.

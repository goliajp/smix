# smix-migrate

Static [maestro](https://maestro.mobile.dev) YAML flow → [smix](https://github.com/goliajp/smix) canonical YAML codemod.

**Not required for correctness** — smix already accepts maestro yaml directly. This crate exists to normalize corpora: canonical verb names for grep, smix-native argument shapes for tooling, deprecated-form flagging for cleanup.

## CLI (via `smix migrate`)

```bash
# stdin/stdout
cat flow.yaml | smix migrate > flow.smix.yaml

# single file, print to stdout
smix migrate flow.yaml

# in-place batch
smix migrate --in-place flows/*.yaml
```

Any unrecognized verb (`runScript`, `evalScript`, etc.) is preserved verbatim with a `WARN:` line to stderr.

## Library

```rust
use smix_migrate::Migrator;

let (canonical_yaml, report) = Migrator::default().migrate(maestro_yaml)?;
println!("{} renames, {} unknown verbs", report.renamed.len(), report.unknown_verbs.len());
```

## Rules

See `DEFAULT_RULES` in `src/lib.rs` for the current 20-rule table.

## Non-goals

- Comment preservation (serde_norway loses `#` comments; documented at CLI)
- Linting / correctness checks
- Formatting (indentation follows serde_norway defaults)

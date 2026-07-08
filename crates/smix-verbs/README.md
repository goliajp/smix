# smix-verbs

Canonical verb table for the [smix](https://github.com/goliajp/smix) yaml automation surface. Pure data crate — no runtime state, no async, no I/O.

Shared source of truth for:
- `smix-adapter-maestro::parser` — accepts both maestro-canonical (`tapOn`) and smix-canonical (`tap`) forms
- `smix-migrate` codemod — verb rename table

## Usage

```rust
use smix_verbs::{VERB_TABLE, find_by_maestro, find_by_smix, is_known_verb};

// Given a yaml key, look up the entry.
if let Some(entry) = find_by_maestro("tapOn") {
    println!("maps to smix name: {}", entry.smix_name); // "tap"
}

// Both canonical forms recognized.
assert!(is_known_verb("tapOn"));
assert!(is_known_verb("tap"));
```

## Reviewer invariant

Any new yaml verb must land in `VERB_TABLE` first. Both parser and codemod pick it up automatically.

See `docs/ai-guide/verb-parity.md` in the smix repo for the cross-platform support matrix generated from this table.

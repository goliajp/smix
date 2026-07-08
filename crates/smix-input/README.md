# smix-input

[![Crates.io](https://img.shields.io/crates/v/smix-input?style=flat-square&logo=rust)](https://crates.io/crates/smix-input)
[![docs.rs](https://img.shields.io/docsrs/smix-input?style=flat-square&logo=docs.rs)](https://docs.rs/smix-input)
[![License](https://img.shields.io/crates/l/smix-input?style=flat-square)](#license)

Tiny `SwipeDirection` + `KeyName` enums shared across every layer that
talks about input — driver, runner-wire, recorder-ir, MCP tool surface.
Standalone stone so cement crates (CLI / MCP / SDK / recorder) can take
the wire enums without dragging in the full driver dep graph.

Both enums serialize as camelCase strings, matching the SmixRunnerCore
wire.

## Quickstart

```rust
use smix_input::{KeyName, SwipeDirection};

assert_eq!(SwipeDirection::Up.as_str(), "up");
assert_eq!(KeyName::ArrowDown.as_str(), "arrowDown");

let body = serde_json::to_string(&KeyName::Return).unwrap();
assert_eq!(body, "\"return\"");

let parsed: SwipeDirection = serde_json::from_str("\"left\"").unwrap();
assert_eq!(parsed, SwipeDirection::Left);
```

## Variants

| Enum | Variants |
|---|---|
| `SwipeDirection` | `Up` / `Down` / `Left` / `Right` |
| `KeyName` | `Return` / `Delete` / `Tab` / `Space` / `Escape` / `ArrowUp` / `ArrowDown` / `ArrowLeft` / `ArrowRight` |

`KeyName` is a deliberate **subset** — only the keys SDK users
reliably exercise on iOS sim. Expanding the set is a checkpoint-level
discussion, not a casual add. Arrow keys are in because focus-traversal
flows need them.

## When to reach for this

| Use case | Pick |
|---|---|
| Building anything that talks to SmixRunnerCore wire | **smix-input** |
| Need richer key set (function keys, modifiers) | open an issue — extension is a deliberate checkpoint decision |
| Want raw HID key codes | out of scope (use a HID crate directly) |

## Scope

- ✅ Two enums + `as_str()` + `Display`
- ✅ camelCase serde wire compatibility
- ✅ Zero deps beyond serde
- ❌ No driver/runner I/O (use [`smix-driver`](https://crates.io/crates/smix-driver) or [`smix-runner-client`](https://crates.io/crates/smix-runner-client))

## License

Dual-licensed under either:

- [Apache License 2.0](../../LICENSE-APACHE)
- [MIT License](../../LICENSE-MIT)

at your option.

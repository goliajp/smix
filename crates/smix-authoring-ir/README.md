# smix-authoring-ir

[![Crates.io](https://img.shields.io/crates/v/smix-authoring-ir?style=flat-square&logo=rust)](https://crates.io/crates/smix-authoring-ir)
[![docs.rs](https://img.shields.io/docsrs/smix-authoring-ir?style=flat-square&logo=docs.rs)](https://docs.rs/smix-authoring-ir)
[![License](https://img.shields.io/crates/l/smix-authoring-ir?style=flat-square)](#license)

Intermediate representation for recorded iOS-sim sessions: 8-variant
`IRAction` enum, `RecorderError` with kebab-case reason codes, and a
`sort_by_timestamp` merge helper. Stone — pure types + serde, no
recording I/O, no code generation. Pairs with
[`smix-recorder`](https://crates.io/crates/smix-recorder) (cement) which
fills it in and emits Rust / Maestro YAML.

## Quickstart

```rust
use smix_authoring_ir::{IRAction, sort_by_timestamp};
use smix_selector::{Pattern, Selector, Modifiers};

let actions = vec![
    IRAction::Tap {
        selector: Selector::Text { text: Pattern::text("Login"), modifiers: Modifiers::default() },
        timestamp_ms: 1716700000.0,
    },
    IRAction::Fill {
        selector: Selector::Id { id: "username".into(), modifiers: Modifiers::default() },
        text: "alice".into(),
        timestamp_ms: 1716700005.0,
    },
];

assert_eq!(actions[0].kind(), "tap");
assert_eq!(actions[1].timestamp_ms(), 1716700005.0);

let json = serde_json::to_string(&actions[0]).unwrap();
assert!(json.contains("\"kind\":\"tap\""));
assert!(json.contains("\"timestampMs\":1716700000"));

let merged = sort_by_timestamp(&actions);
assert_eq!(merged.len(), 2);
```

## IRAction variants

| Variant | Wire `kind` | Carries |
|---|---|---|
| `Tap` | `"tap"` | selector |
| `Fill` | `"fill"` | selector, text |
| `Clear` | `"clear"` | selector |
| `PressKey` | `"pressKey"` | key |
| `Swipe` | `"swipe"` | direction, optional from-selector |
| `GoBack` | `"goBack"` | — |
| `WaitFor` | `"waitFor"` | selector |
| `HideKeyboard` | `"hideKeyboard"` | — |

All variants carry `timestamp_ms: f64`. Wire shape uses `tag = "kind"`,
camelCase.

## RecorderErrorReason

| Variant | Wire string |
|---|---|
| `EmptySession` | `"empty-session"` |
| `MalformedAction` | `"malformed-action"` |
| `CleanupFailed` | `"cleanup-failed"` |
| `CleanupEmptyOutput` | `"cleanup-empty-output"` |
| `CleanupInvalidOutput` | `"cleanup-invalid-output"` |

The three `cleanup-*` variants surface AI-cleanup failures distinctly
from recording-time failures — CLI / SDK consumers can map "the AI
couldn't clean this trace" separately from "the recorder itself blew up".

## Capture-side note

IR is captured from the **smix API channel** (host-side instrumentation
in `RecordSession`), not from sim-side AX notification swizzle. The
swizzle path was investigated and confirmed unable to surface user-tap
events from outside the smix API channel — "user taps sim screen
manually with smix watching" is a separate architecture and is not
currently supported.

## When to reach for this

| Use case | Pick |
|---|---|
| Want types only (build your own recorder/generator) | **smix-authoring-ir** |
| Want a working recorder + Rust/YAML code generation | [`smix-recorder`](https://crates.io/crates/smix-recorder) (consumes this) |
| Want UI test scaffolding from scratch (no IR) | use [`smix-sdk`](https://crates.io/crates/smix-sdk) directly |

## Scope

- ✅ 8 IR variants + RecorderErrorReason + sort helper
- ✅ Pure serde (camelCase tag, kebab-case error reasons)
- ✅ Stable `timestamp_ms()` / `kind()` accessors
- ❌ No recording I/O (use [`smix-recorder`](https://crates.io/crates/smix-recorder))
- ❌ No code generation (use [`smix-recorder`](https://crates.io/crates/smix-recorder))

## License

Dual-licensed under either:

- [Apache License 2.0](../../LICENSE-APACHE)
- [MIT License](../../LICENSE-MIT)

at your option.

# smix-error

[![Crates.io](https://img.shields.io/crates/v/smix-error?style=flat-square&logo=rust)](https://crates.io/crates/smix-error)
[![docs.rs](https://img.shields.io/docsrs/smix-error?style=flat-square&logo=docs.rs)](https://docs.rs/smix-error)
[![License](https://img.shields.io/crates/l/smix-error?style=flat-square)](#license)

AI-readable structured failure type for iOS-Simulator automation:
`ExpectationFailure` carries the selector that failed, the top-10
visible elements at the failure moment, Levenshtein-based
"Did you mean …?" suggestions, an optional hint, and an optional
device-log tail — and renders all of it via `to_prompt()` as a
multi-line block a coding agent can paste back as a user message and
act on without further context.

This crate exists because every iOS-automation library has its own
flavor of `assertion failed`, and most of them throw away the failure
context that a downstream LLM needs to fix the test. `smix-error` is
designed so the failure object itself can be the prompt.

## Quickstart

```rust
use smix_error::{ExpectationFailure, FailureCode, FailureInit, build_suggestions};
use smix_screen::{ElementSummary, Rect, Role};

let visible_elements = vec![
    ElementSummary {
        role: Some(Role::Button), name: Some("Log in".into()),
        id: Some("btn-login".into()), text: None,
        bounds: Rect { x: 0.0, y: 0.0, w: 100.0, h: 40.0 }, enabled: true,
    },
    // ...
];

let suggestions = build_suggestions(Some("Login"), &visible_elements);

let failure = ExpectationFailure::new(FailureInit {
    code: Some(FailureCode::ElementNotFound),
    message: "element not found: { text: \"Login\" }".into(),
    visible_elements,
    suggestions,
    hint: Some(
        "matched 0 nodes in the current a11y tree; check selector or wait for the screen to settle".into(),
    ),
    ..Default::default()
});

println!("{}", failure.to_prompt());
// FAIL [ELEMENT_NOT_FOUND]: element not found: { text: "Login" }
//   suggestions:
//     - Did you mean "Log in"? (similarity 0.83, field name)
//   visible elements (top 1):
//     - button name="Log in" id="btn-login"
//   hint: matched 0 nodes in the current a11y tree; check selector or wait for the screen to settle
```

## Why "AI-readable" matters

When a test fails and an LLM is the test author, the failure object
arrives back at the LLM as a user message. If the failure says only
"ELEMENT_NOT_FOUND" the LLM has to send another tool call to fetch the
screen state — costing latency, tokens, and often the user's patience.

`ExpectationFailure::to_prompt()` packs *all* the context — selector
that failed, visible elements, suggestions from edit-distance matching,
optional hint, optional device-log tail — into one self-contained block.
Coding agents can iterate on it immediately.

## Components

| Symbol | Role |
|---|---|
| `FailureCode` | 9-variant enum: ElementNotFound / NotVisible / NotEnabled / Ambiguous / Timeout / AssertionFailed / AppNotRunning / SimulatorNotBooted / DriverError |
| `ExpectationFailure` | Structured failure with 8 fields, implements `Display` + `std::error::Error` |
| `FailureInit` | Ergonomic builder; default-derived |
| `False` | `ok: false` discriminator newtype (serde wire shape) |
| `to_prompt()` | AI-readable rendering — caps visible elements at 10, device log at 200 lines |
| `build_suggestions(target, visible)` | Top-3 "Did you mean …?" suggestions, Levenshtein-based, score > 0.5 |
| `similarity(a, b)` | `[0, 1]` normalized string similarity |
| `edit_distance(a, b)` | Wagner-Fischer Levenshtein distance |

## When to reach for this

| Use case | Pick |
|---|---|
| Need a failure type that LLM agents can act on directly | **smix-error** |
| Want "Did you mean …?" suggestions from a visible-element list | **smix-error** |
| Need a fast Levenshtein distance | `strsim` or **smix-error** (free with the rest) |
| Generic `std::error::Error` for a non-automation domain | use `thiserror` directly |

## Scope

- ✅ Implements `std::error::Error` + `Display` (Display = `to_prompt`)
- ✅ JSON wire shape: `{ok: false, code, message, selector, suggestions,
  visibleElements, hint, screenshot, deviceLog}`
- ✅ `result_large_err` clippy lint workspace-allowed (the rich context
  is deliberate; boxing the error would defeat the AI-readable design)
- ❌ No I/O, no async
- ❌ No assertions / matchers (use [`smix-sdk`](https://crates.io/crates/smix-sdk) for `assert_visible` / `assert_enabled` / etc.)

## License

Dual-licensed under either:

- [Apache License 2.0](../../LICENSE-APACHE)
- [MIT License](../../LICENSE-MIT)

at your option.

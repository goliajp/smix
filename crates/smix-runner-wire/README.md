# smix-runner-wire

[![Crates.io](https://img.shields.io/crates/v/smix-runner-wire?style=flat-square&logo=rust)](https://crates.io/crates/smix-runner-wire)
[![docs.rs](https://img.shields.io/docsrs/smix-runner-wire?style=flat-square&logo=docs.rs)](https://docs.rs/smix-runner-wire)
[![License](https://img.shields.io/crates/l/smix-runner-wire?style=flat-square)](#license)

Pure-`serde` wire request/response types for the SmixRunnerCore HTTP IPC
(18 endpoints). Zero HTTP / async / I/O dependencies — pairs with
[`smix-runner-client`](https://crates.io/crates/smix-runner-client) (the
reqwest+tokio client) but stands alone so consumers can drive their own
sync/custom transport or serve the same wire contract from a different
backend.

## Quickstart

```rust
use smix_runner_wire::{IncludeScope, RunnerIncludeOpts, TapMode, TapRequest, TapResult};
use smix_selector::{Pattern, Selector, Modifiers};

let req = TapRequest {
    selector: Selector::Text {
        text: Pattern::text("Login"),
        modifiers: Modifiers::default(),
    },
    mode: TapMode::Resolve,
};
let body = serde_json::to_string(&req).unwrap();
assert!(body.contains("\"mode\":\"resolve\""));

let inc = RunnerIncludeOpts { include: Some(IncludeScope::AllWindows) };
let q = inc.include.unwrap().query_value();
assert_eq!(q, "all-windows");

let example_resp = r#"{
  "stages": { "resolveMs": 12.3, "tapCallMs": 4.5, "totalMs": 17.1,
               "waitExistenceMs": 0.0, "frameReadMs": 0.7 },
  "frame": { "x": 50.0, "y": 100.0, "w": 200.0, "h": 40.0 },
  "appFrame": { "x": 0.0, "y": 0.0, "w": 390.0, "h": 844.0 }
}"#;
let parsed: TapResult = serde_json::from_str(example_resp).unwrap();
assert_eq!(parsed.frame.unwrap().w, 200.0);
```

## Route surface

18 endpoints. `Request` / `Response` types live in this crate; the
client method names live in [`smix-runner-client`](https://crates.io/crates/smix-runner-client):

| HTTP | Path | Request type | Response type |
|---|---|---|---|
| GET | `/health` | — | bare 200 |
| GET | `/tree?include=…` | [`RunnerIncludeOpts`] (query) | [`smix_screen::A11yNode`] |
| GET | `/system-popups?include=…` | [`RunnerIncludeOpts`] (query) | `Vec<`[`SystemPopup`]`>` |
| POST | `/tap` | [`TapRequest`] | [`TapResult`] |
| POST | `/tap-at-norm-coord` | [`TapAtNormCoordRequest`] | `{ok}` |
| POST | `/find` | [`FindRequest`] | [`FindResponse`] |
| POST | `/fill` | `{selector, text}` | [`RunnerKeyboardResult`] |
| POST | `/clear` | `{selector}` | [`RunnerKeyboardResult`] |
| POST | `/press-key` | `{key}` | [`RunnerKeyboardResult`] |
| POST | `/scroll` | `{selector,`[`RunnerScrollSelector`]`, direction}` | [`ScrollResponse`] |
| POST | `/swipe-once` | `{direction}` | `{ok}` |
| POST | `/foreground` | `{bundleId}` | `{ok}` |
| POST | `/hide-keyboard` | — | `{ok}` |
| POST | `/back` | — | `{ok}` |
| POST | `/record/start` | — | `{ok}` |
| GET | `/record/poll` | — | `{events:[`[`RecordedEvent`]`]}` |
| POST | `/record/stop` | — | `{events:[`[`RecordedEvent`]`]}` |

All `camelCase` on the wire; Rust-side fields use snake_case via
`#[serde(rename_all = "camelCase")]` or explicit `#[serde(rename)]`.

## When to reach for this

| Use case | Pick |
|---|---|
| Want types only, bring your own HTTP / transport | **smix-runner-wire** |
| Want a working async HTTP client | [`smix-runner-client`](https://crates.io/crates/smix-runner-client) (consumes this) |
| Serving the wire contract from an alternate runner implementation | **smix-runner-wire** (single source of truth for shapes) |
| Need JSON Schema export (e.g. for OpenAPI) | excluded by design — derive separately on the consumer side if needed |

## Scope

- ✅ Pure serde, no async, no HTTP, no schemars
- ✅ camelCase wire compatibility with the SmixRunnerCore contract
- ✅ `RunnerTransportErrorKind` discriminator enum for non-HTTP transports
- ❌ No client implementation (use [`smix-runner-client`](https://crates.io/crates/smix-runner-client))
- ❌ No server implementation (runner side is Swift, lives outside this workspace)

## License

Dual-licensed under either:

- [Apache License 2.0](../../LICENSE-APACHE)
- [MIT License](../../LICENSE-MIT)

at your option.

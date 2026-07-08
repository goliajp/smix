# smix-core

[![Crates.io](https://img.shields.io/crates/v/smix-core?style=flat-square&logo=rust)](https://crates.io/crates/smix-core)
[![docs.rs](https://img.shields.io/docsrs/smix-core?style=flat-square&logo=docs.rs)](https://docs.rs/smix-core)
[![License](https://img.shields.io/crates/l/smix-core?style=flat-square)](#license)

Workspace anchor + version sentinel for the smix stone graph. The
actual sense / decide / act primitives live in dedicated stones (see
table below) so consumers pay only for what they pull in. This crate
itself stays intentionally tiny — its job is to mark the workspace
root for `cargo install smix-core`-style discovery flows and to expose
`CARGO_PKG_VERSION` as a smoke-test compile target.

If you arrived here looking for actual functionality, jump straight to
the relevant stone:

| You want… | Crate |
|---|---|
| A11y tree types + visibility filter | [`smix-screen`](https://crates.io/crates/smix-screen) |
| Selector enum + match primitives | [`smix-selector`](https://crates.io/crates/smix-selector) |
| Selector → matched node resolver | [`smix-selector-resolver`](https://crates.io/crates/smix-selector-resolver) |
| Selector → normalized `(nx, ny)` coord | [`smix-host-coord-resolver`](https://crates.io/crates/smix-host-coord-resolver) |
| `xcrun simctl` wrapper | [`smix-simctl`](https://crates.io/crates/smix-simctl) |
| Input enums (`KeyName` / `SwipeDirection`) | [`smix-input`](https://crates.io/crates/smix-input) |
| AI-readable `ExpectationFailure` | [`smix-error`](https://crates.io/crates/smix-error) |
| Recorder IR (`IRAction` enum) | [`smix-recorder-ir`](https://crates.io/crates/smix-recorder-ir) |
| Runner HTTP wire types | [`smix-runner-wire`](https://crates.io/crates/smix-runner-wire) |
| Runner HTTP async client | [`smix-runner-client`](https://crates.io/crates/smix-runner-client) (cement) |
| Full driver with retry / implicit wait | [`smix-driver`](https://crates.io/crates/smix-driver) (cement) |
| High-level App SDK | [`smix-sdk`](https://crates.io/crates/smix-sdk) (cement) |

## License

Dual-licensed under either:

- [Apache License 2.0](../../LICENSE-APACHE)
- [MIT License](../../LICENSE-MIT)

at your option.

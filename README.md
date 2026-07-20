# smix

[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue?style=flat-square)](#license)
[![Rust](https://img.shields.io/badge/rust-stable-orange?style=flat-square&logo=rust)](#)
[![Swift Package](https://img.shields.io/github/v/release/goliajp/smix?label=Swift%20Package&logo=swift&style=flat-square)](https://github.com/goliajp/smix/releases/latest)
[![npm](https://img.shields.io/npm/v/@goliapkg/smix?label=npm%20%40goliapkg%2Fsmix&logo=npm&style=flat-square)](https://npmjs.com/package/@goliapkg/smix)
[![crates.io](https://img.shields.io/crates/v/smix-cli?label=crates.io%20smix-cli&logo=rust&style=flat-square)](https://crates.io/crates/smix-cli)
[![Maven Central](https://img.shields.io/maven-central/v/jp.golia.smix/smix-sdk?label=Maven%20Central%20jp.golia.smix&logo=apachemaven&style=flat-square)](https://central.sonatype.com/artifact/jp.golia.smix/smix-sdk)

AI-native UI automation for the iOS Simulator and Android Emulator. Written in Rust; distributed as a CLI (`smix`), a Rust SDK, and native SDKs for Swift and Kotlin. A TypeScript package ships the same typed API surface; its native driving transport is still in progress (driving calls throw `SmixNotImplementedError` today — see the [npm package README](./npm/smix-rn/README.md)).

Designed for LLM-authored test flows:

- Explicit selectors (`id` / `text` / `label` / `role` / `anchor`) — no XPath, no coordinate guessing
- AI-readable failure messages with visible-element context and edit-distance suggestions
- Playwright-shape API surface mirrored across the SDKs
- Host-side selector resolution + native event injection (IOHID + XCUIElement chain on iOS, UiAutomator on Android)
- MCP server entry for direct Claude Code integration
- Simulator / emulator only — real-device automation is out of scope

## Install

Pick the SDK that matches your test harness. All ship the same wire-level primitives.

```bash
# Rust CLI + SDK
cargo install smix-cli --locked

# Swift Package Manager
# add https://github.com/goliajp/smix (product: SmixSDK, from: "2.0.0")

# Gradle / Maven (Kotlin / Java)
# implementation("jp.golia.smix:smix-sdk:2.0.0")

# TypeScript / Node / Bun (typed surface; driving transport in progress)
npm install @goliapkg/smix
```

Prerequisites: macOS with Xcode + Simulator (iOS testing); Android SDK with an emulator image (Android testing).

## Quick start

Register a simulator under an alias, boot it, start the runner, run a YAML flow:

```bash
smix sim list                                  # find the UDID
smix sim register dev --udid <UDID>            # creates the .smix/ registry
smix sim boot dev
smix runner up dev --bundle com.example.app
smix run examples/hello.yaml --device dev
```

Equivalent Rust SDK:

```rust
use smix_sdk::{App, text, KeyName};
use std::time::Duration;

let app = App::connect_to_runner(22087).await?
    .with_bundle_id("com.example.app");
app.launch("com.example.app").await?;
app.wait_for(&text("Dashboard"), Duration::from_secs(5)).await?;
app.tap(&text("Sign In")).await?;
app.fill(&text("Email"), "user@example.com").await?;
app.press_key(KeyName::Return).await?;
app.assert_visible(&text("Dashboard")).await?;
```

The TypeScript package declares the same shape, but its driving methods
(`Smix.launchApp`, `App.tap`, `App.fill`, …) are not wired to a native
transport yet and throw `SmixNotImplementedError`. Use the Rust, Swift,
or Kotlin SDK (or the CLI) to drive a device today.

See [`docs/ai-guide/01-quickstart.md`](./docs/ai-guide/01-quickstart.md) for a full walkthrough.

## Documentation

| Topic | Location |
|---|---|
| Quickstart | [`docs/ai-guide/01-quickstart.md`](./docs/ai-guide/01-quickstart.md) |
| YAML reference | [`docs/ai-guide/02-yaml-reference.md`](./docs/ai-guide/02-yaml-reference.md) |
| Selectors | [`docs/ai-guide/03-selectors.md`](./docs/ai-guide/03-selectors.md) |
| Actions | [`docs/ai-guide/04-actions.md`](./docs/ai-guide/04-actions.md) |
| CLI reference | [`docs/ai-guide/05-cli.md`](./docs/ai-guide/05-cli.md) |
| Fixtures | [`docs/ai-guide/06-fixtures.md`](./docs/ai-guide/06-fixtures.md) |
| Errors | [`docs/ai-guide/07-errors.md`](./docs/ai-guide/07-errors.md) |
| Cookbook | [`docs/ai-guide/08-cookbook.md`](./docs/ai-guide/08-cookbook.md) |
| Wire format | [`docs/ai-guide/wire-format.md`](./docs/ai-guide/wire-format.md) |
| ABI stability | [`docs/ai-guide/abi-stability.md`](./docs/ai-guide/abi-stability.md) |
| Verb parity matrix | [`docs/ai-guide/verb-parity.md`](./docs/ai-guide/verb-parity.md) |
| App activation lifetime | [`docs/ai-guide/activate-header-lifetime.md`](./docs/ai-guide/activate-header-lifetime.md) |

## Stability commitments

`smix` follows semantic versioning at the wire, ABI, and CLI surface.

- **Wire format** — versioned by schema negotiation: the runner answers `GET /health` with the schema versions it speaks (currently `[1, 2]`), and each client negotiates the newest both ends share. Any change that would break a spoken schema adds a new version instead.
- **Stone crate ABI** — the core crates (`smix-error`, `smix-selector`, `smix-screen`, `smix-runner-wire`, `smix-input`, `smix-verbs`, `smix-metro-log`, `smix-fixture`, `smix-annotate`, `smix-migrate`) are ABI-stable within the 2.x series; additive changes only. `cargo-semver-checks` gates every release.
- **CLI surface** — `smix run` flags shipped in 2.0 remain accepted for the 2.x lifetime.
- **YAML verb parity** — the verb table is a single source of truth (`smix-verbs`). Removing a verb is a major-version change.

Compatibility matrix:

| Client | Runner | Compatible |
|---|---|---|
| v2.x | v2.x | yes (wire schema 2) |
| v1.x | v2.x | yes — negotiates down to wire schema 1 |
| v1.x YAML flows | v2.x | run `smix migrate` for the renamed verbs; unknown spellings warn and pass through |

## Contributions

**This project does not accept external contributions.** See [`CONTRIBUTING.md`](./CONTRIBUTING.md) for the full policy.

The source is published under Apache 2.0 / MIT so you are free to fork, adapt, and redistribute per those license terms; upstream will not accept pull requests, feature requests, or issue reports.

## License

Licensed under either of

- Apache License, Version 2.0 ([`LICENSE-APACHE`](./LICENSE-APACHE) or <https://www.apache.org/licenses/LICENSE-2.0>)
- MIT license ([`LICENSE-MIT`](./LICENSE-MIT) or <https://opensource.org/licenses/MIT>)

at your option.

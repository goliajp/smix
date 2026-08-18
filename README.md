# smix

[![License](https://img.shields.io/badge/license-Apache--2.0%20OR%20MIT-blue?style=flat-square)](#license)
[![Rust](https://img.shields.io/badge/rust-stable-orange?style=flat-square&logo=rust)](#)
[![Swift Package](https://img.shields.io/github/v/release/goliajp/smix?label=Swift%20Package&logo=swift&style=flat-square)](https://github.com/goliajp/smix/releases/latest)
[![npm](https://img.shields.io/npm/v/@goliapkg/smix?label=npm%20%40goliapkg%2Fsmix&logo=npm&style=flat-square)](https://npmjs.com/package/@goliapkg/smix)
[![crates.io](https://img.shields.io/crates/v/smix-cli?label=crates.io%20smix-cli&logo=rust&style=flat-square)](https://crates.io/crates/smix-cli)
[![Maven Central](https://img.shields.io/maven-central/v/jp.golia.smix/smix-sdk?label=Maven%20Central%20jp.golia.smix&logo=apachemaven&style=flat-square)](https://central.sonatype.com/artifact/jp.golia.smix/smix-sdk)

AI-native UI automation for the iOS Simulator, the Android Emulator, and — once registered — physical iPhones and Android devices. Written in Rust; distributed as a CLI (`smix`), a Rust SDK, and native SDKs for Swift and Kotlin. A TypeScript package ships the same typed API surface and drives a device through the napi addon; three surfaces (`App.screenshot`, `App.openUrl`, `App.launchFresh`) still throw `SmixNotImplementedError` pending their transport — see the [npm package README](./npm/smix-rn/README.md).

Designed for LLM-authored test flows:

- Explicit selectors (`id` / `text` / `label` / `role` / `anchor`) — no XPath, no coordinate guessing
- AI-readable failure messages with visible-element context and edit-distance suggestions
- Playwright-shape API surface mirrored across the SDKs
- Host-side selector resolution + native event injection (IOHID + XCUIElement chain on iOS, UiAutomator on Android)
- MCP server entry for direct Claude Code integration
- Physical devices are a first-class target, held to three rules: a device must be **registered before it can be addressed** (so "whichever phone is plugged in" is never one), destructive actions are **refused per device** until allowed once, and a capability a phone does not have is a **loud error, never a silent no-op**

## Install

Pick the SDK that matches your test harness. All ship the same wire-level primitives.

```bash
# CLI + MCP server, prebuilt — no Rust toolchain needed
npm install -g @goliapkg/smix-cli

# Rust CLI + SDK, from source
cargo install smix-cli --locked

# Swift Package Manager
# add https://github.com/goliajp/smix (product: SmixSDK, from: "6.3.0")

# Gradle / Maven (Kotlin / Java)
# implementation("jp.golia.smix:smix-sdk:6.3.0")

# TypeScript / Node / Bun (drives a simulator through the native addon)
npm install @goliapkg/smix
```

Prerequisites: macOS with Xcode + Simulator (iOS testing); Android SDK with an emulator image (Android testing). For a physical device, USB and a paired phone — plus, on iOS, an Apple Development signing identity and a phone that stays unlocked, because a locked one parks `xcodebuild` rather than failing it.

Coming from 3.x? Device records and leases moved to the machine, so one
checkout no longer holds answers the next cannot see. Run `smix sim
migrate` and `smix lease migrate` once — they copy and never remove —
and read [Migrating to smix 4.0](docs/migrating-to-4.md) if you call the
Rust crates: two signatures changed.

## Quick start

Register a **dedicated device for your project**, then run a YAML flow — `smix
run` needs no `--device` after that, because `smix init` records the project's
default (omit `--alias` and it is derived from the project directory's name):

```bash
smix doctor                                       # says what is missing, and the command for it
smix init --alias dev --device <UDID> --app ./MyApp.app  # registers this project's device, installs the app
smix capsule up dev --bundle <your.bundle.id>     # boots it and starts the runner
smix run examples/hello.yaml                       # no --device: resolves the project's default
```

`smix doctor` prints the next command at every point along that sequence, so
the order above is what it walks you through rather than something to memorise.

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

The TypeScript package declares the same shape and drives through the
napi addon, which `Smix.launchApp` loads for the running platform.
`App.screenshot`, `App.openUrl` and `App.launchFresh` are the three that
still throw `SmixNotImplementedError`, each waiting on a transport of
its own.

See [`docs/ai-guide/01-quickstart.md`](./docs/ai-guide/01-quickstart.md) for a full walkthrough.

### A physical device

Registration is the deliberate act — it is what makes the device addressable
at all, and nothing on this machine can enumerate the world's phones to check
it for you:

```bash
smix sim register phone --udid <UDID> --kind physical-ios
smix runner up phone --bundle com.example.app     # --team <TEAM_ID> if you have more than one
smix sim screenshot phone shot.png                # served by the runner; a phone has no other way to be seen
```

Erasing, uninstalling and keychain resets are refused on it until
`smix sim allow-destructive phone` — recorded once, not confirmed per command,
because a confirmation typed every time ends up pasted into a script.

Full surface: [docs/ai-guide/05-cli.md](docs/ai-guide/05-cli.md#physical-devices).

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

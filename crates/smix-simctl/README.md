# smix-simctl

[![Crates.io](https://img.shields.io/crates/v/smix-simctl?style=flat-square&logo=rust)](https://crates.io/crates/smix-simctl)
[![docs.rs](https://img.shields.io/docsrs/smix-simctl?style=flat-square&logo=docs.rs)](https://docs.rs/smix-simctl)
[![License](https://img.shields.io/crates/l/smix-simctl?style=flat-square)](#license)

Async Rust wrapper around Apple's `xcrun simctl` command-line tool.
Boot / shutdown / launch / install / pasteboard / screenshot — every
simulator-management subcommand wrapped in a typed `tokio::process`-backed
client.

This crate exists because every Rust project doing iOS simulator
automation re-implements the same `Command::new("xcrun").arg("simctl")...`
boilerplate, parses the same JSON outputs, fights the same `pbcopy`
stdin-pipe edge case. `smix-simctl` does it once, cleanly typed.

## Quickstart

```rust,no_run
use smix_simctl::{Appearance, SimctlClient, SimctlPermission};
use std::time::Duration;

# async fn demo() -> Result<(), smix_simctl::DeviceControlError> {
let simctl = SimctlClient::new();

// 1. Inventory
let runtimes = simctl.list_runtimes().await?;
let devices = simctl.list_devices().await?;
let booted: Vec<_> = devices.iter().filter(|d| d.state == "Booted").collect();

// 2. Lifecycle (idempotent on already-booted)
let udid = "4F0B35D2-03F0-4A8F-B729-09072153E8AE";
simctl.boot_and_wait(udid, Duration::from_secs(60)).await?;
let launched = simctl.launch(udid, "com.example.app").await?;
println!("launched pid={}", launched.pid);

// 3. Settings
simctl.set_appearance(udid, Appearance::Dark).await?;
simctl.grant_permission(udid, SimctlPermission::Camera, "com.example.app").await?;
simctl.set_reduce_motion(udid, true).await?;

// 4. IO
let png = simctl.screenshot(udid).await?;
std::fs::write("/tmp/screen.png", png).unwrap();

// 5. Pasteboard
simctl.pasteboard_set(udid, "hello clipboard").await?;
let got = simctl.pasteboard_get(udid).await?;
assert_eq!(got, "hello clipboard");
# Ok(())
# }
```

## Methods

| Category | Methods |
|---|---|
| Inventory | `list_runtimes`, `list_devices` |
| Lifecycle | `boot`, `boot_and_wait`, `shutdown`, `erase`, `install`, `uninstall`, `terminate`, `launch` |
| Settings | `set_appearance`, `grant_permission`, `keychain_reset`, `set_reduce_motion` |
| Pasteboard | `pasteboard_get`, `pasteboard_set` |
| URL | `open_url` |
| IO | `screenshot` (returns `Vec<u8>` PNG bytes) |
| Provisioning | `create_device`, `delete_device` |

## When to reach for this vs. shelling out yourself

| Use case | Pick |
|---|---|
| Need typed `SimctlDevice` / `SimctlRuntime` with `is_available` filter | **smix-simctl** |
| Want `boot_and_wait` polling logic without re-implementing it | **smix-simctl** |
| Need `pasteboard_set` (handles `pbcopy` stdin-pipe correctly) | **smix-simctl** |
| One-off shell script wrapping a single `xcrun simctl boot` call | plain `Command` |
| Need real-device control (instruments, devicectl) | out of scope — this is sim-only |

## Scope

- ✅ All async, returns typed `DeviceControlError` variants
- ✅ Uses `tokio::process::Command` for spawn
- ✅ Parses JSON outputs (`list devices -j`, etc.) into typed structs
- ✅ `screenshot` returns raw PNG bytes — no PNG parsing dep needed
- ❌ No real-device support — this is `xcrun simctl` only
- ❌ No `instruments` / `devicectl` integration
- ❌ No XCUITest runner spawning (see [`smix-runner-client`](https://crates.io/crates/smix-runner-client) for that)

Originally extracted from [smix](https://github.com/goliajp/smix) as a
standalone stone. The crate is intentionally smix-business-decoupled —
any Rust project doing iOS-Simulator automation can adopt it without
pulling in the rest of smix.

## License

Dual-licensed under either:

- [Apache License 2.0](../../LICENSE-APACHE)
- [MIT License](../../LICENSE-MIT)

at your option.

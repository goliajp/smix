# Migrating to smix 14.0.0

14.0.0 is a major release because a public Rust enum gained variants. Flows,
the CLI and the SDKs need no change to keep working.

If you only write YAML or drive smix from the CLI, read **What answers
differently** and stop. **Rust API** is for code that calls the crates.

---

## What answers differently

- **An Android app's covered pages are not on screen.** With an app built
  against this release's `smix-probe`, a node lying wholly inside a window
  the app paints opaquely over it is left out of the tree, and a window that
  takes every touch — a dialog — hides every window below it from reach,
  even where they show through. A flow that asserted on, or tapped, a page
  under a dialog page now fails; it was asserting on something no person
  could see or touch. A flow that skipped `assertNotVisible` on Android for
  this reason can run it on both platforms.
- **The probe has to be this release to get that.** It is built into the
  app: raise `debugImplementation("jp.golia.smix:smix-probe:14.1.1")` and
  rebuild. Until then a run says once that the app carries another
  release's probe, and windows are judged by their shapes as before.
- **smix runs the Android SDK's own adb**, from `ANDROID_HOME`,
  `ANDROID_SDK_ROOT` or `~/Library/Android/sdk`, before any `adb` on `PATH`.
  If you relied on a particular `adb` on `PATH` instead of the SDK's, set
  `ANDROID_HOME` to the SDK that adb belongs to. `smix doctor` shows which
  one is in use.
- **An adb that cannot start fails at once with the reason**, including a
  binary macOS has quarantined, instead of holding a command indefinitely.
- **The single-shot verbs take `--platform`** for a device this machine has
  not registered.

## Rust API

- `smix_adb::AdbError` has new variants `CannotStart(String)`,
  `TimedOut { subcommand, serial, after }` and `NoSdk`, and is
  `#[non_exhaustive]`: add a wildcard arm to a `match` on it.
- New in `smix_adb`: `adb_path`, `adb_usable`, `std_command`, `sdk_root`,
  `sdk_candidates`, `resolve_adb`, `did_not_start` and `AdbSource`.
- New in `smix_runner_client`: `probe_version_note`.

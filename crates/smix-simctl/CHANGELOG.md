# Changelog

All notable changes to `smix-simctl` are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0] - 2026-07-09

Initial public release.

- Tokio-async wrapper over `xcrun simctl` with ~20 methods:
  `list_runtimes` / `list_devices` / `boot` / `boot_and_wait` /
  `shutdown` / `launch` / `install` / `uninstall` / `terminate` /
  `erase` / `open_url` / `set_appearance` / `grant_permission` /
  `keychain_reset` / `pasteboard_get` / `pasteboard_set` /
  `set_reduce_motion` / `screenshot` / `create_device` /
  `delete_device`.
- `SimctlPermission` enum with 16 variants covering every
  documented `xcrun simctl privacy` service (`Calendar` /
  `Contacts` / `Camera` / `Microphone` / `Photos` / `Reminders` /
  `Motion` / `HomeKit` / `MediaLibrary` / `SiriPrivacy` /
  `SpeechRecognition` / `UserTracking` / `Bluetooth` /
  `FaceID` / `Location` / `LocationAlways`).
- `Appearance` enum (`Light` / `Dark`).
- `LaunchResult` struct with `pid` field.
- `SimctlError` `thiserror`-derived enum: `CommandFailed` /
  `JsonParseFailed` / `MissingField` / etc.
- All stdout/stderr surfaced verbatim on failure (no swallowing).

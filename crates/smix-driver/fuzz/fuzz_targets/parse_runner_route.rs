#![no_main]
//! Fuzz the driver-facing wire JSON shapes that user / generator code
//! feeds into `SimctlDriver` routes and the runner-wire reduced shapes
//! consumed by `/scroll` and `/find?include=...`:
//!
//! - `smix_selector::Selector` — the full selector tree accepted by
//!   `SimctlDriver::{find,tap,find_one,find_all}`.
//! - `smix_runner_wire::RunnerScrollSelector` — the reduced text-or-id
//!   selector used by `/scroll` (untagged enum, two variants).
//! - `smix_runner_wire::IncludeScope` — the `include=` scope literal
//!   (kebab-case, single `all-windows` variant today).
//!
//! All three must reject malformed input without panicking before the
//! async runtime stage so untrusted JSON degrades to `serde_json::Error`
//! rather than a host-side abort.
//!
//! Run (nightly required by cargo-fuzz):
//! `cargo +nightly fuzz run parse_runner_route -- -runs=10000`

use libfuzzer_sys::fuzz_target;
use smix_runner_wire::{IncludeScope, RunnerScrollSelector};
use smix_selector::Selector;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<Selector>(data);
    let _ = serde_json::from_slice::<RunnerScrollSelector>(data);
    let _ = serde_json::from_slice::<IncludeScope>(data);
});

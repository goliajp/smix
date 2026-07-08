#![no_main]
//! Fuzz the `xcrun simctl list -j` JSON shapes consumed by
//! `SimctlClient::{list_runtimes,list_devices}` and the `simctl launch`
//! stdout `"<bundle>: <pid>"` parse shape inlined in
//! `SimctlClient::launch` (`crates/smix-simctl/src/lib.rs:355`). Apple
//! bumps both surfaces quietly between Xcode releases; deserialization
//! and stdout parsing must reject malformed input without panicking so a
//! future shape change degrades to a `SimctlError::Malformed` rather than
//! a host-side abort.
//!
//! The launch-stdout arm mirrors the inline parser shape (not a
//! re-export — keeps `lib.rs` surface unchanged per CLAUDE.md §8.1 +
//! §9 #6): `from_utf8 -> rsplit(':').next().map(str::trim) ->
//! parse::<u32>()`. If this arm ever panics on a fuzz input, the
//! production `launch` path would panic on the same Apple stdout shape;
//! the fix is to harden `lib.rs` and re-run.
//!
//! Run (nightly required by cargo-fuzz):
//! `cargo +nightly fuzz run parse_simctl_output -- -runs=10000`

use libfuzzer_sys::fuzz_target;
use smix_simctl::{SimctlDevice, SimctlRuntime};

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<SimctlRuntime>(data);
    let _ = serde_json::from_slice::<SimctlDevice>(data);
    let _ = serde_json::from_slice::<Vec<SimctlRuntime>>(data);
    let _ = serde_json::from_slice::<Vec<SimctlDevice>>(data);

    if let Ok(s) = std::str::from_utf8(data) {
        if let Some(tail) = s.rsplit(':').next().map(str::trim) {
            let _ = tail.parse::<u32>();
        }
    }
});

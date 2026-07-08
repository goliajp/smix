#![no_main]
//! Fuzz the `Selector` JSON shape exposed at the SDK surface (re-exported
//! from `smix-selector`). User code / generator code persisted recordings
//! reach SDK methods via deserialized `Selector` values; the parser must
//! reject malformed input without panicking and `describe_selector` must
//! stay panic-free on any valid parse.

use libfuzzer_sys::fuzz_target;
use smix_sdk::{Selector, describe_selector};

fuzz_target!(|data: &[u8]| {
    let Ok(sel) = serde_json::from_slice::<Selector>(data) else {
        return;
    };
    let _ = describe_selector(&sel);
});

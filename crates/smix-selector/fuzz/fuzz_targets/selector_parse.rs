#![no_main]
//! Fuzz the Selector JSON parse path. Selectors arrive over HTTP wire
//! (recorder JSON, MCP tool calls, runner-client request bodies) — parse
//! must reject malformed input without panicking. Also exercises
//! describe_selector on every successfully-parsed value (renders the
//! human-readable failure-message form, must hold on any valid Selector).

use libfuzzer_sys::fuzz_target;
use smix_selector::{Selector, describe_selector};

fuzz_target!(|data: &[u8]| {
    let Ok(sel) = serde_json::from_slice::<Selector>(data) else {
        return;
    };
    let _ = describe_selector(&sel);
});

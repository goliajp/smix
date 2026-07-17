#![no_main]
//! Fuzz the IRAction JSON parse path. Recorded traces arrive as untrusted
//! JSON arrays (the recorder writes them, but the generator and any
//! third-party consumer reads them); parse must reject malformed input
//! without panicking. Also exercises sort_by_timestamp + kind/timestamp_ms
//! accessors on every successfully-parsed batch.

use libfuzzer_sys::fuzz_target;
use smix_authoring_ir::{IRAction, sort_by_timestamp};

fuzz_target!(|data: &[u8]| {
    if let Ok(action) = serde_json::from_slice::<IRAction>(data) {
        let _ = action.kind();
        let _ = action.timestamp_ms();
        let _ = sort_by_timestamp(&[action]);
    }
    if let Ok(batch) = serde_json::from_slice::<Vec<IRAction>>(data) {
        let _ = sort_by_timestamp(&batch);
        for a in &batch {
            let _ = a.kind();
            let _ = a.timestamp_ms();
        }
    }
});

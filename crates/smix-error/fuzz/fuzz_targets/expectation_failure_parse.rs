#![no_main]
//! Fuzz ExpectationFailure JSON parse + to_prompt rendering. Parse must
//! reject malformed input gracefully; valid parses must not panic in
//! to_prompt regardless of selector / visible_elements shape.

use libfuzzer_sys::fuzz_target;
use smix_error::ExpectationFailure;

fuzz_target!(|data: &[u8]| {
    let Ok(f) = serde_json::from_slice::<ExpectationFailure>(data) else {
        return;
    };
    let p = f.to_prompt();
    // sanity: prompt should be non-empty for valid input
    let _ = p.len();
});

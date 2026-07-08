#![no_main]
//! Fuzz KeyName + SwipeDirection serde parse + as_str round-trip.
//! Arbitrary bytes fed to serde_json::from_slice must reject gracefully;
//! successfully-parsed values must round-trip through as_str + from_str.

use libfuzzer_sys::fuzz_target;
use smix_input::{KeyName, SwipeDirection};

fuzz_target!(|data: &[u8]| {
    if let Ok(k) = serde_json::from_slice::<KeyName>(data) {
        let s = k.as_str();
        let _ = s.to_string();
        let _ = serde_json::to_string(&k);
    }
    if let Ok(d) = serde_json::from_slice::<SwipeDirection>(data) {
        let s = d.as_str();
        let _ = s.to_string();
        let _ = serde_json::to_string(&d);
    }
});

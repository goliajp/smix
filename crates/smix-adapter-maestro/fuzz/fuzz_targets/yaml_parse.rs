#![no_main]
//! Fuzz the maestro YAML parse path. YAML arrives from user-supplied
//! flow files (and runFlow-referenced sub-flows). Parser must reject
//! malformed input without panicking. The c1 scaffold's
//! `parse_flow_yaml` is `todo!()` so this fuzz target will panic until
//! c2 lands the parser body.

use libfuzzer_sys::fuzz_target;
use smix_adapter_maestro::parse_flow_yaml;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = parse_flow_yaml(s);
    }
});

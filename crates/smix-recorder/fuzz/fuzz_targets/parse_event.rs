#![no_main]
//! Fuzz the recorder IRAction JSON parse path + downstream
//! `generate_maestro_yaml` / `generate_rust` emitters. Captured /
//! cleanup-edited sessions land back on disk as JSON arrays; the
//! generators must not panic on any well-typed list of events.

use libfuzzer_sys::fuzz_target;
use smix_recorder::{generate_maestro_yaml, generate_rust};
use smix_recorder_ir::IRAction;

fuzz_target!(|data: &[u8]| {
    let Ok(actions) = serde_json::from_slice::<Vec<IRAction>>(data) else {
        return;
    };
    let _ = generate_maestro_yaml(&actions, "com.example.app");
    let _ = generate_rust(&actions, "com.example.app");
});

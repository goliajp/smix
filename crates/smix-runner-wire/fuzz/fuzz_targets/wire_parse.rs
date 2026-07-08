#![no_main]
//! Fuzz the runner-wire response types. Each represents a JSON body the
//! Swift SmixRunnerCore can return; parse must reject malformed input
//! without panicking on any of: TapResult / SystemPopup / SystemPopupsResponse
//! / RecordedEvent / RecordEventsResponse / FindResponse / ScrollResponse.

use libfuzzer_sys::fuzz_target;
use smix_runner_wire::{
    FindResponse, RecordedEvent, RecordEventsResponse, ScrollResponse, SystemPopup,
    SystemPopupsResponse, TapResult,
};

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<TapResult>(data);
    let _ = serde_json::from_slice::<SystemPopup>(data);
    let _ = serde_json::from_slice::<SystemPopupsResponse>(data);
    let _ = serde_json::from_slice::<RecordedEvent>(data);
    let _ = serde_json::from_slice::<RecordEventsResponse>(data);
    let _ = serde_json::from_slice::<FindResponse>(data);
    let _ = serde_json::from_slice::<ScrollResponse>(data);
});

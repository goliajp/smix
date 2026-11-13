//! Running devices that smix did not boot and nobody has claimed — the
//! ones a script or a bare command started, which the runtime fence will
//! refuse to drive. Named by `smix sim list` and `smix doctor`.

/// Of `running` (simulator UDIDs and emulator serials that are on), the
/// ones the ledger has no owner for. A ledger that cannot be opened names
/// none: these two commands report, and the refusal itself reads the
/// ledger again when it matters.
pub(crate) fn unanswered<'a>(running: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let Ok(leases) = smix_capsule::runner::machine_leases() else {
        return Vec::new();
    };
    running
        .into_iter()
        .filter(|id| smix_capsule::device_fence::refusal(&leases, id).is_some())
        .map(str::to_string)
        .collect()
}

/// Whether an `adb devices` row is a running emulator — the only Android
/// devices the fence is about; a phone is registered on its own route.
pub(crate) fn is_running_emulator(serial: &str, state: &str) -> bool {
    serial.starts_with("emulator-") && state == "device"
}

/// The paragraph that follows a device list when some are unanswered.
pub(crate) fn report(ids: &[String]) -> Option<String> {
    let first = ids.first()?;
    Some(format!(
        "not started by smix and not claimed, so smix will not drive them: {}\n  \
         take one on (logged): smix lease claim {first} --reason \"<why>\"\n  \
         or have whoever started it stop it, then `smix sim boot` it",
        ids.join(", ")
    ))
}

//! Which control drives a device, by its kind.

use crate::load_registry;

/// Whatever drives this kind of device.
///
/// One place decides which tool addresses which kind. It was three
/// hand-written `match device_kind_of(...)` arms before this, on their
/// way to six, and one of them was already wrong: `uninstall` sent a
/// registered iPhone to `simctl`, which lists no phones. A verb that
/// reaches a new kind of device should not also have to know how.
pub(crate) fn device_control_for(
    kind: smix_simctl::registry::DeviceKind,
    udid: &str,
) -> Box<dyn smix_sdk::device_control::DeviceControl> {
    use smix_simctl::registry::DeviceKind;
    // A phone keeps its own state (clipboard, location, photos,
    // animations) unless its row opts it in, as the flow path does.
    let guarded = |inner: Box<dyn smix_sdk::device_control::DeviceControl>| {
        let opted_in = load_registry()
            .registry
            .lookup(udid)
            .is_some_and(|r| r.destructive_opt_in);
        Box::new(smix_sdk::user_state::UserStateGuard::new(
            inner, udid, opted_in,
        ))
    };
    match kind {
        DeviceKind::Simulator => Box::new(smix_sdk::ios_device::IosDeviceControl::new()),
        DeviceKind::Emulator => Box::new(smix_sdk::android_device::AndroidDeviceControl::new()),
        DeviceKind::PhysicalAndroid => guarded(Box::new(
            smix_sdk::android_device::AndroidDeviceControl::new(),
        )),
        DeviceKind::PhysicalIos => guarded(Box::new(
            smix_sdk::devicectl_device::DevicectlClient::new(udid),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smix_simctl::registry::DeviceKind;

    // A made-up phone no registry row opts in: refused before devicectl is
    // started, so nothing is reached.
    #[tokio::test]
    async fn a_phone_with_no_opt_in_keeps_its_clipboard() {
        let control = device_control_for(DeviceKind::PhysicalIos, "00008110-000A1B2C3D4E5F60");
        let e = control
            .pasteboard_set("00008110-000A1B2C3D4E5F60", "x")
            .await
            .expect_err("a phone's clipboard was written without an opt-in");
        assert!(e.to_string().contains("allow-destructive"), "{e}");
    }
}

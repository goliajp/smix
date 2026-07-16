//! Assert `DeviceControl` trait is dyn-compatible.

use smix_sdk::DeviceControl;

fn _dyn_compat() {
    let _: Option<Box<dyn DeviceControl>> = None;
}

#[test]
fn device_control_trait_dyn_compatible() {
    _dyn_compat();
}

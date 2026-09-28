//! Compile-time assertion that `DeviceControl` stays dyn-compatible
//! (`Box<dyn DeviceControl>` must remain a valid type). The check IS
//! this file compiling; the previous version wrapped it in a `#[test]`
//! that could never fail at runtime, which read as coverage it wasn't.

use smix_sdk::DeviceControl;

const _: fn() = || {
    let _: Option<Box<dyn DeviceControl>> = None;
};

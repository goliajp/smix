//! Compile-time assertion that `Driver` stays dyn-compatible
//! (`Box<dyn Driver>` must remain a valid type). The check IS this file
//! compiling; the previous version wrapped it in a `#[test]` that could
//! never fail at runtime, which read as coverage it wasn't.

use smix_driver::Driver;

const _: fn() = || {
    let _: Option<Box<dyn Driver>> = None;
};

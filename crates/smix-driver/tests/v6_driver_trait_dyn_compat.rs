//! v6.0 c1a — assert `Driver` trait is dyn-compatible (Box<dyn Driver> works).

use smix_driver::Driver;

fn _dyn_compat() {
    let _: Option<Box<dyn Driver>> = None;
}

#[test]
fn driver_trait_dyn_compatible() {
    _dyn_compat();
}

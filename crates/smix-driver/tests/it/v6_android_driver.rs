//! `AndroidDriver` impl smoke. Box<dyn Driver> + platform
//! reports Android + as_ios_driver returns None.

use smix_driver::{AndroidDriver, Driver, HttpRunnerClient, Platform};

#[test]
fn android_driver_dyn_compat() {
    let client = HttpRunnerClient::new(28080);
    let _: Box<dyn Driver> = Box::new(AndroidDriver::new(client));
}

#[test]
fn android_driver_platform_reports_android() {
    let driver = AndroidDriver::new(HttpRunnerClient::new(28080));
    assert_eq!(driver.platform(), Platform::Android);
}

#[test]
fn android_driver_as_ios_driver_returns_none() {
    let driver = AndroidDriver::new(HttpRunnerClient::new(28080));
    assert!(driver.as_ios_driver().is_none());
}

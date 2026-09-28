//! `Permission` cross-platform enum maps correctly to iOS
//! `SimctlPermission`. Android mapping (`to_android()`) is verified
//! alongside `AndroidDeviceControl`.

use smix_sdk::Permission;
use smix_simctl::SimctlPermission;

#[test]
fn permission_camera_maps_to_simctl() {
    assert!(matches!(
        Permission::Camera.to_simctl(),
        Some(SimctlPermission::Camera)
    ));
}

#[test]
fn permission_location_maps_to_simctl() {
    assert!(matches!(
        Permission::Location.to_simctl(),
        Some(SimctlPermission::Location)
    ));
}

#[test]
fn permission_storage_android_only() {
    assert!(Permission::Storage.to_simctl().is_none());
}

#[test]
fn permission_post_notifications_aliases_notifications_on_ios() {
    // Android-only by name but maps to iOS Notifications for cross-platform
    // yaml convenience.
    assert!(matches!(
        Permission::PostNotifications.to_simctl(),
        Some(SimctlPermission::Notifications)
    ));
}

//! Ask a physical-device client for something a phone cannot do, and
//! print what it says.
//!
//! The point of running this against real hardware is not the message —
//! a unit test checks that. It is that asking produces **no change on the
//! device**: the refusal happens before anything is dialled.

#[tokio::main]
async fn main() {
    let udid = std::env::args().nth(1).unwrap_or_default();
    let client = smix_sdk::devicectl_device::DevicectlClient::new(udid);
    use smix_sdk::device_control::DeviceControl;
    match client.keychain_reset("").await {
        Ok(()) => println!("UNEXPECTED: keychain_reset reported success on a phone"),
        Err(e) => println!("{e}"),
    }
}

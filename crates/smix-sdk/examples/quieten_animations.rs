//! Run the animation switch against a real device and print what came
//! back.
//!
//! The read-back logic is unit-tested against captured output; this is
//! the other half — that the settings this writes are the ones the
//! device actually keeps. It exists as an example rather than a test
//! because it needs a booted emulator, and a checkpoint that depends on
//! which device happened to be attached cannot be re-run for an answer
//! six months from now.
//!
//! Usage: `cargo run -p smix-sdk --example quieten_animations -- emulator-5554`
//!
//! Name an emulator explicitly. A physical phone is often attached, and
//! this writes settings.

use smix_sdk::{AndroidDeviceControl, DeviceControl};

#[tokio::main]
async fn main() {
    let serial = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("usage: quieten_animations <emulator-NNNN>");
        std::process::exit(2);
    });
    assert!(
        serial.starts_with("emulator-"),
        "refusing to write settings to {serial}: name an emulator"
    );
    let dev = AndroidDeviceControl::new();
    match dev.set_animations_quiet(&serial, true).await {
        Ok(()) => println!("quiet: established and read back on {serial}"),
        Err(e) => {
            println!("quiet: FAILED on {serial}: {e}");
            std::process::exit(1);
        }
    }
    match dev.set_animations_quiet(&serial, false).await {
        Ok(()) => println!("restore: written back on {serial}"),
        Err(e) => println!("restore: FAILED on {serial}: {e}"),
    }
}

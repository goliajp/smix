//! Print the UDID of the first attached iOS device, or nothing.
//!
//! Exists so a shell script can ask "is there a device?" without parsing
//! test output. Silence means no device — which is why it exits 0 either
//! way: absence is an answer, not a failure.

fn main() {
    match smix_usbmux::list_devices() {
        Ok(devices) => {
            if let Some(d) = devices.first() {
                println!("{}", d.serial);
            }
        }
        Err(e) => {
            eprintln!("usbmux: {e}");
        }
    }
}

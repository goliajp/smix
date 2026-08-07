//! Open a forwarder to lockdownd on an attached device and connect to it.
//!
//! Proves the pipe forms end to end — local TCP in, usbmux tunnel out —
//! without depending on anything smix installed on the device. Nothing is
//! sent to lockdownd; the connection is opened and dropped.

use std::io::Write;
use std::net::TcpStream;

const LOCKDOWND: u16 = 62078;

fn main() {
    let serial = std::env::args().nth(1).unwrap_or_default();
    let Ok(Some(device)) = smix_usbmux::find_by_serial(&serial) else {
        eprintln!("no device with serial {serial}");
        std::process::exit(1);
    };
    let forward = match smix_usbmux::forward(device.device_id, LOCKDOWND, 0) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("could not bind a local port: {e}");
            std::process::exit(1);
        }
    };
    let local = forward.local_port();
    match TcpStream::connect(("127.0.0.1", local)) {
        Ok(mut s) => {
            // lockdownd expects a length-prefixed plist; sending nothing
            // keeps this a connectivity check rather than a request.
            let _ = s.flush();
            println!("forwarded 127.0.0.1:{local} -> device {LOCKDOWND} on {serial}");
        }
        Err(e) => {
            eprintln!("local connect to the forwarder failed: {e}");
            std::process::exit(1);
        }
    }
}

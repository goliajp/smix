//! Speak Apple's usbmux protocol: list attached iOS devices, and open a
//! TCP tunnel to a port on one.
//!
//! A physical iPhone does not share the host's loopback, so a server
//! running on the device — like smix's XCUITest runner — cannot be
//! reached by connecting to `127.0.0.1` on the Mac. What bridges the two
//! is `usbmuxd`, and the good news is that it is **Apple's own daemon,
//! shipped with macOS**: `/var/run/usbmuxd` is there on any machine with
//! the developer tools.
//!
//! So the third-party library usually reached for at this point turns out
//! to be a client wrapper around something already present. What is left
//! to write is the protocol itself, and it is small: a 16-byte header and
//! a plist with four keys. That is this crate (IR-1 — what cannot be
//! self-made here is the daemon, and it is not ours to make).

pub mod conn;
pub mod forward;
pub mod plist;
pub mod wire;

pub use conn::{Device, UsbmuxError, connect, find_by_serial, list_devices};
pub use forward::{Forward, forward};

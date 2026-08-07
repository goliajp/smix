//! Tests that need a real iPhone on the other end of a cable.
//!
//! Every one of them prints why it is skipping when there is no device,
//! because a check that quietly does nothing reads as coverage and is
//! not. The distinction matters more than usual here: this crate's whole
//! job is talking to hardware, so a suite that is green on a machine with
//! nothing plugged in has told you almost nothing.
//!
//! Nothing here writes to a device. `ListDevices` is a read, and the one
//! `Connect` opens a tunnel to lockdownd and closes it without sending a
//! single request — what is being proven is that the pipe forms, not
//! anything about what is on the other end.

use smix_usbmux::{UsbmuxError, connect, list_devices};

/// The device, or a printed reason for skipping.
fn device_or_skip(what: &str) -> Option<smix_usbmux::Device> {
    match list_devices() {
        Ok(devices) => {
            if let Some(d) = devices.into_iter().next() {
                return Some(d);
            }
            println!("SKIP {what}: usbmux is running but no iOS device is attached");
            None
        }
        Err(UsbmuxError::NoDaemon) => {
            println!("SKIP {what}: no usbmux daemon on this machine (not macOS, or no Xcode)");
            None
        }
        Err(e) => panic!("listing devices failed in a way that is not 'no device': {e}"),
    }
}

/// lockdownd. Every iOS device listens here, which makes it the right
/// target for proving a tunnel forms without depending on anything smix
/// installed.
const LOCKDOWND: u16 = 62078;

#[test]
fn an_attached_device_reports_a_serial_and_a_transport() {
    let Some(d) = device_or_skip("device listing") else {
        return;
    };
    assert!(
        !d.serial.is_empty(),
        "a device with no serial is not addressable"
    );
    assert!(
        d.connection_type == "USB" || d.connection_type == "Network",
        "unexpected transport {:?}",
        d.connection_type
    );
    println!(
        "device {} serial {} over {}",
        d.device_id, d.serial, d.connection_type
    );
}

#[test]
fn a_tunnel_to_lockdownd_opens() {
    let Some(d) = device_or_skip("tunnel open") else {
        return;
    };
    let sock = connect(d.device_id, LOCKDOWND)
        .unwrap_or_else(|e| panic!("tunnel to lockdownd on device {} failed: {e}", d.device_id));
    // Closed without sending anything: the question is whether the pipe
    // forms, and asking lockdownd for something would be a different test
    // with a side effect.
    drop(sock);
}

#[test]
fn a_port_nobody_listens_on_is_refused_by_the_device_not_by_a_timeout() {
    let Some(d) = device_or_skip("closed-port refusal") else {
        return;
    };
    // Port 1 on iOS has nothing on it. What is asserted is the *shape* of
    // the failure: a caller that polled blindly would have seen a hang,
    // and "the device said no" is the answer that lets them stop.
    match connect(d.device_id, 1) {
        Err(UsbmuxError::Refused { number, detail }) => {
            assert_ne!(number, 0);
            assert!(
                detail.contains("port 1"),
                "refusal should name the port: {detail}"
            );
        }
        Ok(_) => panic!("something is listening on port 1 of device {}", d.device_id),
        Err(e) => panic!("expected a refusal from the device, got: {e}"),
    }
}

#[test]
fn a_serial_that_is_not_attached_is_none_not_an_error() {
    // Distinguishing "not plugged in" from "the lookup broke" is the
    // whole reason this returns Option rather than erroring.
    match smix_usbmux::find_by_serial("00000000-000000000000000A") {
        Ok(None) => {}
        Ok(Some(d)) => panic!("a made-up serial matched device {}", d.device_id),
        Err(UsbmuxError::NoDaemon) => {
            println!("SKIP unknown-serial lookup: no usbmux daemon on this machine");
        }
        Err(e) => panic!("lookup failed in a way that is not 'no daemon': {e}"),
    }
}

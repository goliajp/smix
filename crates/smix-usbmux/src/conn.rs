//! Talking to `usbmuxd`: list devices, open a tunnel.
//!
//! Both operations are one round trip on a Unix socket. `Connect` is the
//! interesting one — after the daemon answers `Number: 0`, the same
//! socket stops being a control channel and becomes a pipe to the port on
//! the device. Nothing further is framed; whatever is written goes
//! through, and whatever the device sends comes back.
//!
//! That is why [`connect`] hands back the [`UnixStream`] itself rather
//! than wrapping it: the caller is going to speak HTTP, or whatever the
//! thing on the other end speaks, and a wrapper would only be in the way.

use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use crate::plist::Value;
use crate::wire::{self, WireError};

/// Where Apple's daemon listens. Present on any macOS with the developer
/// tools; this crate does not install anything.
pub const SOCKET_PATH: &str = "/var/run/usbmuxd";

/// The client name usbmux records for this connection.
const PROG_NAME: &str = "smix";

/// Why a usbmux operation failed.
#[derive(Debug, thiserror::Error)]
pub enum UsbmuxError {
    /// The daemon's socket is not there.
    ///
    /// Said as a fact about *this machine*, because that is what it is —
    /// nothing about the device is known yet at this point.
    #[error(
        "no usbmux daemon at {SOCKET_PATH} — this machine cannot reach iOS devices \
         over USB.\nOn macOS it ships with the developer tools; check `xcode-select -p`."
    )]
    NoDaemon,
    /// Socket-level failure.
    #[error("usbmux socket: {0}")]
    Io(#[from] std::io::Error),
    /// Framing or payload failure.
    #[error(transparent)]
    Wire(#[from] WireError),
    /// The daemon answered, and the answer was no.
    #[error("usbmux refused: {detail} (Number={number})")]
    Refused {
        /// The daemon's numeric result.
        number: i64,
        /// What it means, as far as this crate knows.
        detail: String,
    },
    /// The reply was well-formed but not shaped as expected.
    #[error("usbmux reply had no {field}")]
    MissingField {
        /// What was looked for.
        field: String,
    },
}

/// A device usbmux knows about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Device {
    /// usbmux's own handle for this attachment. **Not stable across
    /// replugs** — it is a session id, not an identity, which is why
    /// [`Self::serial`] exists.
    pub device_id: u32,
    /// The device's real UDID. This is the one to match a registry entry
    /// against; note it differs from the CoreDevice UUID `devicectl`
    /// prints for the same phone.
    pub serial: String,
    /// `"USB"` or `"Network"`.
    pub connection_type: String,
}

fn dict(pairs: &[(&str, Value)]) -> Value {
    Value::Dict(
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
    )
}

fn preamble() -> Vec<(&'static str, Value)> {
    vec![
        ("ClientVersionString", Value::Str(PROG_NAME.into())),
        ("ProgName", Value::Str(PROG_NAME.into())),
        ("kLibUSBMuxVersion", Value::Int(3)),
    ]
}

fn open_socket() -> Result<UnixStream, UsbmuxError> {
    match UnixStream::connect(SOCKET_PATH) {
        Ok(s) => Ok(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(UsbmuxError::NoDaemon),
        Err(e) => Err(UsbmuxError::Io(e)),
    }
}

fn round_trip(sock: &mut UnixStream, payload: &Value, tag: u32) -> Result<Value, UsbmuxError> {
    sock.write_all(&wire::encode_request(payload, tag))?;
    sock.flush()?;

    // The header first, because it says how much else to read. Reading
    // "until the socket goes quiet" would hang on `Connect`, whose socket
    // stays open on purpose.
    let mut header = [0u8; wire::HEADER_LEN];
    sock.read_exact(&mut header)?;
    let declared = wire::declared_len(&header)?;
    let mut body = vec![0u8; declared - wire::HEADER_LEN];
    sock.read_exact(&mut body)?;

    let mut whole = Vec::with_capacity(declared);
    whole.extend_from_slice(&header);
    whole.extend_from_slice(&body);
    Ok(wire::decode_response(&whole)?)
}

/// Every iOS device usbmux currently knows about.
///
/// # Errors
///
/// [`UsbmuxError::NoDaemon`] when this machine has no usbmux socket, plus
/// socket and framing failures. An empty list means no devices; it never
/// means the read failed.
pub fn list_devices() -> Result<Vec<Device>, UsbmuxError> {
    let mut sock = open_socket()?;
    let mut req = preamble();
    req.insert(0, ("MessageType", Value::Str("ListDevices".into())));
    let reply = round_trip(&mut sock, &dict(&req), 1)?;

    let list = reply
        .get("DeviceList")
        .and_then(Value::as_array)
        .ok_or_else(|| UsbmuxError::MissingField {
            field: "DeviceList".into(),
        })?;

    let mut devices = Vec::with_capacity(list.len());
    for entry in list {
        let device_id = entry
            .get("DeviceID")
            .and_then(Value::as_int)
            .ok_or_else(|| UsbmuxError::MissingField {
                field: "DeviceID".into(),
            })?;
        let props = entry.get("Properties");
        let serial = props
            .and_then(|p| p.get("SerialNumber"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let connection_type = props
            .and_then(|p| p.get("ConnectionType"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        devices.push(Device {
            device_id: u32::try_from(device_id).unwrap_or_default(),
            serial,
            connection_type,
        });
    }
    Ok(devices)
}

/// Open a tunnel to `port` on the device usbmux calls `device_id`.
///
/// The returned stream **is** the tunnel: read and write it as if the
/// port were local. There is no further framing, and closing it closes
/// the connection on the device.
///
/// # Errors
///
/// [`UsbmuxError::Refused`] when the daemon answers with a non-zero
/// result — most often because nothing is listening on that port. That is
/// a fact about the device, and it is reported as one rather than as a
/// timeout, which is what a caller polling blindly would have seen.
pub fn connect(device_id: u32, port: u16) -> Result<UnixStream, UsbmuxError> {
    let mut sock = open_socket()?;
    let mut req = preamble();
    req.insert(0, ("MessageType", Value::Str("Connect".into())));
    req.push(("DeviceID", Value::Int(i64::from(device_id))));
    req.push(("PortNumber", Value::Int(wire::port_to_wire(port))));

    let reply = round_trip(&mut sock, &dict(&req), 2)?;
    let number =
        reply
            .get("Number")
            .and_then(Value::as_int)
            .ok_or_else(|| UsbmuxError::MissingField {
                field: "Number".into(),
            })?;
    if number != 0 {
        return Err(UsbmuxError::Refused {
            number,
            detail: match number {
                2 => format!("device {device_id} is not connected"),
                3 => format!("nothing is listening on port {port} of device {device_id}"),
                _ => format!("connecting to port {port} of device {device_id} failed"),
            },
        });
    }
    Ok(sock)
}

/// Find a device by its UDID.
///
/// # Errors
///
/// Propagates listing failures. `Ok(None)` means usbmux does not see that
/// device — unplugged, or the UDID belongs to another phone.
pub fn find_by_serial(serial: &str) -> Result<Option<Device>, UsbmuxError> {
    Ok(list_devices()?
        .into_iter()
        .find(|d| d.serial.eq_ignore_ascii_case(serial)))
}

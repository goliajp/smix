//! The framing usbmux puts around each plist.
//!
//! Sixteen bytes, little-endian: total length *including this header*,
//! protocol version, message type, and a tag the reply echoes back. Then
//! the plist.
//!
//! The length field counting itself is the detail worth stating twice.
//! Get it wrong and nothing fails at encode time — the daemon simply
//! waits for bytes that never come, or reads past the message into the
//! next one. It fails as a hang, far from the mistake.

use crate::plist::{self, Value};

/// Message type for a plist payload. usbmux has older binary types; this
/// crate speaks only the plist one.
pub const TYPE_PLIST: u32 = 8;

/// Protocol version carried in every header.
pub const VERSION: u32 = 1;

/// Header length in bytes, and the amount the length field counts beyond
/// the payload.
pub const HEADER_LEN: usize = 16;

/// Why a response could not be read.
#[derive(Debug, thiserror::Error)]
pub enum WireError {
    /// Fewer than [`HEADER_LEN`] bytes.
    #[error("usbmux response is {got} bytes, too short for a {HEADER_LEN}-byte header")]
    ShortHeader {
        /// What arrived.
        got: usize,
    },
    /// The header's length field does not match what arrived.
    #[error("usbmux header declares {declared} bytes but {got} arrived")]
    LengthMismatch {
        /// From the header.
        declared: usize,
        /// Actually present.
        got: usize,
    },
    /// The header declares a length that cannot include a header.
    #[error("usbmux header declares {declared} bytes, less than the {HEADER_LEN}-byte header")]
    ImpossibleLength {
        /// From the header.
        declared: usize,
    },
    /// The payload is not a plist this crate can read.
    #[error("usbmux payload: {0}")]
    Payload(#[from] plist::PlistError),
}

/// Frame a request: header + XML plist.
///
/// The length field counts the header, which is why it is `body + 16`
/// rather than `body`.
#[must_use]
pub fn encode_request(payload: &Value, tag: u32) -> Vec<u8> {
    let body = plist::to_xml(payload).into_bytes();
    let total = (body.len() + HEADER_LEN) as u32;
    let mut out = Vec::with_capacity(total as usize);
    out.extend_from_slice(&total.to_le_bytes());
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&TYPE_PLIST.to_le_bytes());
    out.extend_from_slice(&tag.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// How many bytes a response claims, header included.
///
/// # Errors
///
/// [`WireError::ShortHeader`] when there is not even a header, and
/// [`WireError::ImpossibleLength`] when the declared length cannot
/// contain one.
pub fn declared_len(header: &[u8]) -> Result<usize, WireError> {
    if header.len() < HEADER_LEN {
        return Err(WireError::ShortHeader { got: header.len() });
    }
    let declared = u32::from_le_bytes([header[0], header[1], header[2], header[3]]) as usize;
    if declared < HEADER_LEN {
        return Err(WireError::ImpossibleLength { declared });
    }
    Ok(declared)
}

/// Read a full framed response into a plist value.
///
/// # Errors
///
/// Propagates framing and payload problems rather than returning an empty
/// value — "no devices" and "could not read the answer" must not look the
/// same to a caller.
pub fn decode_response(bytes: &[u8]) -> Result<Value, WireError> {
    let declared = declared_len(bytes)?;
    if bytes.len() < declared {
        return Err(WireError::LengthMismatch {
            declared,
            got: bytes.len(),
        });
    }
    Ok(plist::from_xml(&bytes[HEADER_LEN..declared])?)
}

/// Put a port number in the byte order usbmux expects.
///
/// `Connect` carries the port **network-endian inside a plist integer** —
/// so 22087 (0x5647) travels as 0x4756. Reversed, the daemon dials a port
/// nobody is on and the failure looks like the device refusing a
/// connection, which is a long way from "the bytes were swapped".
#[must_use]
pub fn port_to_wire(port: u16) -> i64 {
    i64::from(port.swap_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn req() -> Value {
        let mut d = BTreeMap::new();
        d.insert("MessageType".to_string(), Value::Str("ListDevices".into()));
        Value::Dict(d)
    }

    #[test]
    fn the_length_field_counts_the_header() {
        // The mistake this pins does not fail at encode time: the daemon
        // waits for bytes that never arrive, and it reads as a hang.
        let framed = encode_request(&req(), 1);
        let declared = declared_len(&framed).expect("header");
        assert_eq!(declared, framed.len());
        assert_eq!(declared - HEADER_LEN, framed.len() - HEADER_LEN);
    }

    #[test]
    fn the_header_carries_version_type_and_tag() {
        let framed = encode_request(&req(), 42);
        let word = |i: usize| {
            u32::from_le_bytes([
                framed[i * 4],
                framed[i * 4 + 1],
                framed[i * 4 + 2],
                framed[i * 4 + 3],
            ])
        };
        assert_eq!(word(1), VERSION);
        assert_eq!(word(2), TYPE_PLIST);
        assert_eq!(word(3), 42, "the reply echoes this back");
    }

    #[test]
    fn a_framed_response_decodes() {
        let framed = encode_request(&req(), 1);
        let back = decode_response(&framed).expect("decode");
        assert_eq!(
            back.get("MessageType").and_then(super::Value::as_str),
            Some("ListDevices")
        );
    }

    #[test]
    fn a_short_read_is_an_error_not_a_guess() {
        let framed = encode_request(&req(), 1);
        match decode_response(&framed[..framed.len() - 10]) {
            Err(WireError::LengthMismatch { declared, got }) => {
                assert_eq!(declared, framed.len());
                assert_eq!(got, framed.len() - 10);
            }
            other => panic!("expected LengthMismatch, got {other:?}"),
        }
    }

    #[test]
    fn a_header_shorter_than_a_header_is_named_as_such() {
        match decode_response(&[0u8; 4]) {
            Err(WireError::ShortHeader { got: 4 }) => {}
            other => panic!("expected ShortHeader, got {other:?}"),
        }
    }

    #[test]
    fn a_length_that_cannot_hold_a_header_is_refused() {
        let mut bytes = vec![0u8; HEADER_LEN];
        bytes[..4].copy_from_slice(&8u32.to_le_bytes());
        match decode_response(&bytes) {
            Err(WireError::ImpossibleLength { declared: 8 }) => {}
            other => panic!("expected ImpossibleLength, got {other:?}"),
        }
    }

    #[test]
    fn the_port_travels_network_endian() {
        // Reversed, the daemon dials a port nobody is on and the failure
        // reads as the device refusing us.
        // 22087 is 0x5647; on the wire it is 0x4756. These two numbers
        // read alike, which is exactly why the mistake is easy — the
        // values here are the ones the Python probe used when it really
        // did open a tunnel to the runner.
        assert_eq!(port_to_wire(22087), 0x4756);
        assert_eq!(port_to_wire(62078), 0x7EF2);
        // The pair is symmetric, which is what makes the mistake easy.
        assert_eq!(u16::try_from(port_to_wire(256)).unwrap().swap_bytes(), 256);
    }
}

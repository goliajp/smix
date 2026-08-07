//! A local port that behaves like a port on the device.
//!
//! The tunnel from [`crate::connect`] is a `UnixStream`, and almost
//! nothing speaks HTTP over one of those — `reqwest` certainly does not.
//! Rather than teach every caller a second way to reach a runner, this
//! puts a TCP listener on the host and hands each inbound connection its
//! own tunnel.
//!
//! The result is that `http://127.0.0.1:22087` works for a phone exactly
//! as it does for a simulator, and the layers above never learn there was
//! a difference. That is the whole design: the tunnel is plumbing, and
//! plumbing that the rest of the house has to know about is a leak.
//!
//! One tunnel per connection, never shared. usbmux's `Connect` socket is
//! a pipe to one port on one device; two HTTP requests sharing it would
//! interleave their bytes into nonsense.

use std::io;
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::UsbmuxError;

/// How a forwarder reaches the device.
///
/// A trait so the forwarding logic can be exercised without an iPhone:
/// tests substitute a local TCP echo server. Without this the only way to
/// run this code would be to have hardware attached, and the logic that
/// matters here — one tunnel per connection, both directions, clean
/// shutdown — has nothing to do with USB.
pub trait Connector: Send + Sync + 'static {
    /// Open one connection to the device side.
    ///
    /// # Errors
    ///
    /// Whatever prevented the connection, verbatim — the forwarder turns
    /// it into a closed local connection rather than a hang, and the
    /// caller needs to know which it was.
    fn open(&self) -> Result<Box<dyn ReadWrite>, UsbmuxError>;
}

/// A bidirectional stream that can be split for copying.
pub trait ReadWrite: io::Read + io::Write + Send {
    /// Clone the handle so one side can be read while the other writes.
    ///
    /// # Errors
    ///
    /// Propagates the underlying duplication failure.
    fn try_clone_box(&self) -> io::Result<Box<dyn ReadWrite>>;
}

impl ReadWrite for std::os::unix::net::UnixStream {
    fn try_clone_box(&self) -> io::Result<Box<dyn ReadWrite>> {
        Ok(Box::new(self.try_clone()?))
    }
}

impl ReadWrite for TcpStream {
    fn try_clone_box(&self) -> io::Result<Box<dyn ReadWrite>> {
        Ok(Box::new(self.try_clone()?))
    }
}

/// The usbmux connector: one tunnel per call.
struct UsbmuxConnector {
    device_id: u32,
    device_port: u16,
}

impl Connector for UsbmuxConnector {
    fn open(&self) -> Result<Box<dyn ReadWrite>, UsbmuxError> {
        Ok(Box::new(crate::connect(self.device_id, self.device_port)?))
    }
}

/// A running forwarder.
///
/// Dropping it stops accepting new connections. Connections already in
/// flight finish on their own — cutting them at drop would turn an
/// orderly shutdown into a truncated response for whoever was mid-request.
pub struct Forward {
    local_port: u16,
    stop: Arc<AtomicBool>,
}

impl Forward {
    /// The port actually bound.
    ///
    /// Not the port asked for: passing 0 lets the kernel choose, and a
    /// caller that assumed otherwise would dial the wrong number.
    #[must_use]
    pub fn local_port(&self) -> u16 {
        self.local_port
    }

    /// Stop accepting. Idempotent.
    pub fn stop(&self) {
        self.stop.store(true, Ordering::SeqCst);
        // Unblock the accept loop by connecting to it once.
        let _ = TcpStream::connect(("127.0.0.1", self.local_port));
    }
}

impl Drop for Forward {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Forward `127.0.0.1:local_port` to `device_port` on a device.
///
/// `local_port` may be 0, in which case the kernel picks one and
/// [`Forward::local_port`] reports it.
///
/// # Errors
///
/// Binding failures only — a device that refuses the tunnel shows up per
/// connection, not here, because the device is not contacted until
/// somebody actually connects.
pub fn forward(device_id: u32, device_port: u16, local_port: u16) -> io::Result<Forward> {
    forward_with(
        Arc::new(UsbmuxConnector {
            device_id,
            device_port,
        }),
        local_port,
    )
}

/// Forward using any connector. The seam the tests use.
///
/// # Errors
///
/// Binding failures.
pub fn forward_with(connector: Arc<dyn Connector>, local_port: u16) -> io::Result<Forward> {
    let listener = TcpListener::bind(("127.0.0.1", local_port))?;
    let bound = listener.local_addr()?.port();
    let stop = Arc::new(AtomicBool::new(false));

    let stop_for_thread = Arc::clone(&stop);
    std::thread::spawn(move || {
        for incoming in listener.incoming() {
            if stop_for_thread.load(Ordering::SeqCst) {
                return;
            }
            let Ok(client) = incoming else { continue };
            let connector = Arc::clone(&connector);
            std::thread::spawn(move || pump(&client, connector.as_ref()));
        }
    });

    Ok(Forward {
        local_port: bound,
        stop,
    })
}

/// Carry one connection's bytes both ways.
fn pump(client: &TcpStream, connector: &dyn Connector) {
    let device = match connector.open() {
        Ok(d) => d,
        Err(_) => {
            // Close the local side rather than leaving it open with
            // nothing behind it. A caller reading from a socket that will
            // never answer sees a timeout, and a timeout and a refusal
            // call for different responses.
            let _ = client.shutdown(std::net::Shutdown::Both);
            return;
        }
    };
    let Ok(mut device_write) = device.try_clone_box() else {
        let _ = client.shutdown(std::net::Shutdown::Both);
        return;
    };
    let Ok(mut client_write) = client.try_clone() else {
        return;
    };
    let Ok(mut client_read) = client.try_clone() else {
        return;
    };
    let mut device_read = device;

    // Up and down at once: a request can be answered before it has
    // finished being sent, and a half-duplex pump would deadlock on that.
    let up = std::thread::spawn(move || {
        let _ = io::copy(&mut client_read, &mut device_write);
        let _ = device_write.flush();
    });
    let _ = io::copy(&mut device_read, &mut client_write);
    let _ = client_write.shutdown(std::net::Shutdown::Write);
    let _ = up.join();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::sync::Mutex;

    /// A local TCP server standing in for the device.
    struct EchoDevice {
        port: u16,
        opens: Arc<Mutex<usize>>,
    }

    impl EchoDevice {
        fn start(prefix: &'static str) -> Self {
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind echo");
            let port = listener.local_addr().unwrap().port();
            std::thread::spawn(move || {
                for s in listener.incoming() {
                    let Ok(mut s) = s else { continue };
                    std::thread::spawn(move || {
                        let mut buf = [0u8; 1024];
                        while let Ok(n) = s.read(&mut buf) {
                            if n == 0 {
                                return;
                            }
                            let mut out = prefix.as_bytes().to_vec();
                            out.extend_from_slice(&buf[..n]);
                            if s.write_all(&out).is_err() {
                                return;
                            }
                        }
                    });
                }
            });
            Self {
                port,
                opens: Arc::new(Mutex::new(0)),
            }
        }

        fn connector(&self) -> Arc<dyn Connector> {
            Arc::new(EchoConnector {
                port: self.port,
                opens: Arc::clone(&self.opens),
            })
        }
    }

    struct EchoConnector {
        port: u16,
        opens: Arc<Mutex<usize>>,
    }

    impl Connector for EchoConnector {
        fn open(&self) -> Result<Box<dyn ReadWrite>, UsbmuxError> {
            *self.opens.lock().unwrap() += 1;
            Ok(Box::new(
                TcpStream::connect(("127.0.0.1", self.port)).map_err(UsbmuxError::Io)?,
            ))
        }
    }

    /// A connector that always refuses, like a device with nothing on
    /// that port.
    struct RefusingConnector;

    impl Connector for RefusingConnector {
        fn open(&self) -> Result<Box<dyn ReadWrite>, UsbmuxError> {
            Err(UsbmuxError::Refused {
                number: 3,
                detail: "nothing is listening".into(),
            })
        }
    }

    fn round_trip(port: u16, msg: &[u8]) -> Vec<u8> {
        let mut s = TcpStream::connect(("127.0.0.1", port)).expect("connect forwarder");
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        s.write_all(msg).unwrap();
        s.shutdown(std::net::Shutdown::Write).unwrap();
        let mut out = Vec::new();
        let _ = s.read_to_end(&mut out);
        out
    }

    #[test]
    fn the_reported_port_is_the_one_actually_bound() {
        // Asking for 0 means the kernel chooses; a caller that assumed
        // it got 0 would dial the wrong number.
        let dev = EchoDevice::start("");
        let f = forward_with(dev.connector(), 0).expect("forward");
        assert_ne!(f.local_port(), 0);
    }

    #[test]
    fn bytes_travel_both_ways() {
        let dev = EchoDevice::start("echo:");
        let f = forward_with(dev.connector(), 0).expect("forward");
        let out = round_trip(f.local_port(), b"hello");
        assert_eq!(out, b"echo:hello");
    }

    #[test]
    fn two_connections_do_not_share_a_tunnel() {
        // usbmux's Connect socket is a pipe to one port; sharing it would
        // interleave two conversations into nonsense. Each connection
        // must open its own.
        let dev = EchoDevice::start("x:");
        let opens = Arc::clone(&dev.opens);
        let f = forward_with(dev.connector(), 0).expect("forward");
        let a = round_trip(f.local_port(), b"one");
        let b = round_trip(f.local_port(), b"two");
        assert_eq!(a, b"x:one");
        assert_eq!(b, b"x:two");
        assert_eq!(*opens.lock().unwrap(), 2, "one tunnel per connection");
    }

    #[test]
    fn a_refusing_device_closes_the_local_connection_rather_than_hanging() {
        // A socket left open with nothing behind it reads as a timeout,
        // and a timeout calls for a different response than a refusal.
        let f = forward_with(Arc::new(RefusingConnector), 0).expect("forward");
        let mut s = TcpStream::connect(("127.0.0.1", f.local_port())).expect("connect");
        s.set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let _ = s.write_all(b"anything");
        let mut buf = Vec::new();
        let n = s.read_to_end(&mut buf).unwrap_or(0);
        assert_eq!(n, 0, "expected an immediate EOF, got {buf:?}");
    }

    #[test]
    fn stopping_closes_the_door() {
        let dev = EchoDevice::start("");
        let f = forward_with(dev.connector(), 0).expect("forward");
        let port = f.local_port();
        assert_eq!(round_trip(port, b"before"), b"before");
        f.stop();
        std::thread::sleep(std::time::Duration::from_millis(100));
        // Either the connect fails or it yields nothing; both mean the
        // forwarder is no longer serving.
        let served = TcpStream::connect(("127.0.0.1", port))
            .ok()
            .and_then(|mut s| {
                s.set_read_timeout(Some(std::time::Duration::from_millis(500)))
                    .ok()?;
                s.write_all(b"after").ok()?;
                let mut buf = Vec::new();
                let n = s.read_to_end(&mut buf).ok()?;
                (n > 0).then_some(buf)
            });
        assert!(served.is_none(), "forwarder still served after stop");
    }
}

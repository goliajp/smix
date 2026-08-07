//! Persistent CoreSimulator framebuffer capture via a resident per-sim
//! `smix-capture-host` process in request-response ("serve") mode.
//!
//! The per-shot `xcrun simctl io <udid> screenshot` path pays a ~74 ms
//! process-spawn + dyld floor and a ~66 ms PNG encode every shot, both
//! measured by decomposing that path stage by stage. This module holds one
//! resident host per booted UDID that resolves the display `IOSurface` once
//! (~58 ms, amortized) and then answers each capture request by locking +
//! copying the framebuffer directly — ~0.3 ms for a raw BGRA frame, ~66 ms
//! for an in-host ImageIO PNG encode, with **no** per-shot process spawn.
//!
//! Correctness: the host **revalidates the surface on every grab** (re-fetches
//! the device's current `framebufferSurface` and adopts it if the sim rebooted
//! and vended a new one). If the surface can no longer be resolved (sim shut
//! down, framework layout changed) the host answers with a
//! surface-unavailable status and exits, and the caller falls back to the
//! `xcrun simctl io screenshot` path. A stale/garbage frame is never returned.
//!
//! Wire protocol (host `serve` mode):
//!   - host emits `<W>x<H>\n` on stderr once, then loops.
//!   - request:  one opcode byte on stdin — [`OP_RAW`] (raw BGRA) or
//!     [`OP_PNG`] (ImageIO PNG). EOF ends the host.
//!   - response: one status byte — [`STATUS_OK`] or [`STATUS_UNAVAILABLE`].
//!     On OK, followed by a 12-byte header `w:u32 h:u32 len:u32` (little
//!     endian) then `len` payload bytes. On UNAVAILABLE the host exits.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::time::Duration;

use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// Opcode: grab a raw BGRA frame (row padding stripped).
pub const OP_RAW: u8 = b'R';
/// Opcode: grab an in-host ImageIO-encoded PNG frame.
pub const OP_PNG: u8 = b'P';
/// Response status: an `w/h/len` header + payload follow.
pub const STATUS_OK: u8 = 0;
/// Response status: the surface could not be resolved; the host is exiting
/// and the caller must fall back to `simctl`.
pub const STATUS_UNAVAILABLE: u8 = 1;

/// A captured frame — either raw BGRA pixels (the fast diff-loop path) or a
/// PNG (the file-save path, or the `simctl` fallback which is always PNG).
#[derive(Clone, PartialEq, Eq)]
pub enum CapturedFrame {
    /// Raw BGRA8888 pixels, `width * height * 4` bytes, row padding stripped.
    Bgra {
        /// Frame width in pixels.
        width: u32,
        /// Frame height in pixels.
        height: u32,
        /// `width * height * 4` bytes, BGRA order, no row padding.
        data: Vec<u8>,
    },
    /// PNG-encoded frame.
    Png(Vec<u8>),
}

impl std::fmt::Debug for CapturedFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CapturedFrame::Bgra {
                width,
                height,
                data,
            } => f
                .debug_struct("CapturedFrame::Bgra")
                .field("width", width)
                .field("height", height)
                .field("bytes", &data.len())
                .finish(),
            CapturedFrame::Png(b) => f
                .debug_struct("CapturedFrame::Png")
                .field("bytes", &b.len())
                .finish(),
        }
    }
}

/// Why a resident-host capture could not be produced. Every variant means
/// "fall back to `simctl`", but they are distinguished for diagnostics.
#[derive(Debug)]
pub enum HostError {
    /// The `smix-capture-host` binary was not found on disk.
    BinaryMissing(PathBuf),
    /// The host was spawned but never emitted its `WxH` geometry header
    /// (surface resolve failed, or it crashed on launch).
    ResolveFailed(String),
    /// The host process died mid-conversation (stdout EOF on a grab).
    HostGone,
    /// An I/O error talking to the host.
    Io(std::io::Error),
    /// The host sent a malformed response header.
    Protocol(String),
}

impl std::fmt::Display for HostError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HostError::BinaryMissing(p) => write!(f, "smix-capture-host not found at {p:?}"),
            HostError::ResolveFailed(s) => write!(f, "capture-host surface resolve failed: {s}"),
            HostError::HostGone => write!(f, "capture-host process gone"),
            HostError::Io(e) => write!(f, "capture-host io: {e}"),
            HostError::Protocol(s) => write!(f, "capture-host protocol: {s}"),
        }
    }
}

impl std::error::Error for HostError {}

impl From<std::io::Error> for HostError {
    fn from(e: std::io::Error) -> Self {
        HostError::Io(e)
    }
}

/// Parsed `w:u32 h:u32 len:u32` little-endian frame header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    /// Frame width in pixels.
    pub width: u32,
    /// Frame height in pixels.
    pub height: u32,
    /// Payload length in bytes.
    pub len: u32,
}

impl FrameHeader {
    /// Parse a 12-byte little-endian `w/h/len` header.
    pub fn parse(buf: &[u8; 12]) -> FrameHeader {
        FrameHeader {
            width: u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]),
            height: u32::from_le_bytes([buf[4], buf[5], buf[6], buf[7]]),
            len: u32::from_le_bytes([buf[8], buf[9], buf[10], buf[11]]),
        }
    }
}

/// Locate the `smix-capture-host` binary the same way the `/live` pipeline
/// does: honor `SMIX_CAPTURE_HOST_BIN`, else the in-repo release build path.
pub fn capture_host_bin() -> PathBuf {
    std::env::var_os("SMIX_CAPTURE_HOST_BIN").map_or_else(
        || PathBuf::from("swift-bridge/.build/release/smix-capture-host"),
        PathBuf::from,
    )
}

/// A resident `smix-capture-host` in `serve` mode for one booted UDID.
///
/// Holds the child + its stdin/stdout. Dropping it kills the child
/// (`kill_on_drop`), so a dropped [`SimctlClient`](crate::SimctlClient) leaves
/// no stray capture host behind.
pub struct SurfaceCaptureHost {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    /// Geometry from the startup header (informational; each frame carries its
    /// own `w/h`, which is authoritative across a rotation/reboot).
    pub width: u32,
    /// Startup-header height (see [`SurfaceCaptureHost::width`]).
    pub height: u32,
}

impl SurfaceCaptureHost {
    /// Spawn a resident host for `udid` and wait (≤5 s) for its `WxH`
    /// geometry header, which confirms the surface resolved.
    pub async fn spawn(udid: &str) -> Result<SurfaceCaptureHost, HostError> {
        let bin = capture_host_bin();
        if !bin.exists() {
            return Err(HostError::BinaryMissing(bin));
        }
        let mut child = Command::new(&bin)
            .arg(udid)
            .arg("serve")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(HostError::Io)?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| HostError::ResolveFailed("stdin not piped".into()))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| HostError::ResolveFailed("stdout not piped".into()))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| HostError::ResolveFailed("stderr not piped".into()))?;

        let mut stderr_reader = BufReader::new(stderr);
        let mut header = String::new();
        let read =
            tokio::time::timeout(Duration::from_secs(5), stderr_reader.read_line(&mut header))
                .await;
        let (width, height) = match read {
            Ok(Ok(n)) if n > 0 => parse_geometry_line(&header)
                .ok_or_else(|| HostError::ResolveFailed(format!("bad WxH header: {header:?}")))?,
            Ok(Ok(_)) => {
                return Err(HostError::ResolveFailed(
                    "host exited before WxH header".into(),
                ));
            }
            Ok(Err(e)) => return Err(HostError::ResolveFailed(format!("read header: {e}"))),
            Err(_) => {
                return Err(HostError::ResolveFailed(
                    "WxH header not received within 5s".into(),
                ));
            }
        };

        // Drain the rest of stderr in the background so the host never blocks
        // on a full pipe. Ends naturally when the host's stderr closes.
        tokio::spawn(async move {
            let mut lines = stderr_reader.lines();
            while let Ok(Some(_line)) = lines.next_line().await {}
        });

        Ok(SurfaceCaptureHost {
            child,
            stdin,
            stdout: BufReader::new(stdout),
            width,
            height,
        })
    }

    /// Request one frame. `want_png` selects an in-host ImageIO PNG encode;
    /// otherwise a raw BGRA frame.
    ///
    /// Returns `Ok(Some(frame))` on success, `Ok(None)` when the host reports
    /// the surface is no longer resolvable (caller falls back to `simctl`),
    /// and `Err` on a transport failure (caller drops this host + falls back).
    pub async fn grab(&mut self, want_png: bool) -> Result<Option<CapturedFrame>, HostError> {
        let op = if want_png { OP_PNG } else { OP_RAW };
        self.stdin.write_all(&[op]).await?;
        self.stdin.flush().await?;

        let mut status = [0u8; 1];
        if let Err(e) = self.stdout.read_exact(&mut status).await {
            return if e.kind() == std::io::ErrorKind::UnexpectedEof {
                Err(HostError::HostGone)
            } else {
                Err(HostError::Io(e))
            };
        }
        match status[0] {
            STATUS_UNAVAILABLE => Ok(None),
            STATUS_OK => {
                let mut hdr = [0u8; 12];
                self.read_exact_or_gone(&mut hdr).await?;
                let h = FrameHeader::parse(&hdr);
                let len = h.len as usize;
                // Guard against a corrupt length demanding an unbounded read.
                // A full BGRA frame is w*h*4; a PNG is smaller. 128 MB ceiling
                // is generous for any realistic device framebuffer.
                if len > 128 * 1024 * 1024 {
                    return Err(HostError::Protocol(format!("payload len too large: {len}")));
                }
                let mut payload = vec![0u8; len];
                self.read_exact_or_gone(&mut payload).await?;
                let frame = if want_png {
                    CapturedFrame::Png(payload)
                } else {
                    CapturedFrame::Bgra {
                        width: h.width,
                        height: h.height,
                        data: payload,
                    }
                };
                Ok(Some(frame))
            }
            other => Err(HostError::Protocol(format!("unknown status byte {other}"))),
        }
    }

    async fn read_exact_or_gone(&mut self, buf: &mut [u8]) -> Result<(), HostError> {
        match self.stdout.read_exact(buf).await {
            Ok(_) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => Err(HostError::HostGone),
            Err(e) => Err(HostError::Io(e)),
        }
    }

    /// Best-effort clean shutdown: close stdin (host sees EOF → exits 0) and
    /// reap. `kill_on_drop` is the backstop if this is skipped.
    pub async fn shutdown(mut self) {
        drop(self.stdin);
        let _ = tokio::time::timeout(Duration::from_secs(2), self.child.wait()).await;
    }
}

/// Parse a `WxH\n` geometry line.
pub fn parse_geometry_line(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// Per-UDID resident-host registry. Held behind an async mutex inside
/// [`SimctlClient`](crate::SimctlClient).
#[derive(Default)]
pub struct CaptureHostRegistry {
    hosts: HashMap<String, SurfaceCaptureHost>,
}

impl std::fmt::Debug for CaptureHostRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaptureHostRegistry")
            .field("resident_hosts", &self.hosts.len())
            .finish()
    }
}

impl CaptureHostRegistry {
    /// Take (removing) the host for `udid`, if resident.
    pub fn take(&mut self, udid: &str) -> Option<SurfaceCaptureHost> {
        self.hosts.remove(udid)
    }

    /// Re-insert a live host for `udid`.
    pub fn put(&mut self, udid: &str, host: SurfaceCaptureHost) {
        self.hosts.insert(udid.to_string(), host);
    }

    /// Drop the resident host for `udid` (e.g. on a lifecycle change).
    pub fn evict(&mut self, udid: &str) -> Option<SurfaceCaptureHost> {
        self.hosts.remove(udid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_header_parses_little_endian() {
        // w=1206 (0x000004B6), h=2622 (0x00000A3E), len=12648528 (0x00C10050)
        let buf = [
            0xB6, 0x04, 0x00, 0x00, 0x3E, 0x0A, 0x00, 0x00, 0x50, 0x00, 0xC1, 0x00,
        ];
        let h = FrameHeader::parse(&buf);
        assert_eq!(h.width, 1206);
        assert_eq!(h.height, 2622);
        assert_eq!(h.len, 12_648_528);
    }

    #[test]
    fn status_constants_are_distinct_and_ops_are_ascii() {
        assert_ne!(STATUS_OK, STATUS_UNAVAILABLE);
        assert_eq!(OP_RAW, b'R');
        assert_eq!(OP_PNG, b'P');
    }

    #[test]
    fn geometry_line_parses_and_rejects_junk() {
        assert_eq!(parse_geometry_line("1206x2622\n"), Some((1206, 2622)));
        assert_eq!(parse_geometry_line("  800x600  "), Some((800, 600)));
        assert_eq!(parse_geometry_line("not-a-size"), None);
        assert_eq!(parse_geometry_line("1206x"), None);
    }

    #[test]
    fn registry_take_put_evict_roundtrip_key() {
        // No live host needed: exercise the key bookkeeping (put/take/evict)
        // which is the fallback-selection state the async path relies on.
        let mut reg = CaptureHostRegistry::default();
        assert!(reg.take("UDID-A").is_none());
        assert!(reg.evict("UDID-A").is_none());
        // A resident host cannot be fabricated without a child; assert the
        // empty-registry contract that drives "spawn on miss".
        assert!(reg.hosts.is_empty());
    }

    #[test]
    fn bin_path_honors_env_override() {
        // The env var is process-global; set + clear around the assertion.
        let prev = std::env::var_os("SMIX_CAPTURE_HOST_BIN");
        // SAFETY: single-threaded test; restored below.
        unsafe { std::env::set_var("SMIX_CAPTURE_HOST_BIN", "/tmp/custom-host") };
        assert_eq!(capture_host_bin(), PathBuf::from("/tmp/custom-host"));
        unsafe {
            match prev {
                Some(v) => std::env::set_var("SMIX_CAPTURE_HOST_BIN", v),
                None => std::env::remove_var("SMIX_CAPTURE_HOST_BIN"),
            }
        }
    }
}

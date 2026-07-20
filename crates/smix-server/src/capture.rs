//! `mod capture` — sibling of `mod stream`. smix-server actively orchestrates a
//! per-sim live-HLS capture pipeline.
//!
//! The pipeline (rawvideo-pipe single-encoder):
//!
//! ```text
//!   rolling task:  simctl recordVideo (SIGINT finalize)  ──┐  decode to
//!                  → ffmpeg decode .mov to rawvideo CFR ───┤  raw.fifo
//!                                                          ▼
//!   one persistent encoder ffmpeg  reads raw.fifo  →  live HLS (mpegts)
//!                                                     <dir>/index.m3u8 + seg_*.ts
//! ```
//!
//! A single continuous encoder process is what removes per-segment
//! `EXT-X-DISCONTINUITY`: spawning one ffmpeg per recorded round restarts
//! timestamps each time. Here every recorded round is decoded to raw frames
//! and fed into the *same* encoder, so the HLS timeline stays continuous.

use crate::{error::Error, error::Result, state::AppState};
use axum::{Json, extract::State};
use serde::Deserialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::{Child, Command};
use tokio::sync::watch;
use tokio::task::JoinHandle;

pub const CAPTURING_SET: &str = "smix:capturing";

const SEG_SECS: u64 = 2;
const FPS: u32 = 15;

/// Frames the fifo must have received after `elapsed` wall time so the
/// frame-count-paced encoder timeline stays locked to the wall clock.
/// recordVideo restart + decode overhead (~0.15s/round measured) yields
/// less media than wall time; without compensation the live edge drifts
/// behind ~4s per minute and the player periodically stalls/jumps.
fn wall_locked_frame_target(elapsed: Duration, fps: u32) -> u64 {
    (elapsed.as_millis() as u64).saturating_mul(fps as u64) / 1000
}

/// Bytes per rawvideo yuv420p frame; chroma planes are ceil(w/2)×ceil(h/2).
fn yuv420p_frame_bytes(w: u32, h: u32) -> usize {
    let luma = (w as usize) * (h as usize);
    let chroma = (w as usize).div_ceil(2) * (h as usize).div_ceil(2);
    luma + 2 * chroma
}

/// Live handle for one sim's capture pipeline. Dropping it does NOT stop the
/// pipeline — the rolling task detaches and keeps recording, and the encoder is
/// left running. Callers must `stop()` for a clean finalize + resource reclaim.
pub struct CaptureHandle {
    udid: String,
    dir: PathBuf,
    store: std::sync::Arc<smix_store::Store>,
    encoder: Child,
    mode: CaptureMode,
}

/// The active source feeding the encoder. Direct = `smix-capture-host` Swift
/// binary streaming raw BGRA at 30fps from CoreSimulator's IOSurface.
/// Fallback = the recordVideo rolling-segment pipeline.
enum CaptureMode {
    RollingRecord {
        stop_tx: watch::Sender<bool>,
        rolling: JoinHandle<()>,
    },
    Direct {
        host: Child,
        pump: JoinHandle<()>,
    },
}

impl CaptureMode {
    /// Wire name for the pipeline actually in use. `start` falls back
    /// from direct to rolling silently; both used to answer with a
    /// byte-identical `{"status":"started"}`, so a 30fps IOSurface
    /// stream and a 15fps recordVideo rotation were indistinguishable
    /// to every client.
    fn label(&self) -> &'static str {
        match self {
            CaptureMode::Direct { .. } => "direct",
            CaptureMode::RollingRecord { .. } => "rolling-record",
        }
    }
}

impl CaptureHandle {
    /// Which pipeline this capture is actually running.
    #[must_use]
    pub fn mode_label(&self) -> &'static str {
        self.mode.label()
    }

    pub fn udid(&self) -> &str {
        &self.udid
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Stop the pipeline cleanly. The shutdown chain is mode-specific until
    /// the encoder gets EOF; after that, every path waits the encoder out and
    /// SREMs the capturing set.
    pub async fn stop(mut self) -> Result<()> {
        match self.mode {
            CaptureMode::RollingRecord {
                stop_tx,
                mut rolling,
            } => {
                let _ = stop_tx.send(true);
                // The rolling task owns the fifo writer; once it returns, the
                // writer is dropped and the encoder gets EOF. Give the
                // in-flight round time to SIGINT-finalize + decode + drop the
                // writer, but never block forever: if a decode stalls on fifo
                // backpressure, abort the task. Aborting drops its locals —
                // including the writer (→ encoder EOF) and any child
                // (recordVideo has kill_on_drop, so it is reaped, not orphaned).
                match tokio::time::timeout(Duration::from_secs(8), &mut rolling).await {
                    Ok(_) => {}
                    Err(_) => {
                        tracing::warn!("rolling task did not finish in 8s, aborting");
                        rolling.abort();
                        let _ = (&mut rolling).await;
                    }
                }
            }
            CaptureMode::Direct { mut host, pump } => {
                // SIGINT lets the Swift binary's signal handler drain its
                // current row, close stdout, and exit 0. Stdout EOF unblocks
                // the pump task, the pump drops the encoder's stdin handle on
                // exit, and the encoder sees EOF → finalizes the HLS.
                if let Some(pid) = host.id() {
                    // SAFETY: pid is the live child we just spawned; SIGINT is
                    // safe on already-exited PIDs (kill returns ESRCH).
                    unsafe {
                        libc::kill(pid as i32, libc::SIGINT);
                    }
                }
                let _ = tokio::time::timeout(Duration::from_secs(3), host.wait()).await;
                let _ = tokio::time::timeout(Duration::from_secs(3), pump).await;
            }
        }

        match tokio::time::timeout(Duration::from_secs(10), self.encoder.wait()).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => tracing::warn!(error = %e, "encoder wait failed"),
            Err(_) => {
                tracing::warn!("encoder did not finalize in 10s, killing");
                let _ = self.encoder.kill().await;
            }
        }

        crate::capturing::remove(&self.store, &self.udid).map_err(|e| Error::Internal(e.into()))?;
        Ok(())
    }
}

/// Start the capture pipeline for a booted sim. Tries the direct-capture
/// host binary first (30fps BGRA IOSurface → ffmpeg encoder), falls back to
/// the recordVideo rolling-segment pipeline if the host binary is missing,
/// can't resolve a framebuffer, or fails to emit its geometry header within
/// 5s.
pub async fn start(
    udid: &str,
    stream_root: &Path,
    store: std::sync::Arc<smix_store::Store>,
) -> Result<CaptureHandle> {
    booted_device(udid).await?;

    let dir = stream_root.join(udid);
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| Error::Internal(e.into()))?;
    // Fresh start: clear any stale playlist/segments from a previous run.
    clean_dir(&dir).await;

    match try_start_direct(udid, &dir, &store).await {
        Ok(handle) => Ok(handle),
        Err(reason) => {
            tracing::warn!(
                reason = %reason,
                "direct capture unavailable, falling back to recordVideo"
            );
            // The direct attempt may have written partial state (e.g. encoder
            // dropped a stub playlist before being killed); fresh-up again so
            // the fallback path starts from a clean dir.
            clean_dir(&dir).await;
            start_via_record_video(udid, &dir, store).await
        }
    }
}

/// Parse a `WxH\n` geometry header from `smix-capture-host`'s stderr.
fn parse_geometry_line(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.trim().split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

/// Attempt the direct-capture path. Returns the wired `CaptureHandle` on
/// success; on any failure (binary missing, geometry header timeout, encoder
/// spawn) returns a human-readable reason string for the warn log + fallback.
async fn try_start_direct(
    udid: &str,
    dir: &Path,
    store: &std::sync::Arc<smix_store::Store>,
) -> std::result::Result<CaptureHandle, String> {
    use tokio::io::{AsyncBufReadExt, BufReader};

    let bin = std::env::var_os("SMIX_CAPTURE_HOST_BIN").map_or_else(
        || PathBuf::from("swift-bridge/.build/release/smix-capture-host"),
        PathBuf::from,
    );
    if !bin.exists() {
        return Err(format!("smix-capture-host not found at {bin:?}"));
    }

    let mut host = Command::new(&bin)
        .arg(udid)
        .arg("30")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("spawn capture-host: {e}"))?;

    let stderr_pipe = host
        .stderr
        .take()
        .ok_or_else(|| "capture-host stderr not piped".to_string())?;
    let mut stderr_reader = BufReader::new(stderr_pipe);
    let mut header = String::new();
    let read_res =
        tokio::time::timeout(Duration::from_secs(5), stderr_reader.read_line(&mut header)).await;
    let (w, h) = match read_res {
        Ok(Ok(n)) if n > 0 => parse_geometry_line(&header)
            .ok_or_else(|| format!("invalid WxH header from capture-host: {header:?}"))?,
        Ok(Ok(_)) => {
            let _ = host.kill().await;
            return Err("capture-host exited before sending WxH header".into());
        }
        Ok(Err(e)) => {
            let _ = host.kill().await;
            return Err(format!("read WxH header: {e}"));
        }
        Err(_) => {
            let _ = host.kill().await;
            return Err("WxH header not received within 5s".into());
        }
    };

    // Drain remaining stderr in background so capture-host never blocks on a
    // full pipe (the lifetime of this task is bounded by host's exit — when
    // host's stderr closes, `lines.next_line()` returns None and the task
    // exits naturally).
    tokio::spawn(async move {
        let mut lines = stderr_reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            tracing::debug!(line = %line, "capture-host stderr");
        }
    });

    let mut encoder =
        spawn_encoder_direct(dir, w, h).map_err(|e| format!("spawn direct encoder: {e}"))?;
    let mut encoder_stdin = encoder
        .stdin
        .take()
        .ok_or_else(|| "encoder stdin not piped".to_string())?;
    let mut host_stdout = host
        .stdout
        .take()
        .ok_or_else(|| "capture-host stdout not piped".to_string())?;
    // Pump host BGRA → encoder stdin. tokio::io::copy reuses a small buffer
    // (default 8KB) and yields cooperatively, so 38MB/s @ 1206×2622 stays well
    // under one core. The task ends naturally when host's stdout closes (on
    // SIGINT during `stop()` or unexpected crash).
    let pump = tokio::spawn(async move {
        let _ = tokio::io::copy(&mut host_stdout, &mut encoder_stdin).await;
    });

    crate::capturing::add(store, udid).map_err(|e| format!("record capturing: {e}"))?;

    Ok(CaptureHandle {
        udid: udid.to_string(),
        dir: dir.to_path_buf(),
        store: store.clone(),
        encoder,
        mode: CaptureMode::Direct { host, pump },
    })
}

/// simctl recordVideo rolling segments → ffprobe
/// geometry → ffmpeg decode-into-fifo → single persistent encoder with
/// wall-clock pacing. Kept as fallback when the direct path is unavailable.
async fn start_via_record_video(
    udid: &str,
    dir: &Path,
    store: std::sync::Arc<smix_store::Store>,
) -> Result<CaptureHandle> {
    let fifo = dir.join("raw.fifo");
    make_fifo(&fifo)?;

    let (stop_tx, mut stop_rx) = watch::channel(false);

    // Record the first segment up front to learn the geometry.
    let first_mov = dir.join("_seg0.mov");
    let (rec, _) = record_mov(udid, &first_mov, SEG_SECS, &mut stop_rx).await;
    rec?;
    let (w, h) = mov_geometry(&first_mov).await?;

    let mut encoder = spawn_encoder(&fifo, dir, w, h)?;

    // Open the fifo for writing. This blocks until the encoder opens the read
    // end, so it must happen after the encoder is spawned. The rolling task
    // holds this writer for the whole pipeline lifetime — keeping it open is
    // what prevents a premature EOF between rounds.
    let fifo_path = fifo.clone();
    let writer = match tokio::time::timeout(
        Duration::from_secs(15),
        tokio::fs::OpenOptions::new().write(true).open(&fifo_path),
    )
    .await
    {
        Ok(Ok(f)) => f,
        Ok(Err(e)) => {
            let _ = encoder.kill().await;
            return Err(Error::Internal(
                anyhow::Error::new(e).context("open raw.fifo for write"),
            ));
        }
        Err(_) => {
            let _ = encoder.kill().await;
            return Err(Error::Internal(anyhow::anyhow!(
                "encoder never opened raw.fifo (timeout)"
            )));
        }
    };

    let udid_owned = udid.to_string();
    let dir_owned = dir.to_path_buf();
    let rolling = tokio::spawn(rolling_loop(
        udid_owned,
        dir_owned,
        first_mov,
        writer,
        stop_rx,
        yuv420p_frame_bytes(w, h),
    ));

    crate::capturing::add(&store, udid).map_err(|e| Error::Internal(e.into()))?;

    Ok(CaptureHandle {
        udid: udid.to_string(),
        dir: dir.to_path_buf(),
        store,
        encoder,
        mode: CaptureMode::RollingRecord { stop_tx, rolling },
    })
}

/// Rolling capture loop: feed the pre-recorded first segment, then keep
/// recording + decoding fresh segments into the fifo until stop is signalled.
///
/// Wall-clock pacing: the encoder timestamps frames purely by count /
/// FPS, so every frame the capture gaps fail to produce would slide the
/// live edge further behind real time. After each round the loop tops the
/// fifo up to `wall_locked_frame_target` by repeating the last decoded
/// frame — gaps render as a brief freeze instead of accumulating latency.
async fn rolling_loop(
    udid: String,
    dir: PathBuf,
    first_mov: PathBuf,
    mut writer: tokio::fs::File,
    mut stop_rx: watch::Receiver<bool>,
    frame_bytes: usize,
) {
    let t0 = std::time::Instant::now();
    let mut frames_written: u64 = 0;
    let mut last_frame: Vec<u8> = Vec::new();

    match decode_into(&first_mov, &mut writer, frame_bytes, &mut last_frame).await {
        Ok(n) => frames_written += n,
        Err(e) => tracing::warn!(error = %e, "decode first segment failed"),
    }
    let _ = tokio::fs::remove_file(&first_mov).await;

    let mut idx: u32 = 1;
    loop {
        if *stop_rx.borrow() {
            break;
        }
        let mov = dir.join(format!("_seg{idx}.mov"));
        // record_mov finalizes (SIGINT) even if stop fires mid-recording, so no
        // recordVideo is ever orphaned — leaving one holds simctl's host
        // recording lock and blocks every later round / sim.
        let (rec, stopped) = record_mov(&udid, &mov, SEG_SECS, &mut stop_rx).await;
        match rec {
            Ok(()) => match decode_into(&mov, &mut writer, frame_bytes, &mut last_frame).await {
                Ok(n) => frames_written += n,
                Err(e) => tracing::warn!(error = %e, "decode segment failed"),
            },
            Err(e) => {
                // Empty segment (static screen → VFR 0 frames) or transient
                // failure: warn and retry next round, do not poison the fifo.
                tracing::warn!(error = %e, "record_mov failed, skipping round");
            }
        }
        let _ = tokio::fs::remove_file(&mov).await;

        let target = wall_locked_frame_target(t0.elapsed(), FPS);
        let deficit = target.saturating_sub(frames_written);
        if deficit > 0 && !last_frame.is_empty() {
            for _ in 0..deficit {
                if writer.write_all(&last_frame).await.is_err() {
                    break;
                }
            }
            let _ = writer.flush().await;
            frames_written += deficit;
        }

        if stopped {
            break;
        }
        idx += 1;
    }
    // Dropping `writer` here closes the fifo write end → encoder EOF → finalize.
}

/// Decode one `.mov` to CFR rawvideo (yuv420p, fixed fps) and stream it into the
/// persistent fifo writer. Forcing CFR collapses recordVideo's VFR dirty-rect
/// timing into a uniform frame cadence the single encoder can consume seamlessly.
///
/// Frames pass through in exact `frame_bytes` chunks so the caller can
/// count them and retain the last one for wall-clock gap padding. Returns
/// the number of whole frames forwarded; a trailing partial frame (decoder
/// killed mid-frame) is dropped rather than corrupting the fifo alignment.
async fn decode_into(
    mov: &Path,
    writer: &mut tokio::fs::File,
    frame_bytes: usize,
    last_frame: &mut Vec<u8>,
) -> Result<u64> {
    use tokio::io::AsyncReadExt;

    let mut child = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(mov)
        .args([
            "-f",
            "rawvideo",
            "-pix_fmt",
            "yuv420p",
            "-r",
            &FPS.to_string(),
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Internal(anyhow::Error::new(e).context("spawn decoder ffmpeg")))?;

    let mut stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Internal(anyhow::anyhow!("decoder has no stdout")))?;

    let mut frame = vec![0u8; frame_bytes];
    let mut frames: u64 = 0;
    loop {
        let mut filled = 0;
        while filled < frame_bytes {
            match stdout.read(&mut frame[filled..]).await {
                Ok(0) => break,
                Ok(n) => filled += n,
                Err(e) => {
                    let _ = child.wait().await;
                    return Err(Error::Internal(
                        anyhow::Error::new(e).context("read rawvideo from decoder"),
                    ));
                }
            }
        }
        if filled < frame_bytes {
            break;
        }
        writer
            .write_all(&frame)
            .await
            .map_err(|e| Error::Internal(anyhow::Error::new(e).context("pipe rawvideo to fifo")))?;
        last_frame.clear();
        last_frame.extend_from_slice(&frame);
        frames += 1;
    }
    writer
        .flush()
        .await
        .map_err(|e| Error::Internal(anyhow::Error::new(e).context("flush fifo")))?;

    let _ = child.wait().await;
    Ok(frames)
}

/// Spawn the single persistent encoder: read CFR rawvideo from `fifo`, emit a
/// continuous live HLS playlist (no append_list — one process, one timeline).
/// Encoder for the direct-capture path: BGRA stdin pipe at 30fps, 1s GOP.
/// Stdin is `pipe:0` (not a fifo) because the host binary feeds frames through
/// a tokio io::copy pump rather than the persistent-writer fifo trick used by
/// the recordVideo path.
fn spawn_encoder_direct(dir: &Path, w: u32, h: u32) -> Result<Child> {
    let seg_filename = dir.join("seg_%03d.ts");
    let playlist = dir.join("index.m3u8");
    Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "bgra"])
        .args([
            "-s",
            &format!("{w}x{h}"),
            "-framerate",
            "30",
            "-i",
            "pipe:0",
        ])
        .args([
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-tune",
            "zerolatency",
            "-g",
            "30",
            "-pix_fmt",
            "yuv420p",
            "-f",
            "hls",
            "-hls_time",
            "1",
            "-hls_list_size",
            "12",
            "-hls_segment_type",
            "mpegts",
            "-hls_flags",
            "delete_segments+independent_segments",
            "-hls_segment_filename",
        ])
        .arg(&seg_filename)
        .arg(&playlist)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| Error::Internal(anyhow::Error::new(e).context("spawn direct encoder ffmpeg")))
}

fn spawn_encoder(fifo: &Path, dir: &Path, w: u32, h: u32) -> Result<Child> {
    let seg_filename = dir.join("seg_%03d.ts");
    let playlist = dir.join("index.m3u8");
    Command::new("ffmpeg")
        .args(["-y", "-v", "error", "-f", "rawvideo", "-pix_fmt", "yuv420p"])
        .args(["-s", &format!("{w}x{h}"), "-framerate", &FPS.to_string()])
        .arg("-i")
        .arg(fifo)
        .args([
            "-an",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-tune",
            "zerolatency",
            // 1s GOP (15 frames @ 15fps) so every 1s HLS segment can open
            // on a keyframe — a 2s GOP forces the segmenter to 2s anyway.
            "-g",
            "15",
            "-pix_fmt",
            "yuv420p",
            "-f",
            "hls",
            "-hls_time",
            "1",
            "-hls_list_size",
            "12",
            "-hls_segment_type",
            "mpegts",
            "-hls_flags",
            "delete_segments+independent_segments",
            "-hls_segment_filename",
        ])
        .arg(&seg_filename)
        .arg(&playlist)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| Error::Internal(anyhow::Error::new(e).context("spawn encoder ffmpeg")))
}

/// Record one segment via `simctl recordVideo`, finalize with SIGINT, verify.
///
/// recordVideo only flushes the moov atom on a clean SIGINT — SIGTERM (and
/// `child.kill()`, which is SIGKILL) discard the output entirely (0 bytes).
///
/// Returns `(result, stopped)` where `stopped` is true if the recording window
/// was cut short by the stop signal. Crucially, even when stop fires mid-record
/// we still SIGINT-finalize the child rather than abandoning it — an orphaned
/// recordVideo keeps simctl's host recording lock and would wedge the sim.
async fn record_mov(
    udid: &str,
    mov: &Path,
    secs: u64,
    stop_rx: &mut watch::Receiver<bool>,
) -> (Result<()>, bool) {
    let _ = tokio::fs::remove_file(mov).await;
    let spawn = Command::new("xcrun")
        .args([
            "simctl",
            "io",
            udid,
            "recordVideo",
            "--codec=h264",
            "--force",
        ])
        .arg(mov)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        // kill_on_drop is a safety net only (SIGKILL loses output) — the normal
        // path below always SIGINTs first. It guards against a panic unwinding
        // past this point leaving an orphan that holds the host recording lock.
        .kill_on_drop(true)
        .spawn();
    let mut child = match spawn {
        Ok(c) => c,
        Err(e) => {
            return (
                Err(Error::Internal(
                    anyhow::Error::new(e).context("spawn recordVideo"),
                )),
                false,
            );
        }
    };

    let stopped = tokio::select! {
        _ = tokio::time::sleep(Duration::from_secs(secs)) => false,
        _ = stop_rx.changed() => true,
    };

    if let Some(pid) = child.id() {
        // SAFETY: send SIGINT to the recordVideo pid so AVAssetWriter finalizes
        // the file. `child.kill()` would SIGKILL and lose all output.
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGINT);
        }
    }
    let _ = child.wait().await;

    (mov_valid(mov).await, stopped)
}

/// True iff `mov` is non-empty and carries a decodable h264 video stream.
async fn mov_valid(mov: &Path) -> Result<()> {
    let meta = tokio::fs::metadata(mov)
        .await
        .map_err(|e| Error::Internal(anyhow::Error::new(e).context("stat recorded .mov")))?;
    if meta.len() == 0 {
        return Err(Error::Internal(anyhow::anyhow!("empty .mov: {mov:?}")));
    }
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name",
            "-of",
            "csv=p=0",
        ])
        .arg(mov)
        .output()
        .await
        .map_err(|e| Error::Internal(anyhow::Error::new(e).context("ffprobe codec")))?;
    let codec = String::from_utf8_lossy(&out.stdout);
    if codec.contains("h264") {
        Ok(())
    } else {
        Err(Error::Internal(anyhow::anyhow!(
            "no h264 stream in {mov:?}: {codec}"
        )))
    }
}

/// Probe a recorded `.mov`'s width/height — needed to size the rawvideo encoder.
async fn mov_geometry(mov: &Path) -> Result<(u32, u32)> {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "csv=p=0:s=x",
        ])
        .arg(mov)
        .output()
        .await
        .map_err(|e| Error::Internal(anyhow::Error::new(e).context("ffprobe geometry")))?;
    let s = String::from_utf8_lossy(&out.stdout);
    let s = s.trim();
    let (wv, hv) = s
        .split_once('x')
        .ok_or_else(|| Error::Internal(anyhow::anyhow!("unparsable geometry: {s:?}")))?;
    let w: u32 = wv
        .trim()
        .parse()
        .map_err(|_| Error::Internal(anyhow::anyhow!("bad width: {wv:?}")))?;
    let h: u32 = hv
        .trim()
        .parse()
        .map_err(|_| Error::Internal(anyhow::anyhow!("bad height: {hv:?}")))?;
    Ok((w, h))
}

/// Confirm the sim is booted AND return what the registry needs to
/// describe it. Two things at once because both come from one
/// `simctl list`, and because the caller has to write a
/// `stream_sessions` row naming the device.
///
/// This used to substring-match the udid against the whole text of
/// `simctl list devices booted`; the typed client parses the JSON and
/// compares the udid field, so a udid appearing anywhere in the blob
/// (a device name, another column) can no longer read as "booted".
pub(crate) async fn booted_device(udid: &str) -> Result<smix_simctl::SimctlDevice> {
    let devices = smix_simctl::SimctlClient::new()
        .list_devices()
        .await
        .map_err(|e| Error::Internal(anyhow::Error::new(e).context("simctl list devices")))?;
    let device = devices
        .into_iter()
        .find(|d| d.udid.eq_ignore_ascii_case(udid))
        .ok_or_else(|| Error::Internal(anyhow::anyhow!("no such sim: {udid}")))?;
    if device.state != "Booted" {
        return Err(Error::Internal(anyhow::anyhow!(
            "sim not booted: {udid} (state={})",
            device.state
        )));
    }
    Ok(device)
}

fn make_fifo(path: &Path) -> Result<()> {
    let _ = std::fs::remove_file(path);
    let c = std::ffi::CString::new(path.as_os_str().as_encoded_bytes())
        .map_err(|e| Error::Internal(anyhow::Error::new(e).context("fifo path to CString")))?;
    // SAFETY: mkfifo(2) on a path that does not exist; 0o600 owner rw.
    let rc = unsafe { libc::mkfifo(c.as_ptr(), 0o600) };
    if rc != 0 {
        return Err(Error::Internal(anyhow::Error::new(
            std::io::Error::last_os_error(),
        )));
    }
    Ok(())
}

async fn clean_dir(dir: &Path) {
    if let Ok(mut rd) = tokio::fs::read_dir(dir).await {
        while let Ok(Some(entry)) = rd.next_entry().await {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == "index.m3u8"
                || name.starts_with("seg_")
                || name.starts_with("_seg")
                || name == "raw.fifo"
            {
                let _ = tokio::fs::remove_file(entry.path()).await;
            }
        }
    }
}

// ── HTTP handlers ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct StartReq {
    pub udid: String,
}

#[derive(Deserialize)]
pub struct StopReq {
    pub udid: String,
}

pub async fn start_capture(
    State(st): State<AppState>,
    Json(req): Json<StartReq>,
) -> Result<Json<Value>> {
    let udid = req.udid.trim();
    if udid.is_empty() {
        return Err(Error::BadRequest("udid is required".into()));
    }
    // Claim the slot before the long bring-up, so a concurrent start for
    // the same udid loses here rather than building a second pipeline.
    {
        let mut captures = st.captures.lock().await;
        if captures.contains_key(udid) {
            return Ok(Json(json!({ "status": "already_started", "udid": udid })));
        }
        captures.insert(udid.to_string(), None);
    }
    let started = start(udid, Path::new(&st.cfg.stream_root), st.store.clone()).await;
    let mut captures = st.captures.lock().await;
    match started {
        Ok(handle) => {
            let mode = handle.mode_label();
            let stream_path = handle
                .dir()
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(udid)
                .to_string();
            captures.insert(udid.to_string(), Some(handle));
            drop(captures);
            // The stream exists now, so the registry the dashboard reads
            // has to learn about it — nothing wrote that table before,
            // which is why /api/sims was empty in every real deployment
            // while capture ran fine. A failure here does not tear the
            // pipeline down (the capture is live and watchable by path)
            // but it is logged at error, because the symptom is a sim
            // that streams and never appears in the list.
            let device = booted_device(udid).await?;
            if let Err(e) = crate::stream::register_session(
                &st.pg,
                udid,
                &device.name,
                &device.runtime_identifier,
                &stream_path,
            )
            .await
            {
                tracing::error!(
                    udid = %udid,
                    error = %e,
                    "capture started but the sim could not be registered — it will \
                     stream without appearing in /api/sims"
                );
            }
            Ok(Json(
                json!({ "status": "started", "udid": udid, "mode": mode }),
            ))
        }
        Err(e) => {
            // Release the claim, or the device is wedged until restart.
            captures.remove(udid);
            Err(e)
        }
    }
}

pub async fn stop_capture(
    State(st): State<AppState>,
    Json(req): Json<StopReq>,
) -> Result<Json<Value>> {
    let udid = req.udid.trim();
    if udid.is_empty() {
        return Err(Error::BadRequest("udid is required".into()));
    }
    // Stop BEFORE removing: the old order removed first, so a stop()
    // that failed part-way (e.g. the valkey `srem`) dropped the handle
    // on the floor — pipeline still running, udid still in the
    // capturing set, and a retry answering 404.
    let mut captures = st.captures.lock().await;
    match captures.get_mut(udid) {
        Some(slot) => match slot.take() {
            Some(h) => match h.stop().await {
                Ok(()) => {
                    captures.remove(udid);
                    Ok(Json(json!({ "status": "stopped", "udid": udid })))
                }
                Err(e) => {
                    // Keep the slot claimed so a retry reaches this arm
                    // again instead of starting a second pipeline.
                    Err(e)
                }
            },
            // Slot claimed but the bring-up has not finished yet.
            None => Err(Error::BadRequest("capture is still starting".into())),
        },
        None => Err(Error::NotFound("not capturing".into())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_target_tracks_wall_clock() {
        // 3.13s of wall time at 15fps → 46 frames (floor), not the 45 a
        // 3.00s media segment yields — the +1 is the drift compensation.
        assert_eq!(
            wall_locked_frame_target(Duration::from_millis(3130), 15),
            46
        );
        assert_eq!(wall_locked_frame_target(Duration::from_secs(60), 15), 900);
        assert_eq!(wall_locked_frame_target(Duration::ZERO, 15), 0);
    }

    #[test]
    fn yuv420p_frame_bytes_handles_odd_dims() {
        assert_eq!(yuv420p_frame_bytes(4, 4), 24);
        // odd dims: chroma planes use ceil(w/2) x ceil(h/2)
        assert_eq!(yuv420p_frame_bytes(5, 3), 15 + 2 * (3 * 2));
    }
}

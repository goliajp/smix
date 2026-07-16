//! Log source subscribers.
//!
//! Two shapes:
//!
//! - [`WebSocketSubscriber`] — connects to a metro / expo dev server
//!   WebSocket endpoint. Auto-reconnects on disconnect (5 s backoff).
//!   Metro sends either plain text lines or JSON `{ level, message }`
//!   objects; both are normalized into `LogEntry` via
//!   [`parse_log_line`].
//!
//! - [`FileTailSubscriber`] — tails an on-disk log file (`~/Library/
//!   Logs/expo/metro.log` etc.). Poll-based (100 ms tick) using
//!   `File::metadata` size delta; simpler than notify+watcher and
//!   works across macOS/Linux without extra deps.
//!
//! Both spawn a tokio task and return a [`SubscriberHandle`] the
//! caller can drop to signal shutdown.

use std::path::PathBuf;
use std::time::Duration;

use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncSeekExt, SeekFrom};
use tokio::sync::watch;
use tokio_tungstenite::tungstenite::Message;

use crate::{LogLevel, MetroLogTail};

/// Handle returned by [`WebSocketSubscriber::start`] / [`FileTailSubscriber::start`].
/// Drop it — or call [`SubscriberHandle::shutdown`] — to stop the subscriber task.
pub struct SubscriberHandle {
    shutdown_tx: watch::Sender<bool>,
    join: Option<tokio::task::JoinHandle<()>>,
}

impl SubscriberHandle {
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(true);
    }

    /// Await the subscriber task after signaling shutdown.
    pub async fn join(mut self) {
        self.shutdown();
        if let Some(h) = self.join.take() {
            let _ = h.await;
        }
    }
}

impl Drop for SubscriberHandle {
    fn drop(&mut self) {
        let _ = self.shutdown_tx.send(true);
    }
}

/// Parse one line coming from metro/expo. Recognizes:
///
/// - JSON `{"level":"warn","message":"..."}` (metro dev-server format
///   for RN 0.75+)
/// - Plain text (defaulted to `LogLevel::Log`)
pub fn parse_log_line(raw: &str) -> (LogLevel, String) {
    let trimmed = raw.trim();
    if trimmed.starts_with('{')
        && let Ok(v) = serde_json::from_str::<serde_json::Value>(trimmed)
    {
        let level = v
            .get("level")
            .and_then(|x| x.as_str())
            .map(LogLevel::parse)
            .unwrap_or(LogLevel::Log);
        let msg = v
            .get("message")
            .and_then(|x| x.as_str())
            .map(String::from)
            .unwrap_or_else(|| trimmed.to_string());
        return (level, msg);
    }
    (LogLevel::Log, trimmed.to_string())
}

pub struct WebSocketSubscriber;

impl WebSocketSubscriber {
    /// Start a background task that connects, drains messages into
    /// `tail`, and auto-reconnects with 5s backoff. Panic-free
    /// (transport errors → warn to stderr → retry).
    pub fn start(url: String, tail: MetroLogTail) -> SubscriberHandle {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let join = tokio::spawn(async move {
            ws_loop(url, tail, shutdown_rx).await;
        });
        SubscriberHandle {
            shutdown_tx,
            join: Some(join),
        }
    }
}

async fn ws_loop(url: String, tail: MetroLogTail, mut shutdown_rx: watch::Receiver<bool>) {
    let backoff = Duration::from_secs(5);
    loop {
        if *shutdown_rx.borrow() {
            return;
        }
        match tokio_tungstenite::connect_async(&url).await {
            Ok((ws, _resp)) => {
                let (_write, mut read) = ws.split();
                loop {
                    tokio::select! {
                        _ = shutdown_rx.changed() => {
                            if *shutdown_rx.borrow() { return; }
                        }
                        msg = read.next() => match msg {
                            Some(Ok(Message::Text(text))) => {
                                for line in text.lines() {
                                    if !line.trim().is_empty() {
                                        let (level, m) = parse_log_line(line);
                                        tail.push(level, m);
                                    }
                                }
                            }
                            Some(Ok(Message::Binary(bin))) => {
                                if let Ok(s) = std::str::from_utf8(&bin) {
                                    for line in s.lines() {
                                        if !line.trim().is_empty() {
                                            let (level, m) = parse_log_line(line);
                                            tail.push(level, m);
                                        }
                                    }
                                }
                            }
                            Some(Ok(_)) => {}
                            Some(Err(e)) => {
                                eprintln!("smix-metro-log: ws recv error, will reconnect: {e}");
                                break;
                            }
                            None => break,
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("smix-metro-log: ws connect {url} failed: {e}");
            }
        }
        // Sleep with shutdown-awareness.
        tokio::select! {
            _ = tokio::time::sleep(backoff) => {}
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() { return; }
            }
        }
    }
}

pub struct FileTailSubscriber;

impl FileTailSubscriber {
    /// Start a background task that polls `path` at 100 ms intervals
    /// for size growth; new bytes become new [`crate::LogEntry`] items.
    pub fn start(path: PathBuf, tail: MetroLogTail) -> SubscriberHandle {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let join = tokio::spawn(async move {
            file_loop(path, tail, shutdown_rx).await;
        });
        SubscriberHandle {
            shutdown_tx,
            join: Some(join),
        }
    }
}

async fn file_loop(path: PathBuf, tail: MetroLogTail, mut shutdown_rx: watch::Receiver<bool>) {
    // First-open marker distinct from `pos == 0`: 0 is a valid file
    // offset once we've read the whole file.
    let mut seeded = false;
    let mut pos: u64 = 0;
    let mut leftover = String::new();
    loop {
        if *shutdown_rx.borrow() {
            return;
        }
        if let Ok(mut f) = tokio::fs::OpenOptions::new().read(true).open(&path).await {
            if !seeded {
                // First open: start at current end (don't replay
                // history). Same convention as `tail -f` default.
                if let Ok(end) = f.seek(SeekFrom::End(0)).await {
                    pos = end;
                }
                seeded = true;
            }
            if f.seek(SeekFrom::Start(pos)).await.is_ok() {
                let mut buf = Vec::new();
                if let Ok(n) = f.read_to_end(&mut buf).await
                    && n > 0
                {
                    pos += n as u64;
                    if let Ok(s) = std::str::from_utf8(&buf) {
                        let combined = format!("{leftover}{s}");
                        let mut it = combined.split('\n').peekable();
                        while let Some(line) = it.next() {
                            if it.peek().is_none() {
                                // Last piece — might be a partial
                                // line. Keep for next tick.
                                leftover = line.to_string();
                            } else if !line.trim().is_empty() {
                                let (level, m) = parse_log_line(line);
                                tail.push(level, m);
                            }
                        }
                    }
                }
            }
        }
        tokio::select! {
            _ = tokio::time::sleep(Duration::from_millis(100)) => {}
            _ = shutdown_rx.changed() => {
                if *shutdown_rx.borrow() { return; }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn parse_plain_text_line() {
        let (level, msg) = parse_log_line("env=qa-mode ready");
        assert_eq!(level, LogLevel::Log);
        assert_eq!(msg, "env=qa-mode ready");
    }

    #[test]
    fn parse_json_line_with_level() {
        let (level, msg) = parse_log_line(r#"{"level":"warn","message":"careful"}"#);
        assert_eq!(level, LogLevel::Warn);
        assert_eq!(msg, "careful");
    }

    #[test]
    fn parse_json_without_message_falls_back_to_raw() {
        let raw = r#"{"level":"warn"}"#;
        let (level, msg) = parse_log_line(raw);
        assert_eq!(level, LogLevel::Warn);
        assert_eq!(msg, raw);
    }

    #[tokio::test]
    async fn file_tail_reads_appended_lines() {
        // Setup: temp file, subscribe, append lines, verify they appear
        // in the tail's ring buffer.
        let dir = std::env::temp_dir();
        let path = dir.join(format!("smix-metro-log-test-{}.log", std::process::id()));
        // Pre-create empty file.
        tokio::fs::write(&path, b"").await.unwrap();
        let tail = MetroLogTail::new();
        let handle = FileTailSubscriber::start(path.clone(), tail.clone());

        // Give the subscriber a beat to seek-to-end.
        tokio::time::sleep(Duration::from_millis(120)).await;

        // Append 3 lines.
        tokio::fs::write(&path, b"line-1\nline-2\nline-3\n")
            .await
            .unwrap();

        // Poll until the ring buffer has them.
        let mut got = 0;
        for _ in 0..30 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            got = tail.snapshot().len();
            if got >= 3 {
                break;
            }
        }
        handle.join().await;
        let _ = tokio::fs::remove_file(&path).await;
        assert_eq!(got, 3, "expected 3 log entries, got {got}");
    }
}

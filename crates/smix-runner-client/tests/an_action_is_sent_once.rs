//! A request that may already have acted on the device is not sent again.
//!
//! Reproduced 2026-09-25 on emulator-5554 under host load: `inputText:
//! 'mock@…'` left the field holding `mocmock@…`. The
//! runner was slow to answer, the client's timeout fired, and the transport
//! retry sent the same POST a second time — so the device typed twice. The
//! retry was written for failures "before any bytes leave the local socket";
//! reqwest files a timeout, and a connection dropped after the request went
//! out, under the same `is_request()` that retry trusted.
//!
//! The runner here takes the whole request and hangs up without a word:
//! the request certainly arrived, and nothing came back.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use smix_runner_client::{HttpRunnerClient, RunnerTransportError};

/// A runner that reads each request whole and closes without answering.
/// Returns its port and the number of requests it has read.
fn hangs_up_after_reading() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
    let port = listener
        .local_addr()
        .expect("a bound listener has an address")
        .port();
    let seen = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let mut buf = Vec::new();
            let mut chunk = [0u8; 4096];
            // Headers, then the body `content-length` promises.
            while let Ok(n) = s.read(&mut chunk) {
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
                let text = String::from_utf8_lossy(&buf);
                if let Some(end) = text.find("\r\n\r\n") {
                    let want = text[..end]
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse::<usize>().ok())?
                        })
                        .unwrap_or(0);
                    if buf.len() >= end + 4 + want {
                        break;
                    }
                }
            }
            count.fetch_add(1, Ordering::SeqCst);
            // Close without a response: the request went in, nothing came out.
            let _ = s.flush(); // the peer is about to be dropped either way
        }
    });
    (port, seen)
}

#[tokio::test]
async fn typing_that_went_unanswered_is_not_typed_again() {
    let (port, seen) = hangs_up_after_reading();
    let client = HttpRunnerClient::with_base(format!("http://127.0.0.1:{port}"));
    let err = client
        .input_text("mock@example.test")
        .await
        .expect_err("a runner that answered nothing cannot have said the text landed");
    assert_eq!(
        seen.load(Ordering::SeqCst),
        1,
        "the runner was asked to type {} times for one step",
        seen.load(Ordering::SeqCst)
    );
    assert!(
        matches!(err, RunnerTransportError::SentWithoutAnswer { .. }),
        "the error should say the request went out and nothing came back, got: {err}"
    );
    let said = err.to_string();
    assert!(
        said.contains("/input-text") && said.contains("not sent again"),
        "the sentence should name the route and that it was not repeated: {said}"
    );
}

#[tokio::test]
async fn a_look_that_went_unanswered_is_asked_again() {
    // The other side of the same line: asking for the tree twice changes
    // nothing on the device, so a lost answer is worth another try.
    let (port, seen) = hangs_up_after_reading();
    let client = HttpRunnerClient::with_base(format!("http://127.0.0.1:{port}"));
    let _ = client.get_tree(None).await; // it fails; what matters is how often it asked
    assert!(
        seen.load(Ordering::SeqCst) > 1,
        "a GET that went unanswered was asked {} time(s) — reading is safe to repeat",
        seen.load(Ordering::SeqCst)
    );
}

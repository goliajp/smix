//! An action is never written onto a connection the runner is closing.
//!
//! The iOS runner's HTTP server keeps a connection open only when the
//! request asks for it with `Connection: keep-alive`; otherwise it closes
//! the connection once it has answered, and says nothing about it in the
//! response. An HTTP/1.1 client reads the silence as keep-alive and puts
//! the connection back in its pool, so the next request could be written
//! onto a socket the runner was about to close. It never reached the
//! runner, and the client could only report that it had been sent and no
//! answer came back. Reproduced 2026-09-27 on a simulator: a
//! `pressKey`/`swipe` flow failed three runs in three on a step that the
//! runner's log shows never arrived.
//!
//! The runner here does what that server does: one request per
//! connection, answered, then closed a moment later without reading on.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use smix_runner_client::HttpRunnerClient;

fn read_one_request(s: &mut std::net::TcpStream) -> bool {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 4096];
    loop {
        let Ok(n) = s.read(&mut chunk) else {
            return false;
        };
        if n == 0 {
            return false;
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
                return true;
            }
        }
    }
}

/// Answers the first request on each connection, then closes it after
/// `linger` without reading anything else. Returns its port and how many
/// requests it answered.
fn one_request_per_connection(linger: Duration) -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind a loopback port");
    let port = listener
        .local_addr()
        .expect("a bound listener has an address")
        .port();
    let answered = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&answered);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            let count = Arc::clone(&count);
            std::thread::spawn(move || {
                if !read_one_request(&mut s) {
                    return;
                }
                let body = r#"{"ok":true}"#;
                let head = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nContent-Type: application/json\r\n\r\n",
                    body.len()
                );
                if s.write_all(head.as_bytes()).is_err() || s.write_all(body.as_bytes()).is_err() {
                    return;
                }
                count.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(linger);
            });
        }
    });
    (port, answered)
}

#[tokio::test]
async fn back_to_back_actions_each_reach_a_runner_that_closes_after_answering() {
    let (port, answered) = one_request_per_connection(Duration::from_millis(200));
    let client = HttpRunnerClient::with_base(format!("http://127.0.0.1:{port}"));
    for step in 0..5 {
        client
            .swipe_at_norm_coord((0.5, 0.7), (0.5, 0.3))
            .await
            .unwrap_or_else(|e| panic!("swipe {step} did not reach the runner: {e}"));
    }
    assert_eq!(
        answered.load(Ordering::SeqCst),
        5,
        "the runner answered {} of 5 swipes",
        answered.load(Ordering::SeqCst)
    );
}

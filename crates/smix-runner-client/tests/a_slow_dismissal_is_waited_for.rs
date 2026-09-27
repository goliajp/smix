//! A keyboard dismissal that outlasts the plain request timeout is waited
//! for, and the runner is told how long it has.
//!
//! On a GitHub macOS runner (iOS 26.2) `hideKeyboard` ran past 15 s, the
//! host stopped listening at 15 s, and the step failed as "sent and no
//! answer" while the runner was still dismissing — then passed on a retry.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use smix_runner_client::HttpRunnerClient;

/// A runner that reads one request, keeps its body, and answers
/// `{"ok":true}` after `delay`.
fn answers_after(delay: Duration) -> (u16, Arc<Mutex<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(Mutex::new(String::new()));
    let keep = Arc::clone(&seen);
    std::thread::spawn(move || {
        let Ok((mut s, _)) = listener.accept() else {
            return;
        };
        let mut buf = Vec::new();
        let mut chunk = [0u8; 4096];
        while let Ok(n) = s.read(&mut chunk) {
            if n == 0 {
                break;
            }
            buf.extend_from_slice(&chunk[..n]);
            let text = String::from_utf8_lossy(&buf).to_string();
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
                    *keep.lock().expect("lock") = text[end + 4..].to_string();
                    break;
                }
            }
        }
        std::thread::sleep(delay);
        let body = r#"{"ok":true}"#;
        let _ = write!(
            s,
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
    });
    (port, seen)
}

#[tokio::test]
async fn a_dismissal_that_takes_sixteen_seconds_is_answered_not_abandoned() {
    let (port, seen) = answers_after(Duration::from_secs(16));
    let client = HttpRunnerClient::with_base(format!("http://127.0.0.1:{port}"));
    client
        .hide_keyboard()
        .await
        .expect("the runner answered; the host has to still be listening");
    let body = seen.lock().expect("lock").clone();
    assert!(
        body.contains(r#""budgetMs":20000"#),
        "the runner has to be told its budget: {body}"
    );
}

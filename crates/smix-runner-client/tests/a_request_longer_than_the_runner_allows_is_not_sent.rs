//! A request whose wait would outlast what the runner lets any handler run
//! is refused before it is sent, naming both.
//!
//! The SDKs' `input_text` sends the whole text in one `/input-text`. The
//! host waits in proportion to its length; past about 2,100 characters
//! that wait is longer than the iOS runner's server lets a handler run, so
//! the server would answer 500 in the handler's place while the host was
//! still waiting — a failure that names neither the length nor the limit.

use std::io::Read;
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use smix_runner_client::{HttpRunnerClient, RunnerTransportError};

fn counting_runner() -> (u16, Arc<AtomicUsize>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let seen = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&seen);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut s) = stream else { continue };
            count.fetch_add(1, Ordering::SeqCst);
            let mut chunk = [0u8; 4096];
            let _ = s.read(&mut chunk);
        }
    });
    (port, seen)
}

#[tokio::test]
async fn text_too_long_for_one_request_is_refused_before_it_is_sent() {
    let (port, seen) = counting_runner();
    let client = HttpRunnerClient::with_base(format!("http://127.0.0.1:{port}"));
    let text = "x".repeat(3_000);
    let err = client
        .input_text(&text)
        .await
        .expect_err("a request the runner would cut off must not go out");
    assert_eq!(seen.load(Ordering::SeqCst), 0, "the request was sent");
    assert!(
        matches!(err, RunnerTransportError::OutlastsTheRunner { .. }),
        "{err:?}"
    );
    let said = err.to_string();
    assert!(
        said.contains("/input-text") && said.contains("600"),
        "{said}"
    );
}

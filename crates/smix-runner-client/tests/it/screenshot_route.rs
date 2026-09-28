//! The runner can hand back a frame, and the bytes are the frame.
//!
//! `GET /screenshot` has been served by the iOS runner for a long time
//! with exactly one reader in the whole repository — a hand-written
//! synchronous socket in smix-capsule, reached only on a physical iPhone.
//! On a simulator, "take a picture" went out to device tooling instead,
//! which is a different process on a different clock from the one that
//! did the tapping.
//!
//! Three things have to hold before anything is built on it: the bytes
//! survive the trip, a runner that says it cannot take the picture is
//! believed, and a 200 carrying nothing is not mistaken for a picture of
//! nothing.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use smix_runner_client::HttpRunnerClient;

/// A PNG header plus a little tail — enough that "the bytes came back"
/// is a claim about bytes rather than about a length.
const FRAME: &[u8] = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR-and-then-some";

enum Reply {
    Png(&'static [u8]),
    Refusal,
    EmptyBody,
}

/// Bind an ephemeral port, answer one request, hand back the request line.
fn serve(reply: Reply) -> (u16, Arc<Mutex<String>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind ephemeral port");
    let port = listener.local_addr().expect("local addr").port();
    let heard = Arc::new(Mutex::new(String::new()));
    let write_down = Arc::clone(&heard);
    std::thread::spawn(move || {
        let Ok((mut sock, _)) = listener.accept() else {
            return;
        };
        let mut scratch = [0u8; 2048];
        let n = sock.read(&mut scratch).unwrap_or(0);
        *write_down.lock().expect("request log") = String::from_utf8_lossy(&scratch[..n])
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        match reply {
            Reply::Png(bytes) => {
                let _ = write!(
                    sock,
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    bytes.len()
                );
                let _ = sock.write_all(bytes);
            }
            Reply::Refusal => {
                let body = r#"{"ok":false,"error":"screenshot_unavailable","reason":"XCUIScreen returned no PNG"}"#;
                let _ = write!(
                    sock,
                    "HTTP/1.1 503 Service Unavailable\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                    body.len()
                );
            }
            Reply::EmptyBody => {
                let _ = write!(
                    sock,
                    "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                );
            }
        }
    });
    (port, heard)
}

fn client(port: u16) -> HttpRunnerClient {
    HttpRunnerClient::new(port)
}

#[tokio::test]
async fn the_bytes_that_come_back_are_the_frame() {
    let (port, heard) = serve(Reply::Png(FRAME));
    let png = client(port)
        .screenshot()
        .await
        .expect("a runner that answers with a PNG");
    assert_eq!(
        heard.lock().expect("request log").as_str(),
        "GET /screenshot HTTP/1.1",
        "the frame has to come from the runner's own route"
    );
    assert_eq!(
        png, FRAME,
        "asserting only that it did not error would pass for an \
         implementation that returns an empty vec"
    );
}

#[tokio::test]
async fn a_runner_that_cannot_take_the_picture_is_believed() {
    let (port, _heard) = serve(Reply::Refusal);
    let Err(e) = client(port).screenshot().await else {
        panic!("a 503 is the runner saying it has no frame, not a frame");
    };
    let said = e.to_string();
    assert!(
        said.contains("screenshot_unavailable"),
        "carry the runner's own word for it: {said}"
    );
    assert!(
        said.contains("XCUIScreen returned no PNG"),
        "carry the reason it gave: {said}"
    );
}

#[tokio::test]
async fn a_two_hundred_with_nothing_in_it_is_not_a_picture() {
    let (port, _heard) = serve(Reply::EmptyBody);
    assert!(
        client(port).screenshot().await.is_err(),
        "a zero-byte PNG makes every assertion downstream measure an \
         empty file, and none of them would say so"
    );
}

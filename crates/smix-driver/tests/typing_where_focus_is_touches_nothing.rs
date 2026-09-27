//! Typing into whatever holds focus sends the text and nothing else.
//!
//! An untargeted `inputText` on Android resolved "the focused element"
//! and tapped its centre before typing. On a consumer's number pad that
//! tap pressed the `1` key, and the reset code arrived as `112345`. The
//! caller named no field, so there is nothing to aim at: the runner
//! types into the focused editable node and reads it back on its own.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::sync::{Arc, Mutex};

use smix_driver::{AndroidDriver, Driver, HttpRunnerClient};
use smix_selector::{Selector, True};

/// A runner that answers `/input-text` and nothing else, and writes
/// down every path it was asked for.
fn runner_that_only_types() -> (u16, Arc<Mutex<Vec<String>>>) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    let asked = Arc::new(Mutex::new(Vec::new()));
    let log = Arc::clone(&asked);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(stream) = stream else { return };
            let log = Arc::clone(&log);
            std::thread::spawn(move || serve(stream, &log));
        }
    });
    (port, asked)
}

fn serve(stream: std::net::TcpStream, log: &Mutex<Vec<String>>) {
    let mut writer = stream.try_clone().expect("clone");
    let mut reader = BufReader::new(stream);
    loop {
        let mut request_line = String::new();
        if reader.read_line(&mut request_line).unwrap_or(0) == 0 {
            return;
        }
        let path = request_line
            .split_whitespace()
            .nth(1)
            .unwrap_or_default()
            .split('?')
            .next()
            .unwrap_or_default()
            .to_string();
        let mut length = 0usize;
        loop {
            let mut header = String::new();
            reader.read_line(&mut header).expect("header");
            if header == "\r\n" || header.is_empty() {
                break;
            }
            if let Some(v) = header.to_ascii_lowercase().strip_prefix("content-length:") {
                length = v.trim().parse().expect("length");
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).expect("body");
        log.lock().expect("log").push(path.clone());
        let (status, reply) = if path == "/input-text" {
            ("200 OK", r#"{"ok":true,"status":"ok"}"#)
        } else {
            ("404 Not Found", r#"{"ok":false}"#)
        };
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{reply}",
            reply.len()
        );
        if writer.write_all(response.as_bytes()).is_err() {
            return;
        }
    }
}

#[tokio::test]
async fn an_untargeted_fill_sends_the_text_and_touches_nothing() {
    let (port, asked) = runner_that_only_types();
    let driver = AndroidDriver::new(HttpRunnerClient::new(port));
    let focused = Selector::Focused {
        focused: True(true),
    };

    driver
        .fill(&focused, "123456", None, false)
        .await
        .expect("typing where focus is needs nothing but /input-text");

    let asked = asked.lock().expect("log").clone();
    assert_eq!(
        asked,
        vec!["/input-text".to_string()],
        "a fill that named no field sent more than the text — a tap here is \
         a tap somewhere nobody chose"
    );
}

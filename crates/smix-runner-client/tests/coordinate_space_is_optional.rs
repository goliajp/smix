//! Asking a runner about its coordinate spaces, including runners that
//! cannot answer.
//!
//! `GET /coordinate-space` is new. Every runner already in the field
//! answers it with a 404, and those runners drive apps perfectly well —
//! so a client that treats "this route is missing" as a failure would
//! break every working setup on the day the check shipped, in the name
//! of a check that had nothing to say about them.
//!
//! Missing is therefore its own answer, distinct from both "the spaces
//! agree" and "they do not". The caller must be able to tell it apart:
//! rolling it into either one is how a question quietly stops being
//! asked.

use smix_runner_client::HttpRunnerClient;
use std::io::{BufRead, BufReader, Write};
use std::net::TcpListener;
use std::thread;

/// A one-shot server that answers the first request with `response` and
/// exits. Enough to pin what the client does with a status code, and
/// nothing more — the runner's own behaviour is the runner's tests.
fn serve_once(response: &'static str) -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let port = listener.local_addr().expect("addr").port();
    thread::spawn(move || {
        if let Ok((mut stream, _)) = listener.accept() {
            let mut reader = BufReader::new(stream.try_clone().expect("clone"));
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 0 {
                if line == "\r\n" || line == "\n" {
                    break;
                }
                line.clear();
            }
            let _ = stream.write_all(response.as_bytes());
            let _ = stream.flush();
        }
    });
    port
}

fn body(json: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{}",
        json.len(),
        json
    )
}

const DISAGREEING: &str = concat!(
    r#"{"appFrame":{"x":0,"y":0,"w":874,"h":402},"#,
    r#""snapshotRootFrame":{"x":0,"y":0,"w":874,"h":402},"#,
    r#""deviceOrientation":"portrait","eventRecordOrientation":"portrait","#,
    r#""spacesAgree":false,"nx":0.5,"ny":0.5,"#,
    r#""resolvedPoint":{"x":437,"y":201}}"#
);

#[tokio::test]
async fn a_runner_that_answers_reports_both_spaces() {
    let port = serve_once(Box::leak(body(DISAGREEING).into_boxed_str()));
    let client = HttpRunnerClient::new(port);

    let space = client
        .coordinate_space(0.5, 0.5)
        .await
        .expect("the request itself must succeed")
        .expect("a runner that answers 200 has an answer");

    assert_eq!(space.app_frame.w, 874.0);
    assert_eq!(space.snapshot_root_frame.h, 402.0);
    assert_eq!(space.event_record_orientation, "portrait");
    assert!(!space.spaces_agree);
}

#[tokio::test]
async fn a_runner_without_the_route_says_it_cannot_tell() {
    let port = serve_once("HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n");
    let client = HttpRunnerClient::new(port);

    let answer = client
        .coordinate_space(0.5, 0.5)
        .await
        .expect("a missing route is not a transport failure — every older runner 404s here");

    assert!(
        answer.is_none(),
        "a 404 must be `None`, distinguishable from a runner that answered — \
         folding it into either verdict makes an unasked question look answered"
    );
}

//! Why a request that went out came back with nothing.
//!
//! reqwest prints every one of these as "error sending request for url",
//! which is the same sentence for the host giving up on a slow runner and
//! for the runner hanging up mid-request. The two send a reader in
//! different directions — a busy device, or a runner that went away — so
//! the answer names which, and carries the causes underneath.

use std::error::Error as _;

impl crate::RunnerTransportError {
    /// For a request that went out unanswered: `Some(true)` when the host
    /// stopped waiting, `Some(false)` when the connection closed first.
    /// `None` for every other failure.
    pub fn unanswered_because_the_host_stopped_waiting(&self) -> Option<bool> {
        match self {
            Self::SentWithoutAnswer { source, .. } => Some(source.is_timeout()),
            _ => None,
        }
    }
}

/// The reason, in words, for a request that was sent and not answered.
pub(crate) fn why(e: &reqwest::Error) -> String {
    let causes = causes(e);
    if e.is_timeout() {
        format!("the host stopped waiting before the runner answered ({causes})")
    } else {
        format!("the connection closed before an answer came ({causes})")
    }
}

/// Every cause under `e`, outermost first, joined — reqwest's own line
/// stops at the first.
pub(crate) fn causes(e: &reqwest::Error) -> String {
    let mut parts = vec![e.to_string()];
    let mut next = e.source();
    while let Some(cause) = next {
        let text = cause.to_string();
        if parts.last() != Some(&text) {
            parts.push(text);
        }
        next = cause.source();
    }
    parts.join(": ")
}

#[cfg(test)]
mod tests {
    use std::io::Read;
    use std::net::TcpListener;
    use std::time::Duration;

    use super::*;

    /// A loopback server that reads the request, then does `then`.
    fn server(then: fn(std::net::TcpStream)) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
        let port = listener.local_addr().expect("addr").port();
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut s) = stream else { continue };
                let mut chunk = [0u8; 4096];
                let _ = s.read(&mut chunk);
                then(s);
            }
        });
        port
    }

    async fn post(port: u16, timeout: Duration) -> reqwest::Error {
        reqwest::Client::builder()
            .pool_max_idle_per_host(0)
            .timeout(timeout)
            .build()
            .expect("client")
            .post(format!("http://127.0.0.1:{port}/hide-keyboard"))
            .body("{}")
            .send()
            .await
            .expect_err("nothing answered")
    }

    #[tokio::test]
    async fn a_runner_too_slow_to_answer_is_named_as_the_host_giving_up() {
        let port = server(|s| {
            std::thread::sleep(Duration::from_secs(3));
            drop(s);
        });
        let e = post(port, Duration::from_millis(300)).await;
        let said = why(&e);
        assert!(said.starts_with("the host stopped waiting"), "{said}");
    }

    #[tokio::test]
    async fn a_runner_that_hangs_up_is_named_as_the_connection_closing() {
        let port = server(drop);
        let e = post(port, Duration::from_secs(10)).await;
        let said = why(&e);
        assert!(said.starts_with("the connection closed"), "{said}");
        // the cause under reqwest's own sentence is carried, not dropped
        assert!(said.matches(": ").count() >= 1, "{said}");
    }
}

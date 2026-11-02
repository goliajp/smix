//! A log source that is a running program: each line it prints is an entry.
//!
//! An app's own log on a device is read this way — `log stream` on an iOS
//! simulator, `logcat` on an Android device — live, from the moment the
//! subscriber starts. A query of the log after the fact (`log show`) lags
//! the write by seconds, which is why a flow that asked for a line six
//! seconds after it was written was told it had never been said.

use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::watch;

use crate::MetroLogTail;
use crate::subscriber::{SubscriberHandle, parse_log_line};

pub struct ProcessSubscriber;

impl ProcessSubscriber {
    /// Run `program args…` and push each line of its standard output into
    /// `tail`. The program is killed when the handle is shut down or
    /// dropped. A program that cannot start leaves the tail empty and says
    /// so on standard error.
    pub fn start(program: String, args: Vec<String>, tail: MetroLogTail) -> SubscriberHandle {
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let join = tokio::spawn(async move {
            let child = Command::new(&program)
                .args(&args)
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::null())
                .kill_on_drop(true)
                .spawn();
            let mut child = match child {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("WARN: log source `{program}` did not start: {e}");
                    return;
                }
            };
            let Some(out) = child.stdout.take() else {
                return;
            };
            let mut lines = BufReader::new(out).lines();
            loop {
                tokio::select! {
                    line = lines.next_line() => match line {
                        Ok(Some(l)) => {
                            let (level, message) = parse_log_line(&l);
                            if !message.is_empty() {
                                tail.push(level, message);
                            }
                        }
                        _ => return,
                    },
                    _ = shutdown_rx.changed() => {
                        let _ = child.kill().await;
                        return;
                    }
                }
            }
        });
        SubscriberHandle::new(shutdown_tx, join)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SignalMatcher, Window};
    use std::time::Duration;

    #[tokio::test]
    async fn each_line_a_program_prints_is_an_entry_a_signal_can_match() {
        let tail = MetroLogTail::new();
        let handle = ProcessSubscriber::start(
            "/bin/sh".into(),
            vec![
                "-c".into(),
                "echo starting; sleep 0.2; echo 'Insight: dev-mutate handled: push'; sleep 5"
                    .into(),
            ],
            tail.clone(),
        );
        let m = SignalMatcher::new("dev-mutate handled: push", None).unwrap();
        let hit = tail
            .await_signal(&m, Duration::from_secs(3), Window::SinceRun)
            .await
            .expect("the line was matched");
        assert!(hit.message.contains("handled"), "{}", hit.message);
        handle.shutdown();
    }

    #[tokio::test]
    async fn a_program_that_cannot_start_leaves_the_tail_empty() {
        let tail = MetroLogTail::new();
        let _h = ProcessSubscriber::start(
            "smix-test-program-that-does-not-exist".into(),
            vec![],
            tail.clone(),
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
        assert!(tail.snapshot().is_empty());
    }
}

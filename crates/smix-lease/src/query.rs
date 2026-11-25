//! Running a host tool that only reports, such as `ps` or `pgrep`, with a
//! bound on how long its answer may take.
//!
//! These answer in milliseconds. On a Linux host where reading another
//! process's `/proc` entry blocks (a task stuck in uninterruptible sleep),
//! `ps` blocks with it and never returns, and so did every smix command and
//! test that asked it. A child that hands its stdout to a process of its own
//! that outlives it keeps the pipe open the same way.

use std::io::{self, Read};
use std::process::{Command, Output, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

/// How long a query may take before it is given up.
pub const QUERY_LIMIT: Duration = Duration::from_secs(10);

/// [`Command::output`] for a query, given up after [`QUERY_LIMIT`]: the
/// child is killed, a warning naming it goes to stderr, and the error is
/// [`io::ErrorKind::TimedOut`]. Callers treat that as they treat the tool
/// failing to start.
pub fn query(cmd: &mut Command) -> io::Result<Output> {
    query_within(cmd, QUERY_LIMIT)
}

fn query_within(cmd: &mut Command, limit: Duration) -> io::Result<Output> {
    let deadline = Instant::now() + limit;
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = drain(child.stdout.take());
    let stderr = drain(child.stderr.take());
    let mut pause = Duration::from_millis(1);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break Some(status);
        }
        if Instant::now() >= deadline {
            break None;
        }
        std::thread::sleep(pause.min(deadline.saturating_duration_since(Instant::now())));
        pause = (pause * 2).min(Duration::from_millis(20));
    };
    let read = |rx: &mpsc::Receiver<Vec<u8>>| {
        rx.recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()
    };
    if let Some(status) = status
        && let (Some(stdout), Some(stderr)) = (read(&stdout), read(&stderr))
    {
        return Ok(Output {
            status,
            stdout,
            stderr,
        });
    }
    let _ = child.kill();
    // reaped off this thread: a child stuck in the kernel dies only when
    // it leaves it
    std::thread::spawn(move || child.wait());
    let what = format!("{cmd:?}");
    eprintln!(
        "warning: {what} gave no answer within {} s; going on without it",
        limit.as_secs_f32()
    );
    Err(io::Error::new(
        io::ErrorKind::TimedOut,
        format!("{what} gave no answer within {limit:?}"),
    ))
}

fn drain(pipe: Option<impl Read + Send + 'static>) -> mpsc::Receiver<Vec<u8>> {
    let (tx, rx) = mpsc::channel();
    let mut pipe = pipe.expect("piped");
    std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = pipe.read_to_end(&mut buf);
        let _ = tx.send(buf);
    });
    rx
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_answer_comes_back_whole() {
        let out = query_within(
            Command::new("sh").args(["-c", "echo out; echo err >&2; exit 3"]),
            Duration::from_secs(10),
        )
        .expect("answers");
        assert_eq!(out.stdout, b"out\n");
        assert_eq!(out.stderr, b"err\n");
        assert_eq!(out.status.code(), Some(3));
    }

    #[test]
    fn a_tool_that_does_not_answer_is_given_up() {
        let t = Instant::now();
        let e = query_within(Command::new("sleep").arg("5"), Duration::from_millis(300))
            .expect_err("gave up");
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
        assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    }

    #[test]
    fn a_pipe_held_open_by_what_the_tool_left_behind_is_given_up() {
        // sh exits at once; the sleep it started keeps stdout open
        let t = Instant::now();
        let e = query_within(
            Command::new("sh").args(["-c", "sleep 5 & echo started"]),
            Duration::from_millis(300),
        )
        .expect_err("gave up");
        assert_eq!(e.kind(), io::ErrorKind::TimedOut);
        assert!(t.elapsed() < Duration::from_secs(5), "{:?}", t.elapsed());
    }
}
